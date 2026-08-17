use drawille::{Canvas,PixelColor};

use crate::zgicabra::Voice;
use crate::audio::{AudioHandles,AudioErrors,SwarmView};

use super::utils::*;


//
// Voice Panel
//

fn draw_range_label (x: u16, y: u16, label: &str, value: f32, min: f32, max: f32) {
    let norm_value = (value - min) / (max - min);
    let num_half_bars = (norm_value * 16.0).round() as usize;
    let odd_bar = num_half_bars % 2 == 1;
    let num_bars = num_half_bars / 2;
    let range_bar = format!("{}{}", "█".repeat(num_bars), if odd_bar { "▌" } else { "" });

    print!("{}{:>13} {:<8} {:<5.2}", goto(x, y), label, range_bar.to_string(), value);
}

pub fn draw_voice_panel (y: u16, zgicabra: &crate::zgicabra::Zgicabra, audio: &AudioHandles) {

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

    if matches!(zgicabra.voice, Voice::VoiceB) {
        let growl = &audio.voice_b;
        let input = [
            ("bass_drive",    growl.bass_drive_input.value()),
            ("filter",        growl.filter_input.value()),
            ("space",         growl.space_input.value()),
            ("warp",          growl.warp_input.value()),
            ("nam_crossover", growl.nam_crossover_input.value()),
        ];
        let live = [
            ("filter",    growl.filter_live.value()),
            ("warp",      growl.warp_live.value()),
            ("freq_mult", growl.freq_mult_live.value()),
        ];

        for (row, (name, value)) in input.iter().enumerate() {
            print!("{}{:<14}{:>7.4}", goto(2, y + 2 + row as u16), name, value);
        }
        for (row, (name, value)) in live.iter().enumerate() {
            print!("{}{:<14}{:>7.4}", goto(26, y + 2 + row as u16), name, value);
        }
    }

    if matches!(zgicabra.voice, Voice::VoiceD) {
        draw_swarm_panel(y, &audio.voice_d);
    }

    draw_audio_errors(y + 19, &audio.errors);
}

const SWARM_SCOPE_X:    u16 = 42;
const SWARM_SCOPE_COLS: u32 = 34; // char cols -- 2 px per drawille char
const SWARM_SCOPE_ROWS: u32 = 14; // char rows -- 4 px per drawille char

// Right of the params: a little freq (x) / pan (y) scatter of where the 5
// swarm oscillators currently sit, orbiting the origin frequency (see
// swarm.rs's tick -- osc_freq/osc_pan/origin_live are written every sample).
fn draw_swarm_panel (y: u16, swarm: &SwarmView) {
    draw_range_label(2, y + 2, "chase",  swarm.chase_factor.value(), 0.5,  1.0);
    draw_range_label(2, y + 3, "radius", swarm.radius.value(),       0.0,  200.0);
    draw_range_label(2, y + 4, "orbit",  swarm.orbit_speed.value(),  0.0,  2.0);
    draw_range_label(2, y + 5, "phaser", swarm.phaser_depth.value(), 0.0,  1.0);
    draw_range_label(2, y + 6, "xover",  swarm.xover_freq.value(),   0.0,  2000.0);

    print!("{}{:>13} {:<14}", goto(2, y + 6), "nam_lo", swarm.nam_lo.selected_name());
    print!("{}{:>13} {:<14}", goto(2, y + 7), "nam_hi", swarm.nam_hi.selected_name());

    let px_w = (SWARM_SCOPE_COLS * 2) as f32;
    let px_h = (SWARM_SCOPE_ROWS * 4) as f32;
    let mut canvas = Canvas::new(px_w as u32, px_h as u32);

    let origin = swarm.origin_live.value().max(1.0);
    let freqs: Vec<f32> = swarm.osc_freq.iter().map(|s| s.value()).collect();
    let pans:  Vec<f32> = swarm.osc_pan.iter().map(|s| s.value()).collect();

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
    print!("{} {}▪", goto(2, y), fg(GREEN_0));
    print!("{} {}", goto(2, y), fg(RED_0));
    for _e in log.iter() {
        print!("▪");
    }
    print!("{}", termion::color::Fg(termion::color::Reset));
}
