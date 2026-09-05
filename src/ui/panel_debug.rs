
//
// Debug Panel
//
// - Live spectrum analyzer
// - Raw zgicabra state
// - Recent DeltaEvents
//

use std::cell::Cell;
use std::f32::consts::PI;
use drawille::Canvas;
use drawille::PixelColor::TrueColor;
use fundsp::fft::real_fft;

use crate::tools::*;
use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,SignalState};
use crate::plot::{ColorPlot,Chart,Shape};
use crate::audio::{AudioHandles,NAM_SAMPLE_RATE};
use crate::HISTORY_WINDOW;

use super::tw;
use super::utils::*;
use super::panel_voice::{draw_knob_list, selected_index};

thread_local! {
    static SHOW_SPECTRUM:    Cell<bool>  = Cell::new(true);
    static DELTAS_CONSUMED:  Cell<usize> = Cell::new(0);
}

pub fn draw_debug_panel (y: u16, zgicabra: &Zgicabra, history: &Vec<Zgicabra>, delta_history: &Vec<DeltaEvent>, audio: &AudioHandles) {
    const RHS:u16 = 50;

    // Only scan the slice of delta_history not yet seen -- it's an
    // append-only log for the whole session, not a rolling window.
    let seen = DELTAS_CONSUMED.get();
    if delta_history[seen..].iter().any(|e| matches!(e, DeltaEvent::HomeDown(Hand::Left))) {
        SHOW_SPECTRUM.set(!SHOW_SPECTRUM.get());
    }
    DELTAS_CONSUMED.set(delta_history.len());

    if SHOW_SPECTRUM.get() {
        draw_spectrum(RHS as u32 - 5, 15, y, audio);
    } else {
        draw_graph(RHS as u32 - 5, 15, y, &history);
    }

    println!("{}Note:    {:>17}",   goto(RHS, y + 2), format_note(zgicabra.note.current));
    println!("{}Pitch:   {:>17.4}", goto(RHS, y + 3), zgicabra.note.bend);
    println!("{}Filter:  {:>17.4}", goto(RHS, y + 4), zgicabra.signal.filter);
    println!("{}Width:   {:>17.4}", goto(RHS, y + 5), zgicabra.signal.width);
    println!("{}Depth:   {:>17.4}", goto(RHS, y + 5), zgicabra.signal.depth);
    println!("{}Alpha:   {:>17.4}", goto(RHS, y + 6), zgicabra.signal.alpha);
    println!("{}Omega:   {:>17.4}", goto(RHS, y + 7), zgicabra.signal.omega);
    println!("{}Level:   {:>17.4}", goto(RHS, y + 8), zgicabra.signal.level);

    let default_color = termion::color::Fg(termion::color::White);

    print!("{}{}", goto(RHS, y), default_color);

    print!("{}[{:>5.2} {:>5.2} {:>5.2} {:>5.2} ]", goto(RHS, y + 11),
        zgicabra.left.rot[0],
        zgicabra.left.rot[1],
        zgicabra.left.rot[2],
        zgicabra.left.rot[3]);
    print!("{}[{:>5.2} {:>5.2} {:>5.2} {:>5.2} ]", goto(RHS, y + 12),
        zgicabra.right.rot[0],
        zgicabra.right.rot[1],
        zgicabra.right.rot[2],
        zgicabra.right.rot[3]);

    /*
    for row in 0..10 {
        println!("{}{}", goto(RHS, y + 15 + row as u16), " ".repeat(25));
        let color = if row == 0 { fg(tw::WHITE) } else { fg(tw::SLATE_500) };
        match delta_history.iter().rev().nth(row) {
            Some(e) => println!("{}{}- {:?}", goto(RHS, y + 15 + row as u16), color, e),
            None    => println!("{}{}- ",     goto(RHS, y + 15 + row as u16), color),
        }
    }
    */

    print!("{}{}", goto(RHS, y), default_color);
}

const SPECTRUM_FFT_LEN: usize = 1024;
const SPECTRUM_MIN_HZ:  f32   = 20.0;
const SPECTRUM_MAX_HZ:  f32   = 16_000.0;

fn draw_spectrum (w: u32, h: u32, y: u16, audio: &AudioHandles) {
    let px_w = w * 2;
    let px_h = h * 3;

    let mut canvas = Canvas::new(px_w, px_h);
    let raw = audio.capture.samples();

    if raw.len() >= SPECTRUM_FFT_LEN {
        let tail = &raw[raw.len() - SPECTRUM_FFT_LEN..];

        // Hann window to tame spectral leakage from the frame edges.
        let mut frame: [f32; SPECTRUM_FFT_LEN] = tail.try_into().unwrap();
        for (i, s) in frame.iter_mut().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (SPECTRUM_FFT_LEN - 1) as f32).cos();
            *s *= w;
        }

        let bins   = real_fft(&mut frame);
        let bin_hz = NAM_SAMPLE_RATE as f32 / SPECTRUM_FFT_LEN as f32;

        let ceiling_db = 20.0 * (SPECTRUM_FFT_LEN as f32).log10();
        let floor_db   = ceiling_db - 60.0;

        let log_min = SPECTRUM_MIN_HZ.ln();
        let log_max = SPECTRUM_MAX_HZ.ln();

        for band in 0..px_w {
            let t0 = band as f32 / px_w as f32;
            let t1 = (band + 1) as f32 / px_w as f32;
            let f0 = (log_min + (log_max - log_min) * t0).exp();
            let f1 = (log_min + (log_max - log_min) * t1).exp();

            let bin0 = ((f0 / bin_hz) as usize).max(1);
            let bin1 = ((f1 / bin_hz) as usize).max(bin0 + 1).min(bins.len() - 1);

            let mag = bins[bin0..bin1].iter()
                .map(|c| c.norm())
                .fold(0.0f32, f32::max);

            let db    = 20.0 * (mag + 1e-6).log10();
            let level = ((db - floor_db) / (ceiling_db - floor_db)).clamp(0.0, 1.0);
            let bar_h = (level * (px_h - 1) as f32).round() as u32;

            for row in 0..=bar_h {
                let t = row as f32 / (px_h - 1) as f32;
                pset(&mut canvas, band as f32, (px_h - 1 - row) as f32, spectrum_color(t));
            }
        }
    }

    let mut rows = canvas.rows();
    drawille_paste(&mut rows, 2, y + 2);
}

// Loudness-graded gradient (quiet -> loud) built from Tailwind stops, so a
// band's color reinforces its height instead of just decorating it.
fn spectrum_color (t: f32) -> drawille::PixelColor {
    let (a, b, t) = if t < 0.5 {
        (tw::EMERALD_500, tw::AMBER_400, t / 0.5)
    } else {
        (tw::AMBER_400, tw::RED_500, (t - 0.5) / 0.5)
    };

    TrueColor {
        r: lerp_u8(a.r, b.r, t),
        g: lerp_u8(a.g, b.g, t),
        b: lerp_u8(a.b, b.b, t),
    }
}

fn lerp_u8 (a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn draw_graph (w: u32, h: u32, y: u16, history: &Vec<Zgicabra>) {

    let n = history.len();

    let mut left_pos  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_pos : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_vel  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_vel : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_acc  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_acc : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];

    for i in 0..n {
        match history.get(i) {
            None => { },
            Some(frame) => {
                left_pos[i]   = (i as f32, frame.left.pos[0]);
                right_pos[i]  = (i as f32, frame.right.pos[0]);
                left_vel[i]   = (i as f32, frame.left.scalar_vel   *   -100.0);
                right_vel[i]  = (i as f32, frame.right.scalar_vel  *    100.0);
                left_acc[i]   = (i as f32, frame.left.scalar_acc   *   -800.0);
                right_acc[i]  = (i as f32, frame.right.scalar_acc  *    800.0);
            }
        }
    }

    Chart::new(w, h, 0.0, n as f32, -600.0, 600.0)
        .position(2, y + 2)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_acc),  BLUE_2)
        .linecolorplot(&Shape::Lines(&right_vel),  BLUE_0)
        .linecolorplot(&Shape::Lines(&right_pos),  BLUE_1)
        .display();
}
