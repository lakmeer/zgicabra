
//
// Moog ladder lowpass, used inside voices (see swarm.rs). Cutoff and
// resonance come in as named 0..1 params rather than positional Frame
// slots -- there is no graph combinator wrapping this, so there is nothing
// to gain from pretending it has a fixed audio-rate arity.
//

use fundsp::prelude64::*;

use crate::tools::linexp;

const CUTOFF_LO: f32 = 100.0;
const CUTOFF_HI: f32 = 14000.0;

#[derive(Clone)]
pub struct MoogFilterFx {
    filter: An<Moog<f64, U3>>,
}

impl MoogFilterFx {
    pub fn new () -> MoogFilterFx {
        MoogFilterFx { filter: moog() }
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        self.filter.set_sample_rate(sample_rate);
    }

    // cutoff/resonance are 0..1; resonance remaps to a raw Moog Q of
    // 0.1..4.0 (see swarm.rs's MOOG_RESONANCE note on the self-oscillation
    // onset around raw Q 0.6-1.0).
    pub fn tick (&mut self, x: f32, cutoff: f32, resonance: f32) -> f32 {
        let cutoff_hz = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff);
        let q = 0.1 + resonance * 3.9;
        self.filter.tick(&Frame::from([x, cutoff_hz, q]))[0]
    }
}
