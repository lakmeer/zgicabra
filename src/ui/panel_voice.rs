use std::f32::consts::PI;

use drawille::{Canvas,PixelColor};
use fundsp::prelude64::Shared;

use crate::audio::{AudioHandles,AudioErrors,SwarmView,GrowlView,ReeseView,BasicView};
use crate::audio::crusher::{CRUSH_THRESHOLD};
use crate::zgicabra::Zgicabra;

use super::tw;
use super::utils::*;
use super::comp_meter::render_compressor_meter;

use rgb::RGB8;
use super::utils::{fg, GREEN_0, RED_0, FG_RESET};

const VU_CELLS:   usize = 32;
const VU_MIN_DB:  f32 = -46.0;
const VU_MAX_DB:  f32 = 0.0;
const VU_STEP_DB: f32 = (VU_MAX_DB - VU_MIN_DB) / (VU_CELLS as f32 - 1.0);

const YELLOW: RGB8 = tw::YELLOW_500;
const GRAY:   RGB8 = tw::SLATE_500;


//
// Voice Panel
//

fn draw_range_label (x: u16, y: u16, in_out: bool, selected: bool, label: &str, value: f32, min: f32, max: f32) {
    let norm_value = (value - min) / (max - min);
    let num_units = (norm_value * 24.0).round() as usize;
    let num_bars = num_units / 3;
    let partial = match num_units % 3 { 1 => "░", 2 => "▒", _ => "", };
    let range_bar = format!("{}{}", "█".repeat(num_bars), partial);
    let tick_color = if in_out { fg(tw::GREEN_500) } else { fg(tw::BLUE_500) };
    let tick = if in_out { "»" } else { "«" };
    let color = if selected { fg(tw::SLATE_100) } else { fg(tw::SLATE_400) };

    print!("{}{}{:>13} {}{}{} {:░<8} {:<5.2}{}",
        goto(x, y),
        color,
        label,
        tick_color,
        tick,
        color,
        range_bar.to_string(),
        value,
        FG_RESET,
    );
}

pub(super) fn draw_knob_list (x: u16, y: u16, knobs: &[(&'static str, Shared, f32, f32)], selected: usize) {
    for (i, (name, cell, min, max)) in knobs.iter().enumerate() {
        let label = name.strip_suffix("_input").unwrap_or(name);
        let label = label.strip_suffix("_level").unwrap_or(label);
        draw_range_label(x, y + i as u16, true, i == selected, label, cell.value(), *min, *max);
    }
}

pub(super) fn selected_index (selected_knob: &Shared, count: usize) -> usize {
    (selected_knob.value().floor() as usize).min(count.saturating_sub(1))
}

pub fn draw_audio_errors (y: u16, errors: &AudioErrors) {
    let log = errors.lock().unwrap();
    let color = if log.is_empty() { fg(GREEN_0) } else { fg(RED_0) };
    print!("{}▌{}▪{}▐", goto(74, y), color, FG_RESET);
}

fn draw_output_meter (peak_l: f32, rms_l: f32, peak_r: f32, rms_r: f32) -> String {
    let (peak_l_db, rms_l_db) = (amp_db(peak_l), amp_db(rms_l));
    let (peak_r_db, rms_r_db) = (amp_db(peak_r), amp_db(rms_r));

    format!("{} ¤ {}",
        render_channel(peak_l_db, rms_l_db, true),
        render_channel(peak_r_db, rms_r_db, false))
}

fn db_color (db: f32) -> RGB8 {
    if db > -3.0 { RED_0 }
    else if db > -12.0 { YELLOW }
    else { GREEN_0 }
}

fn amp_db (amp: f32) -> f32 {
    (20.0 * amp.max(1e-5).log10()).max(VU_MIN_DB)
}

fn render_channel (peak_db: f32, rms_db: f32, backwards: bool) -> String {
    let cells: Vec<(char, RGB8)> = (0..VU_CELLS).map(|i| {
        let cell_db = VU_MIN_DB + i as f32 * VU_STEP_DB;
        if cell_db < rms_db {
            ('━', db_color(cell_db)) 
        } else if cell_db < peak_db {
            ('-', db_color(peak_db))
        } else {
            ('·', tw::SLATE_500) // headroom
        }
    }).collect();

    let iter: Box<dyn Iterator<Item = &(char, RGB8)>> =
        if backwards { Box::new(cells.iter().rev()) } else { Box::new(cells.iter()) };

    iter.map(|(ch, color)| format!("{}{}{}", fg(*color), ch, FG_RESET)).collect()
}



//
// Main
//

pub fn draw_voice_panel (y: u16, zgicabra: &Zgicabra, audio: &AudioHandles) {
    for iy in y+1..y+21 {
        print!("{}{}", goto(1, iy), " ".repeat(75));
    }

    match audio.voice_selected.value() as usize {
        0 => draw_reese_panel(y + 2, &audio.voice_a),
        1 => draw_growl_panel(y + 2, &audio.voice_b),
        2 => draw_basic_panel(y + 2, &audio.voice_c),
        3 => draw_swarm_panel(y + 2, &audio.voice_d),
        _ => {},
    }

    let out_meter = draw_output_meter(
        audio.out_level_peak_l.value(), audio.out_level_rms_l.value(),
        audio.out_level_peak_r.value(), audio.out_level_rms_r.value());

    print!("{}{}", goto(5, y + 2), out_meter);

    draw_engine_knob_panel(38, y + 4, audio);

    draw_audio_errors(y, &audio.errors);
}

fn draw_engine_knob_panel (x: u16, y: u16, audio: &AudioHandles) {
    let knobs = audio.engine_knobs();
    draw_knob_list(x, y, &knobs, selected_index(&audio.engine_selected_knob, knobs.len()));
}



//
// Reese Panel
//

fn draw_reese_panel (y: u16, reese: &ReeseView) {
    let knobs = reese.knobs();
    draw_knob_list(2, y + 2, &knobs, selected_index(&reese.selected_knob, knobs.len()));

    draw_range_label(2, y + 13, false, false, "drive",     reese.drive_live.value(),     0.0, 5.0);
    draw_range_label(2, y + 14, false, false, "lfo_rate",  reese.lfo_rate_live.value(),  0.0, 5.0);
    draw_range_label(2, y + 15, false, false, "detune",    reese.detune_live.value(),  0.0, 200.0);
    draw_range_label(2, y + 16, false, false, "cutoff",    reese.cutoff_live.value(), 0.0, 6000.0);

    let meter = render_compressor_meter(
        reese.crush_env_live.value(),
        reese.crush_out_live.value(),
        CRUSH_THRESHOLD,
        reese.crush_gr_live.value());
    print!("{}{}", goto(7, y + 19), meter);
}


//
// Growl Panel
//

fn draw_growl_panel (y: u16, growl: &GrowlView) {
    let knobs = growl.knobs();
    draw_knob_list(2, y + 2, &knobs, selected_index(&growl.selected_knob, knobs.len()));

    draw_range_label(2, y + 9,  false, false, "filter",    growl.filter_live.value(), 0.0, 1.0);
    draw_range_label(2, y + 10, false, false, "warp",      growl.warp_live.value(), 0.0, 1.0);
}


//
// Basic Panel
//

fn draw_basic_panel (y: u16, basic: &BasicView) {
    let knobs = basic.knobs();
    draw_knob_list(2, y + 2, &knobs, selected_index(&basic.selected_knob, knobs.len()));

    let live_y = y + 2 + knobs.len() as u16 + 1;
    draw_range_label(2, live_y,     false, false, "wt1_pos", basic.wt1_pos.value(), 0.0, 1.0);
    draw_range_label(2, live_y + 1, false, false, "wt2_pos", basic.wt2_pos.value(), 0.0, 1.0);
    draw_range_label(2, live_y + 2, false, false, "wt3_pos", basic.wt3_pos.value(), 0.0, 1.0);
}




//
// Swarm Panel
//

const SWARM_SCOPE_X:    u16 = 42;
const SWARM_SCOPE_COLS: u32 = 34; // char cols -- 2 px per drawille char
const SWARM_SCOPE_ROWS: u32 = 14; // char rows -- 4 px per drawille char

fn draw_swarm_panel (y: u16, swarm: &SwarmView) {
    let knobs = swarm.knobs();
    draw_knob_list(2, y + 2, &knobs, selected_index(&swarm.selected_knob, knobs.len()));

    let info_y = y + 2 + knobs.len() as u16 + 1;

    draw_range_label(2, info_y + 0, false, false, "radius", swarm.radius_live.value(),       0.0, 200.0);
    draw_range_label(2, info_y + 1, false, false, "orbit",  swarm.orbit_speed_live.value(),  0.0, 8.0);
    draw_range_label(2, info_y + 2, false, false, "phaser", swarm.phaser_depth_live.value(), 0.0, 1.0);
    draw_range_label(2, info_y + 3, false, false, "origin", swarm.origin_live.value(),       0.0, 2000.0);
    draw_range_label(2, info_y + 3, false, false, "comb",   swarm.comb_mix_live.value(),     0.0, 1.0);

    let px_w = (SWARM_SCOPE_COLS * 2) as f32;
    let px_h = (SWARM_SCOPE_ROWS * 4) as f32;
    let mut canvas = Canvas::new(px_w as u32, px_h as u32);

    let origin = swarm.origin_live.value().max(1.0);
    let freqs: Vec<f32> = swarm.osc_freq_live.iter().map(|s| s.value()).collect();
    let pans:  Vec<f32> = swarm.osc_pan_live.iter().map(|s| s.value()).collect();

    // Autoscale the freq axis to whatever spread the oscillators are
    // actually showing right now, with a little headroom.
    let spread = freqs.iter().fold(1.0f32, |m, &f| m.max((f - origin).abs())) * 1.3;

    const OSC_COLORS: [PixelColor; 5] = [
        PixelColor::BrightCyan, PixelColor::Cyan,
        PixelColor::BrightBlue, PixelColor::Blue,
        PixelColor::White,
    ];

    for (k, (&f, &p)) in freqs.iter().zip(pans.iter()).enumerate() {
        let nx = ((f - origin) / spread).clamp(-1.0, 1.0);
        let px = (nx * 0.5 + 0.5) * (px_w - 1.0);
        let py = (1.0 - (p.clamp(-1.0, 1.0) * 0.5 + 0.5)) * (px_h - 1.0);
        pset(&mut canvas, px, py, OSC_COLORS[k % OSC_COLORS.len()]);
    }

    let mut rows = canvas.rows();
    drawille_paste(&mut rows, SWARM_SCOPE_X, y + 2);
}

