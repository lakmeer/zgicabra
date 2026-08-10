
//
// Stutter generator: triangle sub-oscillator at base_freq ring-modulated by
// white noise -- gates the noise on/off with the pitch instead of sitting
// at a fixed-frequency hiss. GenNode (6 in, 2 out); no live params beyond
// freq/level.
//

use fundsp::prelude64::*;

use super::gen_node::GenNode;

const DRIVE: f32 = 4.0;

pub struct StutterGen {
    tri:   An<WaveSynth<U1>>,
    noise: Box<dyn AudioUnit>,
}

impl StutterGen {
    pub fn new () -> StutterGen {
        StutterGen { tri: triangle(), noise: Box::new(white()) }
    }
}

impl Clone for StutterGen {
    fn clone (&self) -> StutterGen { StutterGen::new() }
}

impl AudioNode for StutterGen {
    const ID: u64 = 0x7A_14;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];

        let dry  = self.tri.filter_mono(freq) * self.noise.get_mono();
        let mono = (dry * DRIVE).tanh() * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.tri.set_sample_rate(sample_rate);
        self.noise.set_sample_rate(sample_rate);
    }
}

impl GenNode for StutterGen {
    fn name (&self) -> &'static str { "Stutter" }
    fn param_names (&self) -> [&'static str; 4] { ["", "", "", ""] }
}
