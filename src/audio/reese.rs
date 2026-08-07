
//
// Reese bass voice, as a self-contained fundsp AudioNode (2 in, 1 out). See
// ../../zgi-sc/experiments/exp2.scd -- classic Reese engine, one voice in a
// 4-voice detuned-saw beating stack whose harmonics beat/interfere across
// the spectrum.
//
// Inputs: [0] base_freq, [1] detune_cents_max (live-modulated by the caller
// via param_factor -- this node just does the per-voice detune + oscillator
// math, no knowledge of VoiceParams/SignalState).
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct ReeseVoice {
    saw:  An<WaveSynth<U1>>,
    frac: f32,
}

impl ReeseVoice {
    pub fn new (frac: f32) -> ReeseVoice {
        ReeseVoice { saw: saw(), frac }
    }
}

impl AudioNode for ReeseVoice {
    const ID: u64 = 0x7A_11;
    type Inputs = U2;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U1> {
        let base_freq         = input[0];
        let detune_cents_max  = input[1];

        let detune     = 2f32.powf(self.frac * detune_cents_max / 1200.0);
        let voice_freq = base_freq * detune;

        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = self.saw.filter_mono(voice_freq);
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.saw.set_sample_rate(sample_rate);
    }
}
