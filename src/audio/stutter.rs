
//
// Stutter voice: white noise sampled-and-held at base_freq -- a stepped,
// bit-crushed noise texture locked to pitch instead of a fixed-frequency
// hiss. Self-contained fundsp AudioNode (1 in: base_freq, 1 out, unleveled
// -- caller multiplies by its own level param, same convention as
// VoiceEngine's sub-oscillators).
//
// Previously ring-modulated a triangle wave against continuous noise --
// multiplying two wideband signals together is still wideband, so it just
// amplitude-wobbled the noise floor and read as plain hiss. Sample-and-hold
// actually gates: the output only updates once per base_freq cycle, so it
// reads as a stepped/glitchy texture that audibly tracks pitch.
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct StutterVoice {
    chain: Box<dyn AudioUnit>,
}

impl StutterVoice {
    pub fn new () -> StutterVoice {
        // variability=0.0: perfectly periodic hold timing, so the sample
        // rate locks exactly to base_freq instead of jittering off it.
        StutterVoice { chain: Box::new((white() | pass()) >> hold(0.0)) }
    }
}

impl AudioNode for StutterVoice {
    const ID: u64 = 0x7A_14;
    type Inputs = U1;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let mut out_buf = [0.0f32; 1];
        self.chain.tick(&[input[0]], &mut out_buf);

        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = out_buf[0];
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.chain.set_sample_rate(sample_rate);
    }
}
