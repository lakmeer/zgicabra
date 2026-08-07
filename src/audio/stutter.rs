
//
// Stutter voice: triangle sub-oscillator at base_freq ring-modulated by white
// noise -- gates the noise on/off with the pitch instead of sitting at a
// fixed-frequency hiss. Self-contained fundsp AudioNode (1 in: base_freq, 1
// out, unleveled -- caller multiplies by its own level param, same
// convention as VoiceEngine's sub-oscillators).
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct StutterVoice {
    tri:   An<WaveSynth<U1>>,
    noise: Box<dyn AudioUnit>,
}

impl StutterVoice {
    pub fn new () -> StutterVoice {
        StutterVoice { tri: triangle(), noise: Box::new(white()) }
    }
}

impl AudioNode for StutterVoice {
    const ID: u64 = 0x7A_14;
    type Inputs = U1;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let base_freq = input[0];
        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = self.tri.filter_mono(base_freq) * self.noise.get_mono();
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.tri.set_sample_rate(sample_rate);
        self.noise.set_sample_rate(sample_rate);
    }
}
