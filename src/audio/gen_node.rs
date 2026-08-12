
//
// GenNode: shape shared by this file's individual generator impls (kept for
// potential reuse inside a future Voice -- see voice.rs). Every impl is a
// self-contained fundsp AudioNode, 6 in / 2 out:
//   in:  [freq, level, p1, p2, p3, p4]
//   out: [left, right] -- mono impls duplicate to both channels
// `level` (0..1) always directly multiplies the output (fader/bypass). p1-p4
// (0..1) are free for each impl to interpret and rescale as it likes. `name`/
// `param_names` label the node and its 4 params.
//
// The old GenSlot/GenCycler hot-swap-pool machinery that used to cycle
// through these at runtime is gone -- see the Voice trait in voice.rs.
//

use fundsp::prelude64::*;

use crate::tools::linexp;

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

// The GenSlot/GenCycler swap-pool orchestration (cycle-by-index warm pool,
// GUI cycler handle) that used to live here is gone -- see the Voice trait
// in voice.rs, which replaced the whole hot-swap system. ReeseGen, FmGen,
// StutterGen, and WavetableGen (used directly by GrowlVoice) stay as
// GenNode impls for potential reuse inside a future Voice.
