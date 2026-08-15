
// SDL2 + glow + Dear ImGui window for live-tuning the audio engine's
// globals/voice params and driving the mock Hydra backend without a
// keyboard. SDL2/AppKit requires the window + event loop on the main thread
// on macOS, so this owns main() when --gui is passed (see main.rs). SDL2's
// native KMSDRM video driver (selectable via SDL_VIDEODRIVER) is what lets
// the same binary run windowed on the dev machine and boot straight to the
// performance box's panel with no X server.

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use fundsp::shared::Shared;
use sdl2::event::{Event, WindowEvent};

use crate::hydra::MockControls;
use crate::audio::{AudioHandles, GrowlHandle, BasicHandle, SwarmHandle, GrowlParams, BasicParams, SwarmParams, VoiceParams, snapshot, ReeseHandle, ReeseParams};
use crate::tools::AtomicF32;
use crate::zgicabra::{SignalOverride, ZgicabraBridge};

const VOICE_NAMES: [&str; 4] = ["Reese", "Growl", "Basic", "Swarm"];

// GUI-thread-local browser state for saved voice-param snapshots.
struct SnapshotBrowser {
    names: Vec<String>,
    index: usize,
}

impl SnapshotBrowser {
    fn new () -> SnapshotBrowser {
        let mut browser = SnapshotBrowser { names: Vec::new(), index: 0 };
        browser.refresh();
        browser
    }

    fn refresh (&mut self) {
        self.names = snapshot::list_snapshots().unwrap_or_default();
        if self.index >= self.names.len() {
            self.index = self.names.len().saturating_sub(1);
        }
    }
}

// Drag widgets need a step size that feels right whether the underlying
// range is 0..1 or 100..14000 -- scale it off the row's own lo/hi span.
fn drag_speed (lo: f32, hi: f32) -> f32 {
    ((hi - lo).abs() / 200.0).max(0.0001)
}

fn toggle_checkbox (ui: &imgui::Ui, label: &str, flag: &Arc<AtomicBool>) {
    let mut value = flag.load(Ordering::Relaxed);
    if ui.checkbox(label, &mut value) {
        flag.store(value, Ordering::Relaxed);
    }
}

// Bound to a bare Shared: checked when value >= 1.0. Used both for plain
// enabled/disabled flags and for the *_bypass cells (in which case "checked"
// reads as "bypass is on" -- the label passed in says which).
fn level_checkbox (ui: &imgui::Ui, label: &str, level: &Shared) {
    let mut enabled = level.value() >= 1.0;
    if ui.checkbox(label, &mut enabled) {
        level.set_value(if enabled { 1.0 } else { 0.0 });
    }
}

// A draggable 2D pad: click/drag anywhere inside to set (x, y) in [-1, 1],
// y flipped so up is positive (matches joystick_y convention elsewhere).
// `twist` is the wand's raw rot_quat[2] (not an angle -- doubled here the
// same way ui.rs's draw_wand does, to spin the crosshair) -- imgui has no
// rotated-primitive support, so the line endpoints are rotated by hand.
fn xy_pad (ui: &imgui::Ui, id: &str, size: f32, twist: f32, x: &Arc<AtomicF32>, y: &Arc<AtomicF32>) {
    let origin = ui.cursor_screen_pos();
    ui.invisible_button(id, [size, size]);

    let active  = ui.is_item_active();
    let hovered = ui.is_item_hovered();

    if active {
        let mouse = ui.io().mouse_pos;
        let nx = ((mouse[0] - origin[0]) / size * 2.0 - 1.0).clamp(-1.0, 1.0);
        let ny = (1.0 - (mouse[1] - origin[1]) / size * 2.0).clamp(-1.0, 1.0);
        x.store(nx);
        y.store(ny);
    }

    let draw_list = ui.get_window_draw_list();
    let center = [origin[0] + size/2.0, origin[1] + size/2.0];

    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_circle(center, size/2.0, bg).filled(true).build();
    draw_list.add_circle(center, size/2.0, [0.5, 0.5, 0.55, 1.0]).build();

    let (s, c) = (twist * 2.0).sin_cos();
    let half = size * 0.5;
    let spoke = |lx: f32, ly: f32| [center[0] + lx * c - ly * s, center[1] + lx * s + ly * c];
    draw_list.add_line(spoke(-half, 0.0), spoke(half, 0.0), [0.4, 0.4, 0.45, 1.0]).build();
    draw_list.add_line(spoke(0.0, -half), spoke(0.0, half), [0.4, 0.4, 0.45, 1.0]).build();

    // Joystick offset is in the wand's rotated frame too, so twisting the
    // wand carries the stick around with the crosshair.
    let dot = spoke(x.load() * half, -y.load() * half);
    draw_list.add_line(center, dot, [0.95, 0.80, 0.30, 1.0]).thickness(2.0).build();
    draw_list.add_circle(dot, 5.0, [0.95, 0.80, 0.30, 1.0]).filled(true).build();
}

// A rotary knob: click and drag vertically to change value within [lo, hi]
// (drag up = increase). Uses drag *delta*, not absolute mouse position --
// an absolute-angle knob would make fine adjustment impossible.
fn knob (ui: &imgui::Ui, id: &str, label: &str, radius: f32, lo: f32, hi: f32, value: &mut f32) -> bool {
    let origin = ui.cursor_screen_pos();
    let center = [origin[0] + radius, origin[1] + radius];
    ui.invisible_button(id, [radius * 2.0, radius * 2.0]);

    let active  = ui.is_item_active();
    let hovered = ui.is_item_hovered();
    let mut changed = false;

    if active {
        let delta = ui.io().mouse_delta[1];
        if delta != 0.0 {
            let speed = drag_speed(lo, hi) * 5.0;
            *value = (*value - delta * speed).clamp(lo, hi);
            changed = true;
        }
    }

    let draw_list = ui.get_window_draw_list();
    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_circle(center, radius, bg).filled(true).build();
    draw_list.add_circle(center, radius, [0.5, 0.5, 0.55, 1.0]).build();

    // Sweep -135deg..+135deg (gap at the bottom), 0 = straight down.
    let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let angle = -std::f32::consts::PI * 0.75 + t * std::f32::consts::PI * 1.5;
    let tip = [center[0] + angle.sin() * radius * 0.85, center[1] - angle.cos() * radius * 0.85];
    draw_list.add_line(center, tip, [0.95, 0.80, 0.30, 1.0]).thickness(2.0).build();

    if !label.is_empty() { ui.text(label); }
    changed
}

// Reads/writes a bare Shared through a knob, ranged to (lo, hi).
fn draw_shared_knob (ui: &imgui::Ui, id: &str, label: &str, lo: f32, hi: f32, cell: &Shared) {
    let mut value = cell.value();
    if knob(ui, id, label, KNOB_RADIUS, lo, hi, &mut value) {
        cell.set_value(value);
    }
}

// A vertical fader: click/drag anywhere inside to set value directly off the
// mouse's absolute height in the track (top = hi, bottom = lo) -- a fader's
// handle height *is* the value, unlike knob()'s relative drag. `origin` is
// where the fill starts from (lo, or a signed range's midpoint so it grows
// from center, e.g. bend).
fn vslider (ui: &imgui::Ui, id: &str, label: &str, size: [f32; 2], lo: f32, hi: f32, origin: f32, value: &mut f32) -> bool {
    let [w, h] = size;
    let p_min = ui.cursor_screen_pos();
    ui.invisible_button(id, size);

    let active  = ui.is_item_active();
    let hovered = ui.is_item_hovered();
    let mut changed = false;

    if active {
        let mouse_y = ui.io().mouse_pos[1];
        let t = (1.0 - (mouse_y - p_min[1]) / h).clamp(0.0, 1.0);
        let new_value = lo + t * (hi - lo);
        if new_value != *value {
            *value = new_value;
            changed = true;
        }
    }

    let p_max = [p_min[0] + w, p_min[1] + h];
    let draw_list = ui.get_window_draw_list();
    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_rect(p_min, p_max, bg).filled(true).build();
    draw_list.add_rect(p_min, p_max, [0.5, 0.5, 0.55, 1.0]).build();

    let t_val    = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let t_origin = ((origin  - lo) / (hi - lo)).clamp(0.0, 1.0);
    let y_val    = p_max[1] - t_val * h;
    let y_origin = p_max[1] - t_origin * h;
    let (fill_top, fill_bottom) = if y_val < y_origin { (y_val, y_origin) } else { (y_origin, y_val) };
    draw_list.add_rect([p_min[0], fill_top], [p_max[0], fill_bottom], [0.95, 0.80, 0.30, 1.0]).filled(true).build();
    draw_list.add_line([p_min[0], y_val], [p_max[0], y_val], [1.0, 1.0, 1.0, 1.0]).thickness(2.0).build();

    if !label.is_empty() { ui.text(label); }
    changed
}

const KNOB_RADIUS: f32 = 8.0;
const VSLIDER_SIZE: [f32; 2] = [24.0, 90.0];
// Same width as the trigger hslider (see draw_wand_mock) so it lines up
// atop the signal column between the two wands' trigger rows.
const HCENTER_SIZE: [f32; 2] = [120.0, 18.0];

// A horizontal fader for the analog trigger (continuous, matching the
// hardware's real depth reading -- see hydra/hid.rs's TRIG_SCALE). `mirrored`
// flips which edge is "pulled" so both wands' fills grow toward the middle.
fn hslider (ui: &imgui::Ui, id: &str, size: [f32; 2], mirrored: bool, value: &Arc<AtomicF32>) -> bool {
    let [w, h] = size;
    let p_min = ui.cursor_screen_pos();
    ui.invisible_button(id, size);

    let active  = ui.is_item_active();
    let hovered = ui.is_item_hovered();
    let mut changed = false;

    if active {
        let mouse_x = ui.io().mouse_pos[0];
        let t = ((mouse_x - p_min[0]) / w).clamp(0.0, 1.0);
        let t = if mirrored { 1.0 - t } else { t };
        if t != value.load() {
            value.store(t);
            changed = true;
        }
    }

    let p_max = [p_min[0] + w, p_min[1] + h];
    let draw_list = ui.get_window_draw_list();
    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_rect(p_min, p_max, bg).filled(true).build();
    draw_list.add_rect(p_min, p_max, [0.5, 0.5, 0.55, 1.0]).build();

    let t = value.load().clamp(0.0, 1.0);
    let (fill_from, fill_to) = if mirrored { (p_max[0] - t * w, p_max[0]) } else { (p_min[0], p_min[0] + t * w) };
    draw_list.add_rect([fill_from, p_min[1]], [fill_to, p_max[1]], [0.95, 0.80, 0.30, 1.0]).filled(true).build();

    changed
}

// A double-ended horizontal fader: fills outward from center in both
// directions -- drag position maps to distance from center either way.
fn hslider_center (ui: &imgui::Ui, id: &str, size: [f32; 2], value: &mut f32) -> bool {
    let [w, h] = size;
    let p_min = ui.cursor_screen_pos();
    ui.invisible_button(id, size);

    let active  = ui.is_item_active();
    let hovered = ui.is_item_hovered();
    let mut changed = false;

    if active {
        let mouse_x = ui.io().mouse_pos[0];
        let t = ((mouse_x - p_min[0]) / w).clamp(0.0, 1.0);
        let new_value = ((t - 0.5).abs() * 2.0).clamp(0.0, 1.0);
        if new_value != *value {
            *value = new_value;
            changed = true;
        }
    }

    let p_max = [p_min[0] + w, p_min[1] + h];
    let draw_list = ui.get_window_draw_list();
    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_rect(p_min, p_max, bg).filled(true).build();
    draw_list.add_rect(p_min, p_max, [0.5, 0.5, 0.55, 1.0]).build();

    let center_x = (p_min[0] + p_max[0]) * 0.5;
    let half = value.clamp(0.0, 1.0) * 0.5 * w;
    draw_list.add_rect([center_x - half, p_min[1]], [center_x + half, p_max[1]], [0.95, 0.80, 0.30, 1.0]).filled(true).build();
    draw_list.add_line([center_x, p_min[1]], [center_x, p_max[1]], [1.0, 1.0, 1.0, 1.0]).build();
    changed
}
// Simple cards (fixed knob set) vs. the voice card, sized for a selector
// row plus up to 4 param knobs.
const CARD_SIZE:       [f32; 2] = [140.0, 66.0];
const VOICE_CARD_SIZE: [f32; 2] = [160.0, 140.0];
// Reese exposes 8 knobs (2 rows of 4) instead of the usual single row.
const REESE_CARD_SIZE: [f32; 2] = [160.0, 190.0];

const ACTIVE_VOICE_BG: [f32; 4] = [0.10, 0.16, 0.42, 1.0];

// Bordered box with a title and an optional top-right bypass checkbox --
// the Engine panel's repeated "module card" shape. `active` tints the
// background (used by draw_voice_card to mark the selected voice).
fn draw_module_card (ui: &imgui::Ui, title: &str, bypass: Option<&Shared>, size: [f32; 2], active: bool, body: impl FnOnce(&imgui::Ui)) {
    let _bg = active.then(|| ui.push_style_color(imgui::StyleColor::ChildBg, ACTIVE_VOICE_BG));
    ui.child_window(format!("##card_{title}")).size(size).border(true).build(|| {
        ui.text(title);
        if let Some(level) = bypass {
            ui.same_line_with_pos(size[0] - 28.0);
            level_checkbox(ui, "##bypass", level);
        }
        ui.separator();
        body(ui);
    });
}

fn draw_knob_row (ui: &imgui::Ui, knobs: &[(&str, &str, f32, f32, &Shared)]) {
    for (i, (id, label, lo, hi, cell)) in knobs.iter().enumerate() {
        if i > 0 { ui.same_line(); }
        ui.group(|| draw_shared_knob(ui, &format!("##{id}"), label, *lo, *hi, cell));
    }
}

fn draw_voice_selector (ui: &imgui::Ui, selected: &Shared) {
    let index = selected.value() as i32;
    let name = VOICE_NAMES.get(index as usize).copied().unwrap_or("?");
    ui.text(format!("Voice: {name}"));
    if ui.button("< Voice") {
        selected.set_value((index - 1).rem_euclid(VOICE_NAMES.len() as i32) as f32);
    }
    ui.same_line();
    if ui.button("Voice >") {
        selected.set_value((index + 1).rem_euclid(VOICE_NAMES.len() as i32) as f32);
    }
}

fn draw_voice_growl (ui: &imgui::Ui, growl: &GrowlHandle) {
    draw_knob_row(ui, &[
        ("growl_bass_drive", "bass drive", 0.0, 1.0, &growl.bass_drive),
        ("growl_filter",     "filter",     0.0, 1.0, &growl.filter),
        ("growl_space",      "space",      0.0, 1.0, &growl.space),
        ("growl_warp",       "warp",       0.0, 1.0, &growl.warp),
        ("growl_nam_xover",  "xover",      0.0, 2000.0, &growl.nam_crossover),
    ]);
}

fn draw_voice_basic (ui: &imgui::Ui, basic: &BasicHandle) {
    draw_knob_row(ui, &[
        ("basic_sin",    "sin",    0.0, 1.0, &basic.sin_level),
        ("basic_tri",    "tri",    0.0, 1.0, &basic.tri_level),
        ("basic_square", "square", 0.0, 1.0, &basic.square_level),
        ("basic_saw",    "saw",    0.0, 1.0, &basic.saw_level),
        ("basic_sat",    "sat",    1.0, 10.0, &basic.saturation),
    ]);
}

fn draw_voice_reese (ui: &imgui::Ui, reese: &ReeseHandle) {
    draw_knob_row(ui, &[
        ("reese_detune",    "detune",  0.0,  50.0, &reese.detune),
        ("reese_sub",       "sub",     0.0,  1.0,  &reese.sub_level),
        ("reese_drive",     "drive",   1.0,  8.0,  &reese.drive),
        ("reese_cutoff",    "cutoff",  0.0,  1.0,  &reese.cutoff),
    ]);
    draw_knob_row(ui, &[
        ("reese_res",       "res",     0.3,  3.0,  &reese.resonance),
        ("reese_lfo_rate",  "lfo rate", 0.05, 3.0,  &reese.lfo_rate),
        ("reese_lfo_depth", "lfo dep", 0.0,  1.0,  &reese.lfo_depth),
        ("reese_width",     "width",   0.0,  1.0,  &reese.width),
    ]);
}


fn draw_voice_swarm (ui: &imgui::Ui, handle: &SwarmHandle) {
    draw_knob_row(ui, &[
        ("swarm_chase",  "chase",     0.5, 1.0,    &handle.chase_factor),
        ("swarm_radius", "radius",    0.0, 5.0,    &handle.radius),
        ("swarm_speed",  "speed",     0.0, 1.0,    &handle.orbit_speed),
        ("swarm_warp",   "warp",      0.0, 1.0,    &handle.phaser_depth),
        ("swarm_xover",  "xover",     0.0, 2000.0, &handle.xover_freq),
    ]);

    if ui.button("< ##swarm_nam_lo") { handle.nam_lo.cycle(-1); }
    ui.same_line();
    ui.text(format!("{}", handle.nam_lo.selected_name()));
    ui.same_line();
    if ui.button("> ##swarm_nam_lo") { handle.nam_lo.cycle(1); }

    if ui.button("< ##swarm_nam_hi") { handle.nam_hi.cycle(-1); }
    ui.same_line();
    ui.text(format!("{}", handle.nam_hi.selected_name()));
    ui.same_line();
    if ui.button("> ##swarm_nam_hi") { handle.nam_hi.cycle(1); }
}

// All 4 voices drawn side by side; the active one (voice_selected) gets a
// dark blue card background.
fn draw_voice_card (ui: &imgui::Ui, audio: &AudioHandles) {
    draw_voice_selector(ui, &audio.voice_selected);
    ui.separator();

    let selected = audio.voice_selected.value() as i32;

    draw_module_card(ui, "Reese", None, REESE_CARD_SIZE, selected == 0, |ui| {
        draw_voice_reese(ui, &audio.voice_a);
    });
    ui.same_line();
    draw_module_card(ui, "Growl", None, VOICE_CARD_SIZE, selected == 1, |ui| {
        draw_voice_growl(ui, &audio.voice_b);
    });
    ui.same_line();
    draw_module_card(ui, "Basic", None, VOICE_CARD_SIZE, selected == 2, |ui| {
        draw_voice_basic(ui, &audio.voice_c);
    });
    ui.same_line();
    draw_module_card(ui, "Blank", None, VOICE_CARD_SIZE, selected == 3, |ui| {
        ui.text("(no patch -- silent)");
    });
}

// Save/load buttons for the currently-selected voice's live params, plus a
// cycler over every snapshot found on disk -- each save writes a new
// "{voice_name}_{index}.snap" file rather than overwriting, see
// audio::snapshot.
fn draw_snapshot_browser (ui: &imgui::Ui, audio: &AudioHandles, browser: &mut SnapshotBrowser) {
    if ui.button("Save Snapshot") {
        let result = match audio.voice_selected.value() as i32 {
            0 => snapshot::save_snapshot(ReeseParams::voice_name(), &audio.voice_a.params().fields()),
            1 => snapshot::save_snapshot(GrowlParams::voice_name(), &audio.voice_b.params().fields()),
            2 => snapshot::save_snapshot(BasicParams::voice_name(), &audio.voice_c.params().fields()),
            _ => Ok(std::path::PathBuf::new()),
        };
        if let Err(e) = result {
            eprintln!("║ 🟥 Failed to save snapshot: {e}");
        }
        browser.refresh();
        browser.index = browser.names.len().saturating_sub(1);
    }
    ui.same_line();

    match browser.names.get(browser.index) {
        Some(name) => ui.text(format!("Snapshot: {name}")),
        None       => ui.text("Snapshot: (none saved)"),
    }

    if ui.button("< Snap") && !browser.names.is_empty() {
        browser.index = (browser.index + browser.names.len() - 1) % browser.names.len();
    }
    ui.same_line();
    if ui.button("Snap >") && !browser.names.is_empty() {
        browser.index = (browser.index + 1) % browser.names.len();
    }
    ui.same_line();
    if ui.button("Load Snapshot") {
        if let Some(name) = browser.names.get(browser.index) {
            let path = snapshot::snapshot_path(name);
            match snapshot::load_snapshot(&path) {
                Ok((voice_name, fields)) => {
                    let expected = match audio.voice_selected.value() as i32 {
                        0 => ReeseParams::voice_name(),
                        1 => GrowlParams::voice_name(),
                        2 => BasicParams::voice_name(),
                        _ => "",
                    };
                    if voice_name != expected {
                        eprintln!("║ 🟥 Snapshot '{name}' is for voice '{voice_name}', not the selected voice");
                    } else {
                        match audio.voice_selected.value() as i32 {
                            0 => audio.voice_a.load(&ReeseParams::from_fields(&fields)),
                            1 => audio.voice_b.load(&GrowlParams::from_fields(&fields)),
                            2 => audio.voice_c.load(&BasicParams::from_fields(&fields)),
                            _ => {},
                        }
                    }
                },
                Err(e) => eprintln!("║ 🟥 Failed to load snapshot: {e}"),
            }
        }
    }
}

// One draggable row for a SignalState field: dragging the fader takes the
// field over from whatever normally computes it (field.set() flips
// `enabled` -- see ZgicabraBridge::sync), no separate override switch needed.
fn signal_override_row (ui: &imgui::Ui, label: &str, lo: f32, hi: f32, origin: f32, field: &SignalOverride) {
    ui.group(|| {
        let mut value = field.value.load();
        if vslider(ui, &format!("##{label}_value"), label, VSLIDER_SIZE, lo, hi, origin, &mut value) {
            field.set(value);
        }
    });
}

// Same "drag takes it over" convention as signal_override_row, driving a
// center-out hslider instead of a vslider.
fn signal_override_row_center (ui: &imgui::Ui, label: &str, size: [f32; 2], field: &SignalOverride) {
    ui.group(|| {
        let mut value = field.value.load();
        if hslider_center(ui, &format!("##{label}_value"), size, &mut value) {
            field.set(value);
        }
    });
}

// W (double-ended, center-out) sits above 4 vertical faders (F/B/Z/T).
// velocity/acceleration/jerk are readouts, not hand-ridden, so dropped.
fn draw_signal_state (ui: &imgui::Ui, bridge: &ZgicabraBridge) {
    signal_override_row_center(ui, "W", HCENTER_SIZE, &bridge.width);

    let rows: [(&str, f32, f32, f32, &SignalOverride); 4] = [
        ("F", 0.0,  1.0, 0.0, &bridge.filter),
        ("B", -1.0, 1.0, 0.0, &bridge.bend),
        ("Z", 0.0,  1.0, 0.0, &bridge.fuzz),
        ("T", 0.0,  1.0, 0.0, &bridge.thump),
    ];
    for (i, (label, lo, hi, origin, field)) in rows.into_iter().enumerate() {
        if i > 0 { ui.same_line(); }
        signal_override_row(ui, label, lo, hi, origin, field);
    }
}

// Engine panel: globals first (main sub, dry sub, amp, reverb, limiter,
// voice selector), then the currently-selected voice's own param card.
fn draw_engine_panel (ui: &imgui::Ui, audio: &AudioHandles, snapshot_browser: &mut SnapshotBrowser) {
    draw_module_card(ui, "Main Sub", None, CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[
            ("main_sub_lvl", "level", 0.0, 1.0, &audio.main_sub_lvl),
        ]);
    });
    ui.same_line();
    draw_module_card(ui, "Dry Sub", None, CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[("dry_sub_lvl", "level", 0.0, 1.0, &audio.dry_sub_lvl)]);
    });
    ui.same_line();
    draw_module_card(ui, "Thump", None, CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[
            ("thump_peak",  "peak",  0.0,  1.5, &audio.thump_peak),
            ("thump_decay", "decay", 0.02, 1.0, &audio.thump_decay),
        ]);
    });

    draw_module_card(ui, "Amp", Some(&audio.amp_bypass), CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[
            ("amp_blend",     "blend", 0.0, 1.0,    &audio.amp_blend),
            ("amp_boost",     "boost", 1.0, 4.0,    &audio.amp_boost),
            ("amp_crossover", "xover", 0.0, 2000.0, &audio.amp_crossover),
        ]);
    });
    ui.same_line();
    // reverb_decay/damp/size are baked into the reverb tail at construction
    // time -- editing them here only takes effect on the next restart.
    draw_module_card(ui, "Reverb", Some(&audio.reverb_bypass), CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[
            ("reverb_size",  "size",  10.0, 30.0, &audio.reverb_size),
            ("reverb_decay", "decay", 0.1,  4.0,  &audio.reverb_decay),
            ("reverb_damp",  "damp",  0.0,  1.0,  &audio.reverb_damp),
            ("reverb_dry",   "dry",   0.0,  1.0,  &audio.reverb_dry),
        ]);
    });
    ui.same_line();
    draw_module_card(ui, "Limiter", Some(&audio.limiter_bypass), CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[("limiter_thresh", "thresh", -60.0, 0.0, &audio.limiter_thresh)]);
    });
    ui.same_line();
    draw_module_card(ui, "Master", None, CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[("master_vol", "volume", 0.0, 1.0, &audio.master_vol)]);
    });

    ui.separator();
    draw_voice_card(ui, audio);
    ui.separator();
    draw_snapshot_browser(ui, audio, snapshot_browser);
}

fn draw_wand_mock (ui: &imgui::Ui, label: &str, mirrored: bool, twist: f32, trigger: &Arc<AtomicF32>, stick_x: &Arc<AtomicF32>, stick_y: &Arc<AtomicF32>, buttons: &[Arc<AtomicBool>; 4]) {
    hslider(ui, &format!("##{label}_trigger"), [120.0, 18.0], mirrored, trigger);
    xy_pad(ui, &format!("##{label}_stick"), 120.0, twist, stick_x, stick_y);

    for (i, button) in buttons.iter().enumerate() {
        if i > 0 { ui.same_line(); }
        toggle_checkbox(ui, &format!("##{label}_{}_btn", i + 1), button);
    }
}

fn draw_wand_rotation (ui: &imgui::Ui, rot: &[Arc<AtomicF32>; 4]) {
    ui.text(format!(
        "[{:.3} {:.3} {:.3} {:.3}]",
        rot[0].load(), rot[1].load(), rot[2].load(), rot[3].load(),
    ));
}

// Hydra: left wand | signal-override knob grid | right wand, with the
// sine-drift/tune toggles as one bottom row.
fn draw_hydra_panel (ui: &imgui::Ui, mock_controls: Option<&MockControls>, bridge: &ZgicabraBridge) {
    ui.text("Hydra");
    ui.same_line_with_pos(ui.window_size()[0] - 90.0);
    ui.disabled(true, || {
        let mut mock = mock_controls.is_some();
        ui.checkbox("mock", &mut mock);
    });
    ui.separator();

    match mock_controls {
        Some(controls) => {
            ui.group(|| {
                draw_wand_mock(ui, "Left", false, -bridge.left_rot[2].load(), &controls.left_trigger, &controls.left_stick_x, &controls.left_stick_y, &controls.left_buttons);
                draw_wand_rotation(ui, &bridge.left_rot);
            });
            ui.same_line();
            ui.group(|| draw_signal_state(ui, bridge));
            ui.same_line();
            ui.group(|| {
                draw_wand_mock(ui, "Right", true, -bridge.right_rot[2].load(), &controls.right_trigger, &controls.right_stick_x, &controls.right_stick_y, &controls.right_buttons);
                draw_wand_rotation(ui, &bridge.right_rot);
            });
        },
        None => {
            ui.text("Real Hydra hardware connected -- no mock wand controls.");
            draw_signal_state(ui, bridge);
        },
    }

    ui.spacing();
    if let Some(controls) = mock_controls {
        let seq_label = if controls.seq_playing.load(Ordering::Relaxed) { "Stop Sequence" } else { "Play Sequence" };
        if ui.button("Sine Drift") { MockControls::toggle(&controls.sine_drift); }
        ui.same_line();
        if ui.button(seq_label) { MockControls::toggle(&controls.seq_playing); }
        ui.same_line();
        if ui.button("< Tune") { controls.bump_tune_cycle(-1); }
        ui.same_line();
        if ui.button("Tune >") { controls.bump_tune_cycle(1); }
        ui.same_line();
    }
}

fn draw_ui (ui: &imgui::Ui, audio: Option<&AudioHandles>, mock_controls: Option<&MockControls>, bridge: &ZgicabraBridge, snapshot_browser: &mut SnapshotBrowser) {
    let [screen_width, screen_height] = ui.io().display_size;
    let engine_h = screen_height * 0.63;

    ui.window("Engine")
        .flags(imgui::WindowFlags::NO_MOVE)
        .position([10.0, 10.0], imgui::Condition::FirstUseEver)
        .size([screen_width - 20.0, engine_h], imgui::Condition::FirstUseEver)
        .build(|| {
            match audio {
                Some(audio) => draw_engine_panel(ui, audio, snapshot_browser),
                None => ui.text("Engine controls only available with the --audio backend."),
            }
        });

    ui.window("Hydra")
        .flags(imgui::WindowFlags::NO_MOVE)
        .position([10.0, engine_h + 20.0], imgui::Condition::FirstUseEver)
        .size([screen_width - 20.0, screen_height - engine_h - 30.0], imgui::Condition::FirstUseEver)
        .build(|| draw_hydra_panel(ui, mock_controls, bridge));
}

// Dumps the current framebuffer to a PNG, bypassing the OS screenshot
// permission dance entirely -- for verifying what the window actually
// rendered (e.g. from an agent/CI with no access to macOS's screen
// recording permission), set ZGICABRA_GUI_SCREENSHOT to a file path; the
// window captures itself once (after fonts/layout have settled) and exits.
fn save_screenshot (gl: &glow::Context, width: u32, height: u32, path: &str) {
    use glow::HasContext;

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut pixels));
    }

    // GL's origin is bottom-left; image rows run top-to-bottom, so flip.
    let row_bytes = (width * 4) as usize;
    let mut flipped = vec![0u8; pixels.len()];
    for row in 0..height as usize {
        let src = row * row_bytes;
        let dst = (height as usize - 1 - row) * row_bytes;
        flipped[dst..dst + row_bytes].copy_from_slice(&pixels[src..src + row_bytes]);
    }

    match save_png(path, &flipped, width, height) {
        Ok(())   => println!("║ Saved gui screenshot to {path}"),
        Err(e)   => eprintln!("║ 🟥 Failed to save gui screenshot to {path}: {e}"),
    }
}

fn save_png (path: &str, rgba: &[u8], width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

// Runs the GUI event loop on the calling (main) thread until the window is
// closed, at which point `quit` is set so the background loop can shut down.
pub fn run (audio: Option<AudioHandles>, mock_controls: Option<MockControls>, bridge: ZgicabraBridge, quit: Arc<AtomicBool>) {
    let screenshot_path = env::var("ZGICABRA_GUI_SCREENSHOT").ok();
    let mut frame_count: u32 = 0;
    let mut snapshot_browser = SnapshotBrowser::new();

    let sdl_context = sdl2::init().expect("failed to init SDL2");
    let video_subsystem = sdl_context.video().expect("failed to init SDL2 video subsystem");

    let window = video_subsystem
        .window("zgicabra", 720, 920)
        .position(0, 0)
        .opengl()
        .resizable()
        .build()
        .expect("failed to create window");

    // Kept alive for the duration of run() -- dropping it destroys the GL
    // context out from under the renderer.
    let gl_context = window.gl_create_context().expect("failed to create GL context");
    window.gl_make_current(&gl_context).expect("failed to make GL context current");

    let glow_context = unsafe {
        glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _)
    };

    let mut imgui_context = imgui::Context::create();
    imgui_context.set_ini_filename(None);

    let mut sdl_platform = imgui_sdl2_support::SdlPlatform::new(&mut imgui_context);

    imgui_context.fonts().add_font(&[imgui::FontSource::DefaultFontData { config: None }]);
    imgui_context.io_mut().font_global_scale = 1.3;

    let mut renderer = imgui_glow_renderer::AutoRenderer::new(glow_context, &mut imgui_context)
        .expect("failed to create imgui renderer");

    let mut event_pump = sdl_context.event_pump().expect("failed to create SDL event pump");
    let quit_watch = quit.clone();

    'main: loop {
        // Lets an external thread (e.g. main.rs's --test self-test) request
        // a close the same way the OS window-close button does, instead of
        // only ever setting `quit` on the way out.
        if quit_watch.load(Ordering::Relaxed) {
            break 'main;
        }

        for event in event_pump.poll_iter() {
            sdl_platform.handle_event(&mut imgui_context, &event);
            match event {
                Event::Quit { .. } => break 'main,
                Event::Window { win_event: WindowEvent::Close, .. } => break 'main,
                _ => {}
            }
        }

        sdl_platform.prepare_frame(&mut imgui_context, &window, &event_pump);

        let ui = imgui_context.frame();
        draw_ui(ui, audio.as_ref(), mock_controls.as_ref(), &bridge, &mut snapshot_browser);

        let draw_data = imgui_context.render();

        unsafe {
            use glow::HasContext;
            renderer.gl_context().clear_color(0.08, 0.08, 0.09, 1.0);
            renderer.gl_context().clear(glow::COLOR_BUFFER_BIT);
        }
        renderer.render(draw_data).expect("imgui render failed");

        frame_count += 1;
        if let Some(path) = &screenshot_path {
            if frame_count == 10 {
                let (width, height) = window.drawable_size();
                save_screenshot(renderer.gl_context(), width, height, path);
                break 'main;
            }
        }

        window.gl_swap_window();
    }

    quit.store(true, Ordering::Relaxed);
}
