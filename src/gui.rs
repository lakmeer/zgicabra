
//
// GUI
//
// A winit + glow + Dear ImGui window for live-tuning the ModMatrix (rs.rs)
// and driving the mock Hydra backend's inputs without a keyboard.
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

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use imgui::{Drag, TableFlags};
use raw_window_handle::HasWindowHandle;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Fullscreen, WindowAttributes};

use crate::hydra::MockControls;
use crate::audio::{AudioHandles, GenCycler, FxCycler, AuditionNote, ModMatrix, snapshot};
use crate::tools::{AtomicF32, rand_normal};
use crate::zgicabra::{SignalOverride, ZgicabraBridge};

// Local (GUI-thread-only) browser state for saved mod-matrix snapshots --
// save/load are one-off file actions the GUI thread can just do directly on
// the same ModMatrix cells the sliders already write to, no cross-thread
// cycler needed (contrast NamModelCycler, which the audio thread also reads
// every block).
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

// Local (GUI-thread-only) state for the "hold note" audition button.
struct AuditionState {
    held:  bool,
    pitch: f32,
}

// Drag widgets need a step size that feels right whether the underlying
// range is 0..1 or 100..14000 -- scale it off the row's own lo/hi span.
fn drag_speed (lo: f32, hi: f32) -> f32 {
    ((hi - lo).abs() / 200.0).max(0.0001)
}

fn randomise_weights (matrix: &ModMatrix) {
    for spec in matrix.entries() {
        // LFO depth rows are 0..1, not -1..1 -- a negative weight there
        // would just flip the same saturation to the other rail.
        let (mid, lo, hi) = if spec.name.starts_with("lfo") && spec.name.ends_with("_depth") {
            (0.5, 0.0, 1.0)
        } else {
            (0.0, -1.0, 1.0)
        };
        for (_, cell) in spec.cells().into_iter().skip(3) {
            cell.set_value((mid + rand_normal((hi - lo) / 6.0)).clamp(lo, hi));
        }
    }
}

fn randomise_gens (audio: &AudioHandles) {
    audio.gen_1.randomise();
    audio.gen_2.randomise();
    audio.gen_3.randomise();
    audio.gen_4.randomise();
}

fn draw_mod_matrix (ui: &imgui::Ui, matrix: &ModMatrix) {
    let entries = matrix.entries();

    let Some(_table) = ui.begin_table_with_flags(
        "mod_matrix_grid",
        16,
        TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::RESIZABLE,
    ) else { return };

    ui.table_setup_column("param");
    ui.table_setup_column("default");
    ui.table_setup_column("lo");
    ui.table_setup_column("hi");
    ui.table_setup_column("curve");
    ui.table_setup_column("pitch");
    ui.table_setup_column("width");
    ui.table_setup_column("filter");
    ui.table_setup_column("fuzz");
    ui.table_setup_column("thump");
    ui.table_setup_column("vel");
    ui.table_setup_column("acc");
    ui.table_setup_column("lfo_1");
    ui.table_setup_column("lfo_2");
    ui.table_setup_column("lfo_3");
    ui.table_setup_column("lfo_4");
    ui.table_headers_row();

    for spec in entries {
        ui.table_next_row();

        ui.table_next_column();
        ui.text(spec.name);

        let lo = spec.cells()[1].1.value();
        let hi = spec.cells()[2].1.value();
        let speed = drag_speed(lo, hi);

        // cells() is [default, lo, hi, curve, pitch, width, filter, fuzz,
        // thump, vel, acc, lfo_1..lfo_4] -- indices 0..2 are the base
        // value/range, 3.. are the mod matrix columns (curve + weights),
        // which get the little no-label reset-to-zero button next to them.
        for (i, (cell_name, cell)) in spec.cells().into_iter().enumerate() {
            ui.table_next_column();
            let mut value = cell.value();
            let id = format!("##{}_{}", spec.name, cell_name);

            if Drag::new(id).speed(speed).build(ui, &mut value) {
                cell.set_value(value);
            }

            if i >= 3 {
                ui.same_line();
                if ui.button_with_size(format!("##{}_{}_reset", spec.name, cell_name), [14.0, 0.0]) {
                    cell.set_value(0.0);
                }
            }
        }
    }
}

fn toggle_checkbox (ui: &imgui::Ui, label: &str, flag: &Arc<AtomicBool>) {
    let mut value = flag.load(Ordering::Relaxed);
    if ui.checkbox(label, &mut value) {
        flag.store(value, Ordering::Relaxed);
    }
}

// Fixed-stage bypass: the checkbox just sets the stage's own FxNode `level`
// to 0 or 1 -- no separate flag underneath (see AudioHandles).
fn level_checkbox (ui: &imgui::Ui, label: &str, level: &fundsp::shared::Shared) {
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

// Reads/writes a ModMatrix row's base ("default") value through a knob,
// ranged to that row's own lo/hi -- the Engine panel's per-module controls
// are just this against whichever row names that module owns.
fn draw_param_knob (ui: &imgui::Ui, matrix: &ModMatrix, row: &str, label: &str, radius: f32) {
    let spec = matrix.get(row);
    let cells = spec.cells();
    let (lo, hi) = (cells[1].1.value(), cells[2].1.value());
    let mut value = cells[0].1.value();
    if knob(ui, &format!("##{row}"), label, radius, lo, hi, &mut value) {
        cells[0].1.set_value(value);
    }
}

const KNOB_RADIUS: f32 = 8.0;
// Simple cards (fixed knob set, no type-cycle row) vs. Gen/Fx cards, which
// need extra height for the type-cycle row and extra width for up to 5
// knobs in a row (level + FM's 4 params, the widest case -- see GEN_PARAMS).
const CARD_SIZE:        [f32; 2] = [140.0, 66.0];
const CARD_SIZE_CYCLER: [f32; 2] = [140.0, 90.0];

// Bordered box with a title and an optional top-right bypass checkbox --
// the Engine panel's one repeated "module card" shape.
fn draw_module_card (ui: &imgui::Ui, title: &str, bypass: Option<&fundsp::shared::Shared>, size: [f32; 2], body: impl FnOnce(&imgui::Ui)) {
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
fn draw_knob_row (ui: &imgui::Ui, matrix: &ModMatrix, knobs: &[(String, &str)]) {
    for (i, (row, label)) in knobs.iter().enumerate() {
        if i > 0 { ui.same_line(); }
        ui.group(|| draw_param_knob(ui, matrix, row, label, KNOB_RADIUS));
    }
}

// Gen/Fx slots share the same card shape: the existing type-cycle row,
// then one knob row for level + whichever params the currently-selected
// type actually uses (selected_params() leaves unused slots as "").
fn draw_gen_card (ui: &imgui::Ui, matrix: &ModMatrix, label: &str, cycler: &GenCycler) {
    draw_module_card(ui, label, None, CARD_SIZE_CYCLER, |ui| {
        draw_gen_slot(ui, label, cycler);
        let mut knobs = vec![(format!("{label}_lvl"), "level")];
        for (i, param) in cycler.selected_params().into_iter().enumerate() {
            if !param.is_empty() {
                knobs.push((format!("{label}_p{}", i + 1), param));
            }
        }
        draw_knob_row(ui, matrix, &knobs);
    });
}

fn draw_fx_card (ui: &imgui::Ui, matrix: &ModMatrix, label: &str, cycler: &FxCycler) {
    draw_module_card(ui, label, None, CARD_SIZE_CYCLER, |ui| {
        draw_fx_slot(ui, label, cycler);
        let mut knobs = vec![(format!("{label}_lvl"), "level")];
        for (i, param) in cycler.selected_params().into_iter().enumerate() {
            if !param.is_empty() {
                knobs.push((format!("{label}_p{}", i + 1), param));
            }
        }
        draw_knob_row(ui, matrix, &knobs);
    });
}

fn draw_lfo_card (ui: &imgui::Ui, matrix: &ModMatrix, n: usize) {
    draw_module_card(ui, &format!("LFO {n}"), None, CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[(format!("lfo_{n}_rate"), "rate"), (format!("lfo_{n}_depth"), "depth")]);
    });
}

fn randomise_all (audio: &AudioHandles) {
    randomise_weights(&audio.mod_matrix);
    randomise_gens(audio);
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

// NAM model cycler -- deliberately independent of the hydra/zgicabra
// VoiceChange path (rs.rs no longer reacts to it, see NamModelCycler's
// doc comment); these buttons drive rs.rs's model selection directly, so
// they work the same whether the mock or real hydra backend is active.
fn draw_nam_model (ui: &imgui::Ui, models: &crate::audio::NamModelCycler) {
    ui.text(format!("Model: {}", models.selected_name()));
    if ui.button("< Model") { models.cycle(-1); }
    ui.same_line();
    if ui.button("Model >") { models.cycle(1); }
}

// One row per gen slot (1-4): cycles which GenNode that slot is currently
// running, plus its one extra int field (e.g. BasicOscGen's waveform pick).
// Same idea as draw_nam_model. The slot's level/p1-p4 live in the mod
// matrix as gen_N_lvl/gen_N_p1..p4 rows -- see draw_mod_matrix -- so there's
// no separate param grid here anymore (the old per-slot 12-param grid this
// replaced is gone now that every GenNode shares one fixed 4-param shape).
fn draw_gen_slot (ui: &imgui::Ui, label: &str, cycler: &GenCycler) {
    ui.text(format!("{label}: {} (extra {})", cycler.selected_name(), cycler.extra()));
    if ui.button(format!("< {label}")) { cycler.cycle(-1); }
    ui.same_line();
    if ui.button(format!("{label} >")) { cycler.cycle(1); }
    ui.same_line();
    if ui.button(format!("{label} extra-")) { cycler.set_extra(cycler.extra() - 1); }
    ui.same_line();
    if ui.button(format!("{label} extra+")) { cycler.set_extra(cycler.extra() + 1); }
}

// One row per fx slot (1-4): cycles which FxNode that slot is currently
// running. Same shape as draw_gen_slot minus the extra int field (no
// current FxNode option uses one).
fn draw_fx_slot (ui: &imgui::Ui, label: &str, cycler: &FxCycler) {
    ui.text(format!("{label}: {}", cycler.selected_name()));
    if ui.button(format!("< {label}")) { cycler.cycle(-1); }
    ui.same_line();
    if ui.button(format!("{label} >")) { cycler.cycle(1); }
}

// "Hold Note" toggle: drives AudioOutput's freq/gate cells directly so
// weights/generators can be auditioned by ear without touching the wand
// controller. Click to trigger and hold the gate open; click again to
// release.
fn draw_audition_note (ui: &imgui::Ui, note: &AuditionNote, state: &mut AuditionState) {
    ui.set_next_item_width(80.0);
    Drag::new("Note##audition_pitch").range(0.0, 127.0).speed(0.2).build(ui, &mut state.pitch);
    ui.same_line();

    let label = if state.held { "Release Note" } else { "Hold Note" };
    if ui.button(label) {
        state.held = !state.held;
        if state.held { note.hold(state.pitch as u8); } else { note.release(); }
    }
}

// Save/load buttons for the weight matrix (VoiceParams), plus a cycler over
// every snapshot found on disk -- each save writes a new timestamped file
// rather than overwriting, see audio::snapshot.
fn draw_snapshot_browser (ui: &imgui::Ui, audio: &AudioHandles, browser: &mut SnapshotBrowser) {
    if ui.button("Save Snapshot") {
        if let Err(e) = snapshot::save_snapshot(audio) {
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
            if let Err(e) = snapshot::load_snapshot(&path, audio) {
                eprintln!("║ 🟥 Failed to load snapshot: {e}");
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

// Engine panel: one card per module, grouped into rows the same way the
// layout mockup does (utility row, LFOs, Gens, Compressor solo, FXs,
// Amp/Crusher/Reverb solo). Every card reads/writes the same ModMatrix
// rows the Matrix panel's table edits -- a card's knob is just that row's
// "default" cell, see draw_param_knob.
fn draw_engine_panel (ui: &imgui::Ui, audio: &AudioHandles, audition_state: &mut AuditionState) {
    let matrix = &audio.mod_matrix;

    draw_nam_model(ui, &audio.nam_models);
    ui.same_line();
    level_checkbox(ui, "Bypass Lowpass", &audio.lowpass_level);
    ui.same_line();
    draw_audition_note(ui, &audio.audition_note, audition_state);
    ui.separator();

    draw_module_card(ui, "Main Sub", None, CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[("main_sub_wave".into(), "wave"), ("main_sub_lvl".into(), "level")]);
    });
    ui.same_line();
    draw_module_card(ui, "Dry Sub", None, CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[("dry_sub_lvl".into(), "level")]);
    });
    ui.same_line();
    draw_module_card(ui, "Thump", None, CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[("thump_peak".into(), "peak"), ("thump_decay".into(), "decay")]);
    });

    for n in 1..=4 {
        draw_lfo_card(ui, matrix, n);
        if n < 4 { ui.same_line(); }
    }

    draw_gen_card(ui, matrix, "gen_1", &audio.gen_1);
    ui.same_line();
    draw_gen_card(ui, matrix, "gen_2", &audio.gen_2);
    ui.same_line();
    draw_gen_card(ui, matrix, "gen_3", &audio.gen_3);
    ui.same_line();
    draw_gen_card(ui, matrix, "gen_4", &audio.gen_4);

    draw_module_card(ui, "Compressor", Some(&audio.compressor_level), CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[
            ("comp_thresh".into(), "thresh"),
            ("comp_attack".into(), "attack"),
            ("comp_depth".into(), "depth"),
        ]);
    });

    draw_fx_card(ui, matrix, "fx_1", &audio.fx_1);
    ui.same_line();
    draw_fx_card(ui, matrix, "fx_2", &audio.fx_2);
    ui.same_line();
    draw_fx_card(ui, matrix, "fx_3", &audio.fx_3);
    ui.same_line();
    draw_fx_card(ui, matrix, "fx_4", &audio.fx_4);

    draw_module_card(ui, "Amp", Some(&audio.nam_level), CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[("amp_blend".into(), "blend"), ("amp_boost".into(), "boost")]);
    });
    ui.same_line();
    draw_module_card(ui, "Crusher", Some(&audio.crusher_level), CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[
            ("crush_thresh".into(), "thresh"),
            ("crush_attack".into(), "attack"),
            ("crush_depth".into(), "depth"),
            ("crush_boost".into(), "boost"),
        ]);
    });
    ui.same_line();
    draw_module_card(ui, "Reverb", Some(&audio.reverb_level), CARD_SIZE, |ui| {
        draw_knob_row(ui, matrix, &[
            ("reverb_size".into(), "size"),
            ("reverb_decay".into(), "decay"),
            ("reverb_damp".into(), "damp"),
            ("reverb_wet".into(), "wet"),
        ]);
    });
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

fn draw_matrix_panel (ui: &imgui::Ui, audio: &AudioHandles, snapshot_browser: &mut SnapshotBrowser) {
    if ui.button("RND") { randomise_all(audio); }
    ui.same_line();
    draw_snapshot_browser(ui, audio, snapshot_browser);
    ui.separator();
    draw_mod_matrix(ui, &audio.mod_matrix);
}

fn draw_ui (ui: &imgui::Ui, audio: Option<&AudioHandles>, mock_controls: Option<&MockControls>, bridge: &ZgicabraBridge, audition_state: &mut AuditionState, snapshot_browser: &mut SnapshotBrowser) {
    let [screen_width, screen_height] = ui.io().display_size;
    let left_w = screen_width * 0.44;
    let engine_h = screen_height * 0.63;

    ui.window("Engine")
        .position([10.0, 10.0], imgui::Condition::FirstUseEver)
        .size([left_w, engine_h], imgui::Condition::FirstUseEver)
        .build(|| {
            match audio {
                Some(audio) => draw_engine_panel(ui, audio, audition_state),
                None => ui.text("Engine controls only available with the --audio backend."),
            }
        });

    ui.window("Hydra")
        .position([10.0, engine_h + 20.0], imgui::Condition::FirstUseEver)
        .size([left_w, screen_height - engine_h - 30.0], imgui::Condition::FirstUseEver)
        .build(|| draw_hydra_panel(ui, mock_controls, bridge));

    ui.window("Matrix")
        .position([left_w + 20.0, 10.0], imgui::Condition::FirstUseEver)
        .size([screen_width - left_w - 30.0, screen_height - 20.0], imgui::Condition::FirstUseEver)
        .build(|| {
            match audio {
                Some(audio) => draw_matrix_panel(ui, audio, snapshot_browser),
                None => ui.text("Mod matrix only available with the --audio backend."),
            }
        });
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
    let mut audition_state = AuditionState { held: false, pitch: 43.0 };
    let mut snapshot_browser = SnapshotBrowser::new();
    let event_loop = EventLoop::new().expect("failed to create winit event loop");

    let window_attributes = WindowAttributes::default()
        .with_title("zgicabra tuner")
        .with_inner_size(winit::dpi::LogicalSize::new(1060.0, 620.0))
        .with_fullscreen(Some(Fullscreen::Borderless(None)));

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
