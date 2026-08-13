
//
// GUI
//
// A winit + glow + Dear ImGui window for live-tuning the audio engine's
// globals and the currently-selected Voice's params, and driving the mock
// Hydra backend's inputs without a keyboard.
//
// winit/AppKit requires window creation and the event loop to run on the
// process's main thread on macOS, so this owns main() when --gui is passed;
// main.rs moves the rest of the app (hydra/zgicabra/audio loop) onto a
// background thread instead. See main.rs.
//

use std::env;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fundsp::shared::Shared;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Fullscreen, WindowAttributes};

use crate::hydra::MockControls;
use crate::audio::{AudioHandles, AuditionSequence, GrowlHandle, BasicHandle, GrowlParams, BasicParams, VoiceParams, snapshot, GorgleHandle, GorgleParams};
use crate::tools::AtomicF32;
use crate::zgicabra::{SignalOverride, ZgicabraBridge};

const VOICE_NAMES: [&str; 4] = ["Growl", "Gorgle", "Basic", "Basic"];

// Local (GUI-thread-only) browser state for saved voice-param snapshots --
// save/load are one-off file actions the GUI thread can just do directly on
// the same Shared cells the knobs already write to.
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

// Local (GUI-thread-only) state for the audition sequence player: whether
// it's running, and how far into the current loop it is.
struct AuditionState {
    playing: bool,
    elapsed: f32,
}

// One 16-beat loop at 120bpm: a descending line (C3 F#2 F2) answered a
// fifth up (G3 C#3 C2). MIDI numbers assume C4 = 60 (same convention as the
// old "Hold Note" A4-is-69 test tone).
const SEQ_BPM: f32 = 120.0;
const SEQ_NOTES: [(u8, f32); 6] = [
    (48, 1.5), // C3
    (42, 1.5), // F#2
    (41, 5.0), // F2
    (43, 1.5), // G2
    (37, 1.5), // C#2
    (36, 5.0), // C2
];

fn seq_beat_seconds () -> f32 { 60.0 / SEQ_BPM }

fn seq_total_seconds () -> f32 {
    SEQ_NOTES.iter().map(|(_, beats)| beats).sum::<f32>() * seq_beat_seconds()
}

// Which note is sounding `t` seconds into the loop.
fn seq_note_at (t: f32) -> u8 {
    let mut acc = 0.0;
    for (note, beats) in SEQ_NOTES {
        acc += beats * seq_beat_seconds();
        if t < acc { return note; }
    }
    SEQ_NOTES.last().unwrap().0
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
// y flipped so up (top of the pad) is positive -- matches joystick_y
// convention elsewhere (ControllerFrame, Joystick).
fn xy_pad (ui: &imgui::Ui, id: &str, size: f32, x: &Arc<AtomicF32>, y: &Arc<AtomicF32>) {
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
    let p_min = origin;
    let p_max = [origin[0] + size, origin[1] + size];

    let bg = if active { [0.30, 0.34, 0.42, 1.0] } else if hovered { [0.22, 0.25, 0.31, 1.0] } else { [0.15, 0.16, 0.20, 1.0] };
    draw_list.add_rect(p_min, p_max, bg).filled(true).build();
    draw_list.add_rect(p_min, p_max, [0.5, 0.5, 0.55, 1.0]).build();

    let cx = origin[0] + size * 0.5;
    let cy = origin[1] + size * 0.5;
    draw_list.add_line([cx, p_min[1]], [cx, p_max[1]], [0.4, 0.4, 0.45, 1.0]).build();
    draw_list.add_line([p_min[0], cy], [p_max[0], cy], [0.4, 0.4, 0.45, 1.0]).build();

    let px = origin[0] + (x.load() * 0.5 + 0.5) * size;
    let py = origin[1] + (1.0 - (y.load() * 0.5 + 0.5)) * size;
    draw_list.add_circle([px, py], 5.0, [0.95, 0.80, 0.30, 1.0]).filled(true).build();
}

// A rotary knob: click and drag vertically to change value within [lo, hi]
// (drag up = increase). Vertical drag *delta*, not absolute mouse position,
// drives the value -- same convention as most DAW/synth knobs, since an
// absolute-angle knob (angle = mouse angle from center) makes fine
// adjustment impossible. speed comes from the same drag_speed() the Drag
// boxes use, so a knob and a drag box on the same range feel identical.
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

// Reads/writes a bare Shared through a knob, ranged to (lo, hi) -- every
// global/voice param control is just this against whichever Shared cell it
// owns, now that there's no ModMatrix row to pull lo/hi from.
fn draw_shared_knob (ui: &imgui::Ui, id: &str, label: &str, lo: f32, hi: f32, cell: &Shared) {
    let mut value = cell.value();
    if knob(ui, id, label, KNOB_RADIUS, lo, hi, &mut value) {
        cell.set_value(value);
    }
}

const KNOB_RADIUS: f32 = 8.0;
// Simple cards (fixed knob set) vs. the voice card, which needs extra width
// for the voice selector row plus up to 4 param knobs.
const CARD_SIZE:       [f32; 2] = [140.0, 66.0];
// One per-voice card (4 knobs, plus Growl's extra NAM cycler row) -- all 4
// now drawn side by side (see draw_voice_card), not just the selected one.
const VOICE_CARD_SIZE: [f32; 2] = [160.0, 90.0];

// Dark blue background tint for whichever voice card is currently active --
// see draw_voice_card.
const ACTIVE_VOICE_BG: [f32; 4] = [0.10, 0.16, 0.42, 1.0];

// Bordered box with a title and an optional top-right bypass checkbox --
// the Engine panel's one repeated "module card" shape. `active` tints the
// card's background (used by draw_voice_card to mark the selected voice;
// every other caller passes false, which leaves imgui's default ChildBg).
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

// One row of knobs laid out side by side -- knob() ends with a text label,
// which (like any imgui item) advances the cursor to a new line, so knobs
// placed side by side need to be wrapped in their own group() with
// same_line() between groups, not called bare in sequence (that would
// stack them vertically against the previous knob's label instead).
fn draw_knob_row (ui: &imgui::Ui, knobs: &[(&str, &str, f32, f32, &Shared)]) {
    for (i, (id, label, lo, hi, cell)) in knobs.iter().enumerate() {
        if i > 0 { ui.same_line(); }
        ui.group(|| draw_shared_knob(ui, &format!("##{id}"), label, *lo, *hi, cell));
    }
}

// "Play Sequence" toggle: drives AudioOutput's freq/gate/filter cells
// directly so voices can be auditioned by ear without touching the wand
// controller. Click to start the loop (see SEQ_NOTES); click again to stop.
// While playing, the filter cell is driven by a slow sine drift (period =
// 1.5x the loop length) independent of the note stepping, so the sweep
// isn't locked to the melody's rhythm.
fn draw_audition_sequence (ui: &imgui::Ui, seq: &AuditionSequence, state: &mut AuditionState) {
    let label = if state.playing { "Stop Sequence" } else { "Play Sequence" };
    if ui.button(label) {
        state.playing = !state.playing;
        if state.playing {
            state.elapsed = 0.0;
            seq.start(seq_note_at(0.0));
        } else {
            seq.stop();
        }
    }

    if state.playing {
        state.elapsed = (state.elapsed + ui.io().delta_time) % seq_total_seconds();
        seq.set_note(seq_note_at(state.elapsed));

        let period = seq_total_seconds() * 1.5;
        let filter = (state.elapsed / period * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        seq.set_filter(filter);
    }
}

// Voice selector: cycles voice_selected between VOICE_NAMES by index, same
// "< label >" idiom the old gen/fx slot cyclers used.
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

// Growl's 4 macro knobs -- see wavetable_gen.rs for what each does -- plus
// its bolted-on NAM amp stage's model cycler ("< Model / name / Model >",
// same idiom as draw_voice_selector).
fn draw_voice_growl (ui: &imgui::Ui, growl: &GrowlHandle) {
    draw_knob_row(ui, &[
        ("growl_bass_drive", "bass drive", 0.0, 1.0, &growl.bass_drive),
        ("growl_filter",     "filter",     0.0, 1.0, &growl.filter),
        ("growl_space",      "space",      0.0, 1.0, &growl.space),
        ("growl_warp",       "warp",       0.0, 1.0, &growl.warp),
    ]);
    if ui.button("< ##growl_nam") { growl.nam.cycle(-1); }
    ui.same_line();
    ui.text(format!("{}", growl.nam.selected_name()));
    ui.same_line();
    if ui.button(">##growl_nam") { growl.nam.cycle(1); }
}

// Basic's 4 oscillator mix levels.
fn draw_voice_basic (ui: &imgui::Ui, basic: &BasicHandle) {
    draw_knob_row(ui, &[
        ("basic_sin",    "sin",    0.0, 1.0, &basic.sin_level),
        ("basic_tri",    "tri",    0.0, 1.0, &basic.tri_level),
        ("basic_square", "square", 0.0, 1.0, &basic.square_level),
        ("basic_saw",    "saw",    0.0, 1.0, &basic.saw_level),
    ]);
}

// Gorgle's 4 macro knobs -- see gorgle.rs for what each does.
fn draw_voice_gorgle (ui: &imgui::Ui, gorgle: &GorgleHandle) {
    draw_knob_row(ui, &[
        ("gorgle_wobble",   "wobble",   0.0, 1.0, &gorgle.wobble),
        ("gorgle_ambience", "ambience", 0.0, 1.0, &gorgle.ambience),
        ("gorgle_girgle",   "girgle",   0.0, 1.0, &gorgle.girgle),
        ("gorgle_grind",    "grind",    0.0, 1.0, &gorgle.grind),
    ]);
}

// All 4 voices drawn side by side, always -- the active one (voice_selected)
// gets a dark blue card background instead of only the selected voice's
// controls being shown.
fn draw_voice_card (ui: &imgui::Ui, audio: &AudioHandles) {
    draw_voice_selector(ui, &audio.voice_selected);
    ui.separator();

    let selected = audio.voice_selected.value() as i32;

    draw_module_card(ui, "Growl", None, VOICE_CARD_SIZE, selected == 0, |ui| {
        draw_voice_growl(ui, &audio.voice_a);
    });
    ui.same_line();
    draw_module_card(ui, "Gorgle", None, VOICE_CARD_SIZE, selected == 1, |ui| {
        draw_voice_gorgle(ui, &audio.voice_b);
    });
    ui.same_line();
    draw_module_card(ui, "Basic C", None, VOICE_CARD_SIZE, selected == 2, |ui| {
        draw_voice_basic(ui, &audio.voice_c);
    });
    ui.same_line();
    draw_module_card(ui, "Basic D", None, VOICE_CARD_SIZE, selected == 3, |ui| {
        draw_voice_basic(ui, &audio.voice_d);
    });
}

// Save/load buttons for the currently-selected voice's live params, plus a
// cycler over every snapshot found on disk -- each save writes a new
// "{voice_name}_{index}.snap" file rather than overwriting, see
// audio::snapshot.
fn draw_snapshot_browser (ui: &imgui::Ui, audio: &AudioHandles, browser: &mut SnapshotBrowser) {
    if ui.button("Save Snapshot") {
        let result = match audio.voice_selected.value() as i32 {
            0 => snapshot::save_snapshot(GrowlParams::voice_name(), &audio.voice_a.params().fields()),
            1 => snapshot::save_snapshot(GorgleParams::voice_name(), &audio.voice_b.params().fields()),
            2 => snapshot::save_snapshot(BasicParams::voice_name(), &audio.voice_c.params().fields()),
            3 => snapshot::save_snapshot(BasicParams::voice_name(), &audio.voice_d.params().fields()),
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
                        0 => GrowlParams::voice_name(),
                        1 => GorgleParams::voice_name(),
                        2 | 3 => BasicParams::voice_name(),
                        _ => "",
                    };
                    if voice_name != expected {
                        eprintln!("║ 🟥 Snapshot '{name}' is for voice '{voice_name}', not the selected voice");
                    } else {
                        match audio.voice_selected.value() as i32 {
                            0 => audio.voice_a.load(&GrowlParams::from_fields(&fields)),
                            1 => audio.voice_b.load(&GorgleParams::from_fields(&fields)),
                            2 => audio.voice_c.load(&BasicParams::from_fields(&fields)),
                            3 => audio.voice_d.load(&BasicParams::from_fields(&fields)),
                            _ => {},
                        }
                    }
                },
                Err(e) => eprintln!("║ 🟥 Failed to load snapshot: {e}"),
            }
        }
    }
}

// One draggable row for a SignalState field: an "override" checkbox that
// takes the field over from whatever normally computes it, and a drag box
// for the value itself (greyed out until override is checked, since
// otherwise the engine loop stomps it back to the live value every tick --
// see ZgicabraBridge::sync).
fn signal_override_row (ui: &imgui::Ui, label: &str, lo: f32, hi: f32, field: &SignalOverride) {
    let mut enabled = field.enabled.load(Ordering::Relaxed);
    if ui.checkbox(format!("##{label}_override"), &mut enabled) {
        field.enabled.store(enabled, Ordering::Relaxed);
    }
    ui.same_line();

    ui.disabled(!enabled, || {
        let mut value = field.value.load();
        if knob(ui, &format!("##{label}_value"), label, 18.0, lo, hi, &mut value) {
            field.value.store(value);
        }
    });
}

// Compact knob grid (4 per row) for the 8 signal overrides -- the Hydra
// panel's answer to the layout mockup's central F/T/P/Z/W knob column,
// packed tighter than a tall single-column list since it now shares a row
// with the two wand pads (see draw_hydra_panel).
fn draw_signal_state (ui: &imgui::Ui, bridge: &ZgicabraBridge) {
    let rows: [(&str, f32, f32, &SignalOverride); 8] = [
        ("bend",         -2.0, 2.0,    &bridge.bend),
        ("filter",        0.0, 1.0,    &bridge.filter),
        ("fuzz",          0.0, 1.0,    &bridge.fuzz),
        ("width",        -1.0, 1.0,    &bridge.width),
        ("thump",         0.0, 1.0,    &bridge.thump),
        ("velocity",      0.0, 10.0,   &bridge.velocity),
        ("acceleration",  0.0, 100.0,  &bridge.acceleration),
        ("jerk",          0.0, 1000.0, &bridge.jerk),
    ];
    for (i, (label, lo, hi, field)) in rows.into_iter().enumerate() {
        if i % 4 != 0 { ui.same_line(); }
        ui.group(|| signal_override_row(ui, label, lo, hi, field));
    }
}

// Engine panel: globals first (main sub, dry sub, amp, reverb, limiter,
// voice selector), then the currently-selected voice's own param card.
fn draw_engine_panel (ui: &imgui::Ui, audio: &AudioHandles, audition_state: &mut AuditionState, snapshot_browser: &mut SnapshotBrowser) {
    draw_audition_sequence(ui, &audio.audition_seq, audition_state);
    ui.separator();

    draw_module_card(ui, "Main Sub", None, CARD_SIZE, false, |ui| {
        draw_knob_row(ui, &[
            ("main_sub_wave", "wave",  0.0, 1.0, &audio.main_sub_wave),
            ("main_sub_lvl",  "level", 0.0, 1.0, &audio.main_sub_lvl),
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
    // reverb_decay/damp/size are baked into the reverb tail at engine
    // construction time -- editing them here only takes effect on the next
    // process restart, same documented caveat this project has always had.
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

    ui.separator();
    draw_voice_card(ui, audio);
    ui.separator();
    draw_snapshot_browser(ui, audio, snapshot_browser);
}

fn draw_wand_mock (ui: &imgui::Ui, label: &str, trigger: &Arc<AtomicBool>, stick_x: &Arc<AtomicF32>, stick_y: &Arc<AtomicF32>, buttons: &[Arc<AtomicBool>; 4]) {
    ui.text(label);
    xy_pad(ui, &format!("##{label}_stick"), 120.0, stick_x, stick_y);

    toggle_checkbox(ui, &format!("Trigger##{label}"), trigger);

    ui.text("Buttons");
    for (i, button) in buttons.iter().enumerate() {
        if i > 0 { ui.same_line(); }
        toggle_checkbox(ui, &format!("{}##{label}_btn", i + 1), button);
    }
}

fn draw_wand_rotation (ui: &imgui::Ui, label: &str, rot: &[Arc<AtomicF32>; 4]) {
    ui.text(format!(
        "{label} rot: [{:.3} {:.3} {:.3} {:.3}]",
        rot[0].load(), rot[1].load(), rot[2].load(), rot[3].load(),
    ));
}

// Hydra: the physical/mock wand controller. Left wand | signal-override
// knob grid | right wand, side by side (mirroring the layout mockup's two
// octagon wand pads flanking a center knob column), with the sine-drift/
// tune/thump/fuzz toggles as one bottom row instead of scattered separately.
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
                draw_wand_mock(ui, "Left", &controls.left_trigger, &controls.left_stick_x, &controls.left_stick_y, &controls.left_buttons);
                draw_wand_rotation(ui, "Left", &bridge.left_rot);
            });
            ui.same_line();
            ui.group(|| draw_signal_state(ui, bridge));
            ui.same_line();
            ui.group(|| {
                draw_wand_mock(ui, "Right", &controls.right_trigger, &controls.right_stick_x, &controls.right_stick_y, &controls.right_buttons);
                draw_wand_rotation(ui, "Right", &bridge.right_rot);
            });
        },
        None => {
            ui.text("Real Hydra hardware connected -- no mock wand controls.");
            draw_signal_state(ui, bridge);
        },
    }

    ui.spacing();
    if let Some(controls) = mock_controls {
        if ui.button("Sine Drift") { MockControls::toggle(&controls.sine_drift); }
        ui.same_line();
        if ui.button("< Tune") { controls.bump_tune_cycle(-1); }
        ui.same_line();
        if ui.button("Tune >") { controls.bump_tune_cycle(1); }
        ui.same_line();
    }
    if ui.button("Thump") { bridge.thump.toggle(); }
    ui.same_line();
    if ui.button("Fuzz") { bridge.fuzz.toggle(); }
}

fn draw_ui (ui: &imgui::Ui, audio: Option<&AudioHandles>, mock_controls: Option<&MockControls>, bridge: &ZgicabraBridge, audition_state: &mut AuditionState, snapshot_browser: &mut SnapshotBrowser) {
    let [screen_width, screen_height] = ui.io().display_size;
    let engine_h = screen_height * 0.63;

    ui.window("Engine")
        .flags(imgui::WindowFlags::NO_MOVE)
        .position([10.0, 10.0], imgui::Condition::FirstUseEver)
        .size([screen_width - 20.0, engine_h], imgui::Condition::FirstUseEver)
        .build(|| {
            match audio {
                Some(audio) => draw_engine_panel(ui, audio, audition_state, snapshot_browser),
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

    match image::save_buffer(path, &flipped, width, height, image::ColorType::Rgba8) {
        Ok(())   => println!("║ Saved gui screenshot to {path}"),
        Err(e)   => eprintln!("║ 🟥 Failed to save gui screenshot to {path}: {e}"),
    }
}

// Runs the GUI event loop on the calling (main) thread until the window is
// closed, at which point `quit` is set so the background loop can shut down.
pub fn run (audio: Option<AudioHandles>, mock_controls: Option<MockControls>, bridge: ZgicabraBridge, quit: Arc<AtomicBool>) {
    let screenshot_path = env::var("ZGICABRA_GUI_SCREENSHOT").ok();
    let mut frame_count: u32 = 0;
    let mut audition_state = AuditionState { playing: false, elapsed: 0.0 };
    let mut snapshot_browser = SnapshotBrowser::new();
    let event_loop = EventLoop::new().expect("failed to create winit event loop");

    let window_attributes = WindowAttributes::default()
        .with_title("zgicabra")
        .with_position(winit::dpi::LogicalPosition::new(0.0, 0.0))
        .with_inner_size(winit::dpi::LogicalSize::new(720.0, 920.0));

    let template = ConfigTemplateBuilder::new();
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |mut configs| configs.next().unwrap())
        .expect("failed to create window/GL config");
    let window = window.expect("display builder returned no window");

    let raw_window_handle = window.window_handle().unwrap().as_raw();
    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let not_current_context = unsafe {
        gl_display.create_context(&gl_config, &context_attributes).expect("failed to create GL context")
    };

    let size = window.inner_size();
    let width  = NonZeroU32::new(size.width).unwrap_or(NonZeroU32::new(1).unwrap());
    let height = NonZeroU32::new(size.height).unwrap_or(NonZeroU32::new(1).unwrap());
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new()
        .build(raw_window_handle, width, height);
    let surface = unsafe {
        gl_display.create_window_surface(&gl_config, &surface_attributes).expect("failed to create GL surface")
    };

    let gl_context = not_current_context.make_current(&surface).expect("failed to make GL context current");

    let glow_context = unsafe {
        glow::Context::from_loader_function(|s| {
            let s = CString::new(s).unwrap();
            gl_display.get_proc_address(&s) as *const _
        })
    };

    let mut imgui_context = imgui::Context::create();
    imgui_context.set_ini_filename(None);

    let mut winit_platform = imgui_winit_support::WinitPlatform::new(&mut imgui_context);
    winit_platform.attach_window(imgui_context.io_mut(), &window, imgui_winit_support::HiDpiMode::Rounded);

    imgui_context.fonts().add_font(&[imgui::FontSource::DefaultFontData { config: None }]);
    imgui_context.io_mut().font_global_scale = (1.3 / winit_platform.hidpi_factor()) as f32;

    let mut renderer = imgui_glow_renderer::AutoRenderer::new(glow_context, &mut imgui_context)
        .expect("failed to create imgui renderer");

    let mut last_frame = Instant::now();
    let quit_watch = quit.clone();

    #[allow(deprecated)]
    event_loop.run(move |event, window_target| {
        match event {
            Event::NewEvents(_) => {
                let now = Instant::now();
                imgui_context.io_mut().update_delta_time(now.duration_since(last_frame));
                last_frame = now;
            }
            Event::AboutToWait => {
                // Lets an external thread (e.g. main.rs's --test self-test)
                // request a close the same way the OS window-close button
                // does, instead of only ever setting `quit` on the way out.
                if quit_watch.load(Ordering::Relaxed) {
                    window_target.exit();
                    return;
                }
                winit_platform.prepare_frame(imgui_context.io_mut(), &window).unwrap();
                window.request_redraw();
            }
            Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                let ui = imgui_context.frame();
                draw_ui(ui, audio.as_ref(), mock_controls.as_ref(), &bridge, &mut audition_state, &mut snapshot_browser);

                winit_platform.prepare_render(ui, &window);
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
                        let size = window.inner_size();
                        save_screenshot(renderer.gl_context(), size.width, size.height, path);
                        window_target.exit();
                    }
                }

                surface.swap_buffers(&gl_context).expect("failed to swap buffers");
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                window_target.exit();
            }
            Event::WindowEvent { event: WindowEvent::Resized(new_size), .. } => {
                if new_size.width > 0 && new_size.height > 0 {
                    surface.resize(
                        &gl_context,
                        NonZeroU32::new(new_size.width).unwrap(),
                        NonZeroU32::new(new_size.height).unwrap(),
                    );
                }
                winit_platform.handle_event(imgui_context.io_mut(), &window, &event);
            }
            event => {
                winit_platform.handle_event(imgui_context.io_mut(), &window, &event);
            }
        }
    }).expect("event loop error");

    quit.store(true, Ordering::Relaxed);
}
