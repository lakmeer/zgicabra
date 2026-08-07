
//
// 3-operator FM voice, as a self-contained fundsp AudioNode (7 in, 1 out).
//
// Inputs: [0] base_freq, [1] ratio_a, [2] ratio_b, [3] ratio_c, [4] index_b,
// [5] index_c, [6] detune_cents_max -- all live-modulated by the caller via
// param_factor. This node just does the per-voice detune + 3-op FM math, no
// knowledge of VoiceParams/SignalState.
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct FmVoice {
    op_a: An<Sine<f64>>,
    op_b: An<Sine<f64>>,
    op_c: An<Sine<f64>>,
    frac: f32,
}

impl FmVoice {
    pub fn new (frac: f32) -> FmVoice {
        FmVoice { op_a: sine(), op_b: sine(), op_c: sine(), frac }
    }
}

impl AudioNode for FmVoice {
    const ID: u64 = 0x7A_10;
    type Inputs = U7;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U1> {
        let base_freq         = input[0];
        let ratio_a            = input[1];
        let ratio_b            = input[2];
        let ratio_c            = input[3];
        let index_b            = input[4];
        let index_c            = input[5];
        let detune_cents_max   = input[6];

        let detune     = 2f32.powf(self.frac * detune_cents_max / 1200.0);
        let voice_freq = base_freq * detune;

        let freq_c = voice_freq * ratio_c;
        let out_c  = self.op_c.filter_mono(freq_c);

        let freq_b = voice_freq * ratio_b;
        let out_b  = self.op_b.filter_mono(freq_b + out_c * (index_c * freq_c));

        let freq_a = voice_freq * ratio_a;
        let out_a  = self.op_a.filter_mono(freq_a + out_b * (index_b * freq_b));

        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = out_a;
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.op_a.set_sample_rate(sample_rate);
        self.op_b.set_sample_rate(sample_rate);
        self.op_c.set_sample_rate(sample_rate);
    }
}
