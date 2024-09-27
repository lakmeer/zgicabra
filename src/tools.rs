
use core::f32::consts::PI;
use std::time::Instant;

use rand::prelude::*;
use rand_distr::StandardNormal;

use lazy_static::lazy_static;

lazy_static! {
    static ref START_TIME: Instant = Instant::now();
}

const NOTE_NAME:&str = "C C#D D#E F F#G G#A A#B ";


// Time

pub fn time_now () -> f32 {
    START_TIME.elapsed().as_millis() as f32 / 1000.0
}


// Trig

pub fn hyp (v: &[f32;3]) -> f32 {
    let dx = v[0] * v[0];
    let dy = v[1] * v[1];
    let dz = v[2] * v[2];
    (dx + dy + dz).sqrt()
}

pub fn rad_to_cycles (radians: f32) -> f32 {
    1.0 - (radians + PI/2.0 + PI) / (2.0 * PI) % 1.0
}

pub fn sin (freq: f32, phase: f32) -> f32 {
    (time_now() * PI * freq + phase).sin()
}

pub fn nsin (phase: f32) -> f32 {
    sin(1.0, phase) * 0.5 + 0.5
}

pub fn cos (phase: f32) -> f32 {
    (time_now() + phase).cos()
}

pub fn ncos (phase: f32) -> f32 {
    cos(phase) * 0.5 + 0.5
}


// Random Numbers

pub fn rand_normal (n: f32) -> f32 {
    n * rand::thread_rng().sample::<f32,_>(StandardNormal)
}

pub fn rand_uniform (n: f32) -> f32 {
    n * rand::random::<f32>()
}


// Easing

pub fn smoothstep (a: f32, b: f32, t: f32) -> f32 {
    let t = (t - a) / (b - a);
    t * t * (3.0 - 2.0 * t)
}

pub fn ease_in (t: f32) -> f32 {
    t * t
}

pub fn ease_out (t: f32) -> f32 {
    1.0 - ease_in(1.0 - t)
}


//
// Misc
//

pub fn button_mask (buttons: u32, mask: u32) -> bool {
    (buttons & mask) != 0
}

pub fn format_note (n: u8) -> String {
    format!("{} [{}]", n, NOTE_NAME.chars().skip((n % 12) as usize * 2).take(2).collect::<String>())
}

