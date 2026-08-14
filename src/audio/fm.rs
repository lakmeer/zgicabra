
//
// 3-operator FM generator, as a GenNode (6 in, 2 out). Not currently
// wired into Engine (see mod.rs). Classic op_c-modulates-op_b-modulates-
// op_a FM stack; each operator gets an additive-Hz vibrato LFO from one
// shared depth knob. Params: ratio_b, index_b, detune (cents), vibrato
// depth (Hz); ratio_a/ratio_c/index_c are fixed constants.
//

use fundsp::prelude64::*;

use super::gen_node::GenNode;

const DETUNE_CENTS_MAX: f32 = 50.0;
const VIBRATO_HZ_MAX:   f32 = 8.0;
const VIBRATO_RATE_HZ:  f32 = 5.0;

const RATIO_A: f32 = 1.0;
const RATIO_C: f32 = 2.0;
const INDEX_C: f32 = 1.0;

#[derive(Clone)]
pub struct FmGen {
    op_a: An<Sine<f64>>,
    op_b: An<Sine<f64>>,
    op_c: An<Sine<f64>>,
    lfo:  An<Sine<f64>>,
}

impl FmGen {
    pub fn new () -> FmGen {
        FmGen { op_a: sine(), op_b: sine(), op_c: sine(), lfo: sine() }
    }
}

impl AudioNode for FmGen {
    const ID: u64 = 0x7A_10;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];
        let ratio_b = 1.0 + input[2] * 4.0; // 1..5
        let index_b = input[3] * 4.0;       // 0..4
        let p3      = input[4];
        let p4      = input[5];

        let detune     = 2f32.powf(p3 * DETUNE_CENTS_MAX / 1200.0);
        let voice_freq = freq * detune;

        let wobble = self.lfo.filter_mono(VIBRATO_RATE_HZ) * (p4 * VIBRATO_HZ_MAX);

        let freq_c_base = voice_freq * RATIO_C;
        let freq_c      = freq_c_base + wobble;
        let out_c       = self.op_c.filter_mono(freq_c);

        let freq_b_base = voice_freq * ratio_b;
        let freq_b      = freq_b_base + wobble;
        let out_b       = self.op_b.filter_mono(freq_b + out_c * (INDEX_C * freq_c_base));

        let freq_a = voice_freq * RATIO_A + wobble;
        let out_a  = self.op_a.filter_mono(freq_a + out_b * (index_b * freq_b_base));

        let mono = out_a * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.op_a.set_sample_rate(sample_rate);
        self.op_b.set_sample_rate(sample_rate);
        self.op_c.set_sample_rate(sample_rate);
        self.lfo.set_sample_rate(sample_rate);
    }
}

impl GenNode for FmGen {
    fn name (&self) -> &'static str { "FM" }
    fn param_names (&self) -> [&'static str; 4] { ["ratio", "index", "detune", "vibrato"] }
}
