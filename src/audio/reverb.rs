
//
// Reverb tail, as a fixed FxNode (7 in, 2 out) -- the last stage in the
// chain. room_size/decay/damp are baked into fundsp's FDN at construction
// (not live audio-rate inputs, same restart-to-apply caveat the old
// VoiceParams reverb fields had). p1 = reverb_wet, the live dry/wet balance
// between the input and the reverb tail; `level` is still the FxNode-
// standard master bypass on top of that (level=0 skips the reverb
// entirely, level=1 exposes the p1 tail balance).
//
// Unlike Crusher/NamStage/LowpassFx, this stage is genuinely stereo --
// reverb_stereo produces a distinct L/R tail from a mono-summed input.
//

use fundsp::prelude64::*;

use super::fx_node::FxNode;

pub struct ReverbFx {
    tail: Box<dyn AudioUnit>, // 2 in (L, R) / 2 out, built once from reverb_stereo
}

impl ReverbFx {
    pub fn new (room_size: f32, decay: f32, damp: f32) -> ReverbFx {
        ReverbFx { tail: Box::new(reverb_stereo(room_size, decay, damp)) }
    }
}

impl Clone for ReverbFx {
    fn clone (&self) -> ReverbFx { panic!("ReverbFx is not meant to be cloned -- built once in AuditionNode::new") }
}

impl AudioNode for ReverbFx {
    const ID: u64 = 0x7A_24;
    type Inputs = U7;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U2> {
        let x     = (input[0] + input[1]) * 0.5;
        let level = input[2];
        let wet_mix = input[3].clamp(0.0, 1.0); // reverb_wet

        let mut tail_out = [0.0f32; 2];
        self.tail.tick(&[x, x], &mut tail_out);

        let inner_l = x * (1.0 - wet_mix) + tail_out[0] * wet_mix;
        let inner_r = x * (1.0 - wet_mix) + tail_out[1] * wet_mix;

        let out_l = x * (1.0 - level) + inner_l * level;
        let out_r = x * (1.0 - level) + inner_r * level;
        Frame::from([out_l, out_r])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.tail.set_sample_rate(sample_rate);
    }
}

impl FxNode for ReverbFx {
    fn name (&self) -> &'static str { "Reverb" }
    fn param_names (&self) -> [&'static str; 4] { ["wet", "", "", ""] }
}
