
//
// Basic
//

use fundsp::prelude64::*;
use zgicabra_voice_macro::voice;

use crate::tools::{linexp,cents_to_ratio};
use super::stutter::stutter;
use super::wavetable::{wavetable, Wavetable};
use super::crusher::{crusher, LOW_MID_HZ, MID_HIGH_HZ};
use super::nam::load_named_model;
use super::nam_graph::nam_band;
use super::nam_node::NAM_WINDOW;
use super::signal::SharedSignal;
use super::voice::{Voice, VoiceDsp, ThumpMod, KnobPickup};

const DETUNE_CENTS: f32 = 1.0;
const CRUSH_DEPTH:  f32 = 1.0;

const CUTOFF_LO: f32 = 100.0;
const CUTOFF_HI: f32 = 14000.0;

const NAM_MODEL: &str = "sansamp";

const WT1_OFFSET: f32 = 0.236;

const LFO1_HZ: f32 = 0.11 * 57.0;
const LFO2_HZ: f32 = 0.13 * 31.0;
const LFO3_HZ: f32 = 0.17 * 13.0;

type Lfo = An<Pipe<Constant<U1>, Sine<f64>>>;



#[voice(index = 2, label = "Basic")]
#[derive(Clone)]
pub struct BasicVoice {
    #[node(init = wavetable("rt_9"))]     wt1: An<Wavetable>,
    #[node(init = wavetable("rt_fm_5"))]  wt2: An<Wavetable>,
    #[node(init = wavetable("rt_7"))]     wt3: An<Wavetable>,

    #[node(init = sine_hz(LFO1_HZ))] lfo1: Lfo,
    #[node(init = sine_hz(LFO2_HZ))] lfo2: Lfo,
    #[node(init = sine_hz(LFO3_HZ))] lfo3: Lfo,

    #[node(init = stutter())]   stutter: An<Unit<U1, U1>>,

    #[node(init = lowpass())] filter: An<Svf<f64, LowpassMode<f64>>>,

    #[node(init = Box::new(nam_band(&load_named_model(NAM_MODEL).unwrap(), &shared(1.0), &shared(0.0), NAM_WINDOW)))]
    nam: Box<dyn AudioUnit>,

    #[node(init = Box::new(crusher(&shared(CRUSH_DEPTH), shared(LOW_MID_HZ), shared(MID_HIGH_HZ), shared(0.0), shared(0.0), shared(0.0))))]
    crush: Box<dyn AudioUnit>,

    #[knob(range = 0.0..1.0,  default = 0.0)]  pub wt1_level:      Shared,
    #[knob(range = 0.0..1.0,  default = 0.0)]  pub wt2_level:      Shared,
    #[knob(range = 0.0..1.0,  default = 0.0)]  pub wt3_level:      Shared,
    #[knob(range = 0.0..1.0,  default = 0.25)] pub stutter_level:  Shared,
    #[knob(range = 1.0..10.0, set = |v| 1.0 + v*9.0, default = 1.0)]  pub saturation:    Shared,
    #[knob(range = 0.0..1.0,  default = 1.0)]  pub max_filter_input:    Shared,
    #[knob(range = 0.0..3.0,  set= |v| v*3.0, default = 0.5)]  pub filter_rez:    Shared,

    #[live(range = 0.0..1.0)] pub wt1_pos: Shared,
    #[live(range = 0.0..1.0)] pub wt2_pos: Shared,
    #[live(range = 0.0..1.0)] pub wt3_pos: Shared,

    thump: ThumpMod,
    sig:   SharedSignal,
}

impl VoiceDsp for BasicVoice {
    fn render (&mut self, freq: f32, thump_mult: f32) -> Frame<f32, U2> {

        let f = freq * thump_mult / 2.0;

        let width = lerp(0.2, 0.8, self.sig.width.value().clamp(0.0, 1.0));
        let fuzz  = self.sig.fuzz.value().clamp(0.0, 1.0) / 2.0;

        let d1 =  lerp(0.02, 0.2, self.sig.vel.value().clamp(0.0, 1.0));

        let depth = self.sig.depth.value();

        let pos1 = (depth + WT1_OFFSET + self.lfo1.get_mono() * d1).clamp(0.0, 1.0);
        self.wt1_pos.set_value(pos1);
        let osc1 = self.wt1.tick(&Frame::from([f, pos1]))[0] * self.wt1_level.value();

        let pos2 = (depth + width + self.lfo2.get_mono() * d1 + 0.1).clamp(0.0, 1.0);
        self.wt2_pos.set_value(pos2);
        let osc2 = self.wt2.tick(&Frame::from([f * cents_to_ratio(DETUNE_CENTS), pos2]))[0] * self.wt2_level.value();

        let pos3 = (depth + width + self.lfo3.get_mono() * d1).clamp(0.0, 1.0);
        self.wt3_pos.set_value(pos3);
        let osc3 = self.wt3.tick(&Frame::from([f * cents_to_ratio(-DETUNE_CENTS), pos3]))[0] * self.wt3_level.value();

        let st = self.stutter.filter_mono(f) * self.stutter_level.value();
        let mono = osc1 + osc2 + osc3 + st;

        let mono = (mono * self.saturation.value()).tanh() * self.sig.env.value();

        let cutoff = lerp(0.0, self.max_filter_input.value(), self.sig.filter.value()).clamp(0.0, 1.0);
        let cutoff_hz = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff);

        let wet = self.nam.filter_mono(mono);
        let nam_blend = self.sig.fuzz.value().clamp(0.0, 1.0);
        let namd = mono + nam_blend * (wet - mono);

        let namd = self.filter.tick(&Frame::from([namd, cutoff_hz, self.filter_rez.value()]))[0];

        let crushed = self.crush.filter_mono(namd);

        Frame::from([crushed, crushed])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_finite_bounded_signal () {
        let sig = SharedSignal::new();
        sig.env.set_value(1.0);

        let mut voice = BasicVoice::new(shared(0.0), shared(0.0), shared(0.0), sig);
        voice.set_sample_rate(48_000.0);

        voice.wt1_level.set_value(0.5);
        voice.wt2_level.set_value(0.5);
        voice.wt3_level.set_value(0.5);
        voice.stutter_level.set_value(0.25);

        for freq in [55.0, 220.0, 880.0] {
            for _ in 0..48_000 {
                let out = voice.render(freq, 1.0);
                assert!(out[0].is_finite() && out[1].is_finite(), "non-finite output at freq={freq}: {out:?}");
            }
        }
    }
}
