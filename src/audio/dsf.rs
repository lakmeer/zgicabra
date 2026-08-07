
//
// DSF monster voice, as a self-contained fundsp AudioNode (7 in, 1 out).
// DSF oscillator (rich, roughness-controlled harmonic stack) -> Moog ladder
// filter with an LFO wobbling the cutoff -> tanh saturation for growl. The
// dubstep/Serum-style "monster bass" chain: fat harmonics, wobble, grit.
//
// Inputs: [0] base_freq, [1] roughness (DSF partial falloff, 0..1 -- higher
// is fatter/buzzier), [2] cutoff_hz (filter base cutoff), [3] resonance
// (Moog Q), [4] lfo_rate_hz (wobble speed), [5] lfo_depth_hz (wobble depth,
// added to cutoff_hz), [6] drive (post-filter tanh saturation amount) --
// all live-modulated by the caller via param_factor. This node just does
// the oscillator/filter/wobble/drive math, no knowledge of VoiceParams/
// SignalState.
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct DsfVoice {
    osc:    An<Dsf<U2>>,
    filter: An<Moog<f64, U3>>,
    lfo:    An<Sine<f64>>,
}

impl DsfVoice {
    pub fn new () -> DsfVoice {
        DsfVoice { osc: dsf_saw(), filter: moog(), lfo: sine() }
    }
}

impl AudioNode for DsfVoice {
    const ID: u64 = 0x7A_15;
    type Inputs = U7;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U1> {
        let base_freq    = input[0];
        // fundsp's Dsf blows up as roughness approaches/exceeds 1.0 (the
        // partial-sum denominator heads to zero) -- clamp below the edge.
        let roughness    = input[1].clamp(0.0, 0.999);
        let cutoff_hz    = input[2];
        let resonance    = input[3];
        let lfo_rate_hz  = input[4];
        let lfo_depth_hz = input[5];
        let drive        = input[6];

        let osc_out = self.osc.tick(&Frame::from([base_freq, roughness]))[0];

        let wobble = self.lfo.filter_mono(lfo_rate_hz);
        let wobble_cutoff = (cutoff_hz + wobble * lfo_depth_hz).max(20.0);

        let filtered = self.filter.tick(&Frame::from([osc_out, wobble_cutoff, resonance]))[0];
        let growled  = (filtered * (1.0 + drive)).tanh();

        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = growled;
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.osc.set_sample_rate(sample_rate);
        self.filter.set_sample_rate(sample_rate);
        self.lfo.set_sample_rate(sample_rate);
    }
}
