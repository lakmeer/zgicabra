
//
// 3-operator FM voice, as a self-contained fundsp AudioNode (13 in, 1 out).
//
// Inputs: [0] base_freq, [1] ratio_a, [2] ratio_b, [3] ratio_c, [4] index_b,
// [5] index_c, [6] detune_cents_max, [7] lfo_rate_a_hz, [8] lfo_depth_a_hz,
// [9] lfo_rate_b_hz, [10] lfo_depth_b_hz, [11] lfo_rate_c_hz, [12]
// lfo_depth_c_hz -- all live-modulated by the caller via param_factor. Each
// operator gets its own vibrato LFO (sine, additive in Hz on that
// operator's own frequency, before it's used as a modulator for the next
// operator down the chain) for per-operator wobble/movement instead of a
// static tone. This node just does the per-voice detune + 3-op FM + vibrato
// math, no knowledge of VoiceParams/SignalState.
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct FmVoice {
    op_a:  An<Sine<f64>>,
    op_b:  An<Sine<f64>>,
    op_c:  An<Sine<f64>>,
    lfo_a: An<Sine<f64>>,
    lfo_b: An<Sine<f64>>,
    lfo_c: An<Sine<f64>>,
    frac:  f32,
}

impl FmVoice {
    pub fn new (frac: f32) -> FmVoice {
        FmVoice {
            op_a: sine(), op_b: sine(), op_c: sine(),
            lfo_a: sine(), lfo_b: sine(), lfo_c: sine(),
            frac,
        }
    }
}

impl AudioNode for FmVoice {
    const ID: u64 = 0x7A_10;
    type Inputs = U13;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U13>) -> Frame<f32, U1> {
        let base_freq         = input[0];
        let ratio_a            = input[1];
        let ratio_b            = input[2];
        let ratio_c            = input[3];
        let index_b            = input[4];
        let index_c            = input[5];
        let detune_cents_max   = input[6];
        let lfo_rate_a_hz      = input[7];
        let lfo_depth_a_hz     = input[8];
        let lfo_rate_b_hz      = input[9];
        let lfo_depth_b_hz     = input[10];
        let lfo_rate_c_hz      = input[11];
        let lfo_depth_c_hz     = input[12];

        let detune     = 2f32.powf(self.frac * detune_cents_max / 1200.0);
        let voice_freq = base_freq * detune;

        let wobble_a = self.lfo_a.filter_mono(lfo_rate_a_hz) * lfo_depth_a_hz;
        let wobble_b = self.lfo_b.filter_mono(lfo_rate_b_hz) * lfo_depth_b_hz;
        let wobble_c = self.lfo_c.filter_mono(lfo_rate_c_hz) * lfo_depth_c_hz;

        // index_x * freq_x scales FM modulation depth, not pitch -- keep it
        // on the unwobbled base frequency so vibrato only ever moves each
        // operator's own pitch, not the modulation index it's feeding down
        // the chain (that coupling is what read as tremolo/amplitude wobble
        // instead of pitch wobble).
        let freq_c_base = voice_freq * ratio_c;
        let freq_c = freq_c_base + wobble_c;
        let out_c  = self.op_c.filter_mono(freq_c);

        let freq_b_base = voice_freq * ratio_b;
        let freq_b = freq_b_base + wobble_b;
        let out_b  = self.op_b.filter_mono(freq_b + out_c * (index_c * freq_c_base));

        let freq_a = voice_freq * ratio_a + wobble_a;
        let out_a  = self.op_a.filter_mono(freq_a + out_b * (index_b * freq_b_base));

        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = out_a;
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.op_a.set_sample_rate(sample_rate);
        self.op_b.set_sample_rate(sample_rate);
        self.op_c.set_sample_rate(sample_rate);
        self.lfo_a.set_sample_rate(sample_rate);
        self.lfo_b.set_sample_rate(sample_rate);
        self.lfo_c.set_sample_rate(sample_rate);
    }
}
