use drawille::{Canvas,PixelColor};

use crate::zgicabra::Voice;
use crate::audio::{AudioHandles,AudioErrors,SwarmView,GrowlView,ReeseView};

use super::utils::*;


//
// Voice Panel
//

fn draw_range_label (x: u16, y: u16, in_out: bool, label: &str, value: f32, min: f32, max: f32) {
    let norm_value = (value - min) / (max - min);
    let num_half_bars = (norm_value * 16.0).round() as usize;
    let odd_bar = num_half_bars % 2 == 1;
    let num_bars = num_half_bars / 2;
    let range_bar = format!("{}{}", "█".repeat(num_bars), if odd_bar { "▌" } else { "" });
    let color = if in_out { fg(GREEN_0) } else { fg(BLUE_0) };
    let tick = if in_out { "»" } else { "«" };

    print!("{}{:>13} {}{}{} {:<8} {:<5.2}",
        goto(x, y),
        label,
        color,
        tick,
        FG_RESET,
        range_bar.to_string(),
        value);
}

pub fn draw_voice_panel (y: u16, zgicabra: &crate::zgicabra::Zgicabra, audio: &AudioHandles) {

    // Blank
    for iy in y+1..y+21 {
        print!("{}{}", goto(1, iy), " ".repeat(75));
    }

    /*
    draw_range_label(2, y +  2, "master_vol",  audio.master_vol.value(), 0.0, 1.0);

    draw_range_label(2, y +  4, "main_sub",    audio.main_sub_lvl.value(),   0.0, 1.0);
    draw_range_label(2, y +  5, "dry_subl",    audio.dry_sub_lvl.value(),    0.0, 1.0);
    draw_range_label(2, y +  6, "thump_peak",  audio.thump_peak.value(),     1.0, 2.0);
    draw_range_label(2, y +  7, "thump_decay", audio.thump_decay.value(),    0.0, 1.0);
    draw_range_label(2, y +  8, "amp_boost",   audio.amp_boost.value(),      0.0, 1.0);
    draw_range_label(2, y +  9, "amp_blend",   audio.amp_blend.value(),      0.0, 1.0);
    draw_range_label(2, y + 10, "amp_xover",   audio.amp_crossover.value(),  0.0, 2000.0);
    draw_range_label(2, y + 11, "rev_dry",     audio.reverb_dry.value(),     0.0, 1.0);
    draw_range_label(2, y + 12, "rev_decay",   audio.reverb_decay.value(),   0.0, 1.0);
    draw_range_label(2, y + 13, "rev_damp",    audio.reverb_damp.value(),    0.0, 1.0);
    draw_range_label(2, y + 14, "rev_size",    audio.reverb_size.value(),    0.0, 100.0);
    draw_range_label(2, y + 15, "lim_thresh",  audio.limiter_thresh.value(), 0.0, 1.0);
    */

    match zgicabra.voice {
        Voice::VoiceA => draw_reese_panel(y, &audio.voice_a),
        Voice::VoiceB => draw_growl_panel(y, &audio.voice_b),
        //Voice::VoiceC => draw_basic_panel(y, &audio.voice_c),
        Voice::VoiceD => draw_swarm_panel(y, &audio.voice_d),
        _ => {},
    }

    draw_audio_errors(y, &audio.errors);
}


//
// Reese Panel
//

fn draw_reese_panel (y: u16, reese: &ReeseView) {
    draw_range_label(2, y +  2, true, "detune",    reese.detune_input.value(),  0.0, 100.0);
    draw_range_label(2, y +  3, true, "sub_level", reese.sub_level_input.value(), 0.0, 1.0);
    draw_range_label(2, y +  4, true, "drive",     reese.drive_input.value(),     0.0, 5.0);
    draw_range_label(2, y +  5, true, "cutoff",    reese.cutoff_input.value(),    0.0, 1.0);
    draw_range_label(2, y +  6, true, "resonance", reese.resonance_input.value(), 0.0, 5.0);
    draw_range_label(2, y +  7, true, "lfo_rate",  reese.lfo_rate_input.value(),  0.0, 1.0);
    draw_range_label(2, y +  8, true, "lfo_depth", reese.lfo_depth_input.value(), 0.0, 1.0);
    draw_range_label(2, y +  9, true, "impact_level", reese.impact_level_input.value(), 0.0, 1.0);

    draw_range_label(2, y + 11, false, "drive",    reese.drive_live.value(),      0.0, 5.0);
    draw_range_label(2, y + 12, false, "lfo_rate", reese.lfo_rate_live.value(),   0.0, 5.0);
    draw_range_label(2, y + 13, false, "detune",   reese.detune_live.value(),   0.0, 200.0);
    draw_range_label(2, y + 14, false, "cutoff",   reese.cutoff_live.value(),  0.0, 6000.0);
}


//
// Growl Panel
//

fn draw_growl_panel (y: u16, growl: &GrowlView) {
    draw_range_label(2, y + 2, true, "bass_drive", growl.bass_drive_input.value(), 0.0, 1.0);
    draw_range_label(2, y + 3, true, "filter",     growl.filter_input.value(), 0.0, 1.0);
    draw_range_label(2, y + 4, true, "space",      growl.space_input.value(), 0.0, 1.0);
    draw_range_label(2, y + 5, true, "warp",       growl.warp_input.value(), 0.0, 1.0);
    draw_range_label(2, y + 6, true, "nam_xover",  growl.nam_crossover_input.value(), 0.0, 10000.0);

    draw_range_label(2, y + 8, false, "filter",    growl.filter_live.value(), 0.0, 1.0);
    draw_range_label(2, y + 9, false, "warp",      growl.warp_live.value(), 0.0, 1.0);
}


//
// Swarm Panel
//

const SWARM_SCOPE_X:    u16 = 42;
const SWARM_SCOPE_COLS: u32 = 34; // char cols -- 2 px per drawille char
const SWARM_SCOPE_ROWS: u32 = 14; // char rows -- 4 px per drawille char

fn draw_swarm_panel (y: u16, swarm: &SwarmView) {
    draw_range_label(2, y +  2, true, "chase",  swarm.chase_factor_input.value(), 0.5,  1.0);
    draw_range_label(2, y +  3, true, "radius", swarm.radius_input.value(),       0.0,  200.0);
    draw_range_label(2, y +  4, true, "orbit",  swarm.orbit_speed_input.value(),  0.0,  2.0);
    draw_range_label(2, y +  5, true, "phaser", swarm.phaser_depth_input.value(), 0.0,  1.0);
    draw_range_label(2, y +  6, true, "xover",  swarm.xover_freq_input.value(),   0.0,  2000.0);

    draw_range_label(2, y +  8, false, "radius", swarm.radius_live.value(),       0.0, 200.0);
    draw_range_label(2, y +  9, false, "orbit",  swarm.orbit_speed_live.value(),  0.0, 2.0);
    draw_range_label(2, y + 10, false, "phaser", swarm.phaser_depth_live.value(), 0.0, 1.0);
    draw_range_label(2, y + 11, false, "origin", swarm.origin_live.value(),       0.0, 2000.0);

    print!("{}{:>13} {:<14}", goto(2, y + 6), "nam_lo", swarm.nam_lo.selected_name());
    print!("{}{:>13} {:<14}", goto(2, y + 7), "nam_hi", swarm.nam_hi.selected_name());

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

pub fn draw_audio_errors (y: u16, errors: &AudioErrors) {
    let log = errors.lock().unwrap();
    let color = if log.is_empty() { fg(GREEN_0) } else { fg(RED_0) };
    print!("{}▌{}▪{}▐", goto(74, y), color, FG_RESET);
}
