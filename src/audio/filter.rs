
//
// Moog ladder lowpass, as an FxNode (7 in, 2 out). Not currently wired
// into Engine (see mod.rs). MoogFilterFx takes cutoff/resonance as
// generic 0..1 params; LowpassFx is a fixed-resonance variant intended to
// track the live `filter` hardware signal directly.
//

use fundsp::prelude64::*;

use crate::tools::linexp;

use super::fx_node::FxNode;

const CUTOFF_LO: f32 = 100.0;
const CUTOFF_HI: f32 = 14000.0;
const FIXED_RESONANCE: f32 = 0.6;

#[derive(Clone)]
pub struct MoogFilterFx {
    filter: An<Moog<f64, U3>>,
}

impl MoogFilterFx {
    pub fn new () -> MoogFilterFx {
        MoogFilterFx { filter: moog() }
    }
}

impl AudioNode for MoogFilterFx {
    const ID: u64 = 0x7A_22;
    type Inputs = U7;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U2> {
        let x     = (input[0] + input[1]) * 0.5;
        let level = input[2];
        let cutoff_hz = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, input[3]);
        let resonance = 0.1 + input[4] * 3.9; // 0.1..4.0

        let wet  = self.filter.tick(&Frame::from([x, cutoff_hz, resonance]))[0];
        let mono = x * (1.0 - level) + wet * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.filter.set_sample_rate(sample_rate);
    }
}

impl FxNode for MoogFilterFx {
    fn name (&self) -> &'static str { "Moog Filter" }
    fn param_names (&self) -> [&'static str; 4] { ["cutoff", "resonance", "", ""] }
}

#[derive(Clone)]
pub struct LowpassFx {
    filter: An<Moog<f64, U3>>,
}

impl LowpassFx {
    pub fn new () -> LowpassFx {
        LowpassFx { filter: moog() }
    }
}

impl AudioNode for LowpassFx {
    const ID: u64 = 0x7A_23;
    type Inputs = U7;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U2> {
        let x     = (input[0] + input[1]) * 0.5;
        let level = input[2];
        let cutoff_hz = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, input[3]);

        let wet  = self.filter.tick(&Frame::from([x, cutoff_hz, FIXED_RESONANCE]))[0];
        let mono = x * (1.0 - level) + wet * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.filter.set_sample_rate(sample_rate);
    }
}

impl FxNode for LowpassFx {
    fn name (&self) -> &'static str { "Lowpass" }
    fn param_names (&self) -> [&'static str; 4] { ["filter", "", "", ""] }
}
