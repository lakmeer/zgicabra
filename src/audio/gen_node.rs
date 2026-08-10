
//
// GenNode: shared interface for swappable sound generators. Every impl is a
// self-contained fundsp AudioNode, 6 in / 2 out:
//   in:  [freq, level, p1, p2, p3, p4]
//   out: [left, right] -- mono impls duplicate to both channels
// `level` (0..1) always directly multiplies the output (fader/bypass). p1-p4
// (0..1) are free for each impl to interpret and rescale as it likes. `name`/
// `param_names` label the node and its 4 params for the GUI.
//
// GenSlot cycles through a fixed set of GenNode impls (index 0 = Bypass),
// built once and kept warm, same "warm pool + Shared index" pattern as
// NamModelCycler/AuditionCycler -- see nam.rs. Because every impl shares
// the same 6-in/2-out shape now, dispatch no longer needs the dynamic-arity
// scratch-buffer trick the old AuditionVoice::tick() had.
//

use fundsp::prelude64::*;

use crate::tools::linexp;

use super::reese::ReeseGen;
use super::fm::FmGen;
use super::stutter::StutterGen;

pub trait GenNode: AudioNode<Inputs = U6, Outputs = U2> {
    fn name(&self) -> &'static str;
    fn param_names(&self) -> [&'static str; 4];
}

// Every fundsp builtin oscillator plus the live-cutoff noise generator,
// folded into one GenNode: the impl's one extra int field picks the
// waveform, p1 carries whichever single extra param that waveform needs
// (width for Pulse/Poly Pulse, roughness for Dsf Saw/Square, cutoff for
// Noise, ignored otherwise).
const OSC_NAMES: [&str; 14] = [
    "Saw", "Square", "Triangle", "Soft Saw", "Ramp", "Organ", "Hammond",
    "Pulse", "Poly Saw", "Poly Square", "Poly Pulse", "Dsf Saw", "Dsf Square", "Noise",
];

fn build_oscillators () -> Vec<Box<dyn AudioUnit>> {
    vec![
        Box::new(saw()), Box::new(square()), Box::new(triangle()), Box::new(soft_saw()),
        Box::new(ramp()), Box::new(organ()), Box::new(hammond()), Box::new(pulse()),
        Box::new(poly_saw()), Box::new(poly_square()), Box::new(poly_pulse()),
        Box::new(dsf_saw()), Box::new(dsf_square()),
        Box::new((white() | sink() | pass()) >> lowpass_q(1.0)),
    ]
}

pub struct BasicOscGen {
    extra: Shared, // waveform index into OSC_NAMES/build_oscillators
    units: Vec<Box<dyn AudioUnit>>,
}

impl BasicOscGen {
    pub fn new (extra: Shared) -> BasicOscGen {
        BasicOscGen { extra, units: build_oscillators() }
    }
}

impl Clone for BasicOscGen {
    fn clone (&self) -> BasicOscGen { BasicOscGen::new(self.extra.clone()) }
}

impl AudioNode for BasicOscGen {
    const ID: u64 = 0x7A_30;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];
        let p1    = input[2];

        let index = std::cmp::min(self.extra.value() as usize, self.units.len() - 1);
        let unit  = &mut self.units[index];

        let n_in = unit.inputs();
        let mut in_buf = [0.0f32; 2];
        if n_in >= 1 { in_buf[0] = freq; }
        // Noise's second input is a live lowpass cutoff (needs Hz, not
        // 0..1); every other user of a 2nd input (Pulse/Poly Pulse's
        // width, Dsf Saw/Square's roughness) wants 0..1 directly.
        if n_in >= 2 {
            in_buf[1] = if index == OSC_NAMES.len() - 1 { linexp(0.0, 1.0, 100.0, 14000.0, p1) } else { p1.clamp(0.0, 0.999) };
        }

        let mut out_buf = [0.0f32; 1];
        unit.tick(&in_buf[..n_in], &mut out_buf);
        let mono = out_buf[0] * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        for unit in self.units.iter_mut() { unit.set_sample_rate(sample_rate); }
    }
}

impl GenNode for BasicOscGen {
    fn name (&self) -> &'static str { "Basic Osc" }
    fn param_names (&self) -> [&'static str; 4] { ["wave_param", "", "", ""] }
}

// (name, param_names) for every GenSlot entry in order, index 0 = Bypass.
// Kept as a plain static table (same convention as the rest of this
// codebase, e.g. LFO_RATE_NAMES) rather than round-tripping through a
// throwaway instance, since GenCycler (the cheap GUI-thread handle) needs
// these without holding the actual boxed units.
pub const GEN_NAMES: [&str; 5] = ["Bypass", "Basic Osc", "Reese", "FM", "Stutter"];
pub const GEN_PARAMS: [[&str; 4]; 5] = [
    ["", "", "", ""],
    ["wave_param", "", "", ""],
    ["detune", "", "", ""],
    ["ratio", "index", "detune", "vibrato"],
    ["", "", "", ""],
];

fn build_gens (extra: Shared) -> Vec<Box<dyn AudioUnit>> {
    vec![
        Box::new(An(BasicOscGen::new(extra))),
        Box::new(An(ReeseGen::new())),
        Box::new(An(FmGen::new())),
        Box::new(An(StutterGen::new())),
    ]
}

// Audio-thread owner of a gen1-4 slot: cycles among GEN_NAMES by index
// (Bypass = 0), forwarding [freq, level, p1..p4] into whichever GenNode is
// selected every tick.
pub struct GenSlot {
    selected: Shared,
    units:    Vec<Box<dyn AudioUnit>>,
}

impl GenSlot {
    pub fn new (selected: Shared, extra: Shared) -> GenSlot {
        GenSlot { selected, units: build_gens(extra) }
    }

    pub fn tick (&mut self, freq: f32, level: f32, p: [f32; 4]) -> (f32, f32) {
        let index = self.selected.value() as usize;
        if index == 0 || index > self.units.len() { return (0.0, 0.0); }
        let unit = &mut self.units[index - 1];

        let in_buf = [freq, level, p[0], p[1], p[2], p[3]];
        let mut out = [0.0f32; 2];
        unit.tick(&in_buf, &mut out);
        (out[0], out[1])
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        for unit in self.units.iter_mut() { unit.set_sample_rate(sample_rate); }
    }
}

// Cheap GUI-facing handle onto a gen1-4 slot -- just the two live Shared
// cells (which GenNode is selected, and its one extra int field), same
// shape as NamModelCycler.
#[derive(Clone)]
pub struct GenCycler {
    selected: Shared,
    extra:    Shared,
}

impl GenCycler {
    pub fn new (selected: Shared, extra: Shared) -> GenCycler {
        GenCycler { selected, extra }
    }

    pub fn selected_name (&self) -> &'static str {
        GEN_NAMES.get(self.selected.value() as usize).copied().unwrap_or("?")
    }

    pub fn selected_params (&self) -> [&'static str; 4] {
        GEN_PARAMS.get(self.selected.value() as usize).copied().unwrap_or(["", "", "", ""])
    }

    pub fn extra (&self) -> i32 { self.extra.value() as i32 }
    pub fn set_extra (&self, v: i32) { self.extra.set_value(v as f32); }

    pub fn cycle (&self, delta: i32) {
        let count = GEN_NAMES.len() as i32;
        let current = self.selected.value() as i32;
        self.selected.set_value((current + delta).rem_euclid(count) as f32);
    }

    pub fn cells (&self) -> [(&'static str, &Shared); 2] {
        [("selected", &self.selected), ("extra", &self.extra)]
    }

    // Picks a random GenNode and a random extra value (harmless if the
    // chosen type ignores it, same "stale value" tolerance the old
    // AuditionCycler had).
    pub fn randomise (&self) {
        let mut rng = rand::thread_rng();
        self.selected.set_value(rand::Rng::gen_range(&mut rng, 0..GEN_NAMES.len() as i32) as f32);
        self.extra.set_value(rand::Rng::gen_range(&mut rng, 0..14) as f32);
    }
}
