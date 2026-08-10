
//
// Reese bass generator, as a GenNode (6 in, 2 out). See
// ../../zgi-sc/experiments/exp2.scd -- classic Reese engine, one voice in a
// 4-voice detuned-saw beating stack whose harmonics beat/interfere across
// the spectrum.
//
// p1 = detune (0..1, rescaled to cents internally).
//

use fundsp::prelude64::*;

use super::gen_node::GenNode;

const DETUNE_CENTS_MAX: f32 = 50.0;

#[derive(Clone)]
pub struct ReeseGen {
    saw: An<WaveSynth<U1>>,
}

impl ReeseGen {
    pub fn new () -> ReeseGen {
        ReeseGen { saw: saw() }
    }
}

impl AudioNode for ReeseGen {
    const ID: u64 = 0x7A_11;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];
        let p1    = input[2];

        let detune     = 2f32.powf(p1 * DETUNE_CENTS_MAX / 1200.0);
        let voice_freq = freq * detune;

        let mono = self.saw.filter_mono(voice_freq) * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.saw.set_sample_rate(sample_rate);
    }
}

impl GenNode for ReeseGen {
    fn name (&self) -> &'static str { "Reese" }
    fn param_names (&self) -> [&'static str; 4] { ["detune", "", "", ""] }
}
