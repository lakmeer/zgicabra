
//
// Reese -- classic detuned-unison-saw bass, per ref/reese-bass-dsp-guide.md.
// A stack of VOICES band-limited saws (fundsp's wavetable saw(), already
// anti-aliased -- no need to hand-roll PolyBLEP per the guide's §2.2) spread
// symmetrically in cents (§3/§4, odd voice count so one voice sits at zero
// detune/center pan as a phase-stable anchor), each run through its own
// fundsp panner() (§4's equal-power pan law, built in rather than hand-
// rolled trig) so the live `width` signal can spread/collapse the stack.
// A detuned sub-octave layer (§3's optional f3) adds weight and is left
// unpanned -- kept mono/centered on purpose, cheap stand-in for §11's
// "keep the low end phase-coherent" advice without a full crossover split.
// A slow LFO (§6) animates both detune spread and filter cutoff together
// for "motion", and a live-modulated fundsp SVF lowpass (§7) shapes the
// tone, driven by both the LFO and the live `filter` signal (same macro-
// times-signal idiom growl.rs already uses for its own filter mapping).
// tanh soft-clip (§8) sits pre-filter for harmonic richness. No sidechain/
// hoover/multiband crossover -- those need a kick signal or per-band
// routing this single-voice slot doesn't have inputs for.
//

use fundsp::prelude64::*;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::voice::{Voice, VoiceParams, ThumpMod};

const VOICES: usize = 7; // odd -- center voice lands at zero detune/pan

const SUB_RATIO: f32 = 0.5;         // one octave down
const SUB_DETUNE_CENTS: f32 = -6.0; // keeps the sub from phase-locking to voice 0

const CUTOFF_LO: f32 = 80.0;
const CUTOFF_HI: f32 = 6000.0;
const LFO_DEPTH_OCTAVES: f32 = 2.5; // max cutoff swing at lfo_depth = 1
const DETUNE_LFO_DEPTH: f32 = 0.35; // detune spread wobble, fraction of base detune

fn cents_to_ratio (cents: f32) -> f32 { 2f32.powf(cents / 1200.0) }

#[derive(Clone, Copy)]
pub struct ReeseParams {
    pub detune:    f32, // unison spread, cents
    pub sub_level: f32,
    pub drive:     f32, // pre-filter tanh saturation
    pub cutoff:    f32, // macro 0..1, scaled by live `filter` signal (see growl.rs precedent)
    pub resonance: f32, // filter Q
    pub lfo_rate:  f32, // Hz
    pub lfo_depth: f32, // 0..1, drives both cutoff and detune wobble
    pub width:     f32, // base stereo spread, 0..1, added to live `width` signal
}

impl Default for ReeseParams {
    fn default () -> ReeseParams {
        ReeseParams {
            detune: 24.0, sub_level: 0.5, drive: 3.0, cutoff: 0.6,
            resonance: 1.2, lfo_rate: 0.3, lfo_depth: 0.35, width: 0.7,
        }
    }
}

impl VoiceParams for ReeseParams {
    fn voice_name () -> &'static str { "reese" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("detune",    self.detune),
            ("sub_level", self.sub_level),
            ("drive",     self.drive),
            ("cutoff",    self.cutoff),
            ("resonance", self.resonance),
            ("lfo_rate",  self.lfo_rate),
            ("lfo_depth", self.lfo_depth),
            ("width",     self.width),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> ReeseParams {
        let mut params = ReeseParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "detune"    => params.detune    = *value,
                "sub_level" => params.sub_level = *value,
                "drive"     => params.drive     = *value,
                "cutoff"    => params.cutoff    = *value,
                "resonance" => params.resonance = *value,
                "lfo_rate"  => params.lfo_rate  = *value,
                "lfo_depth" => params.lfo_depth = *value,
                "width"     => params.width     = *value,
                _ => {},
            }
        }
        params
    }
}

#[derive(Clone)]
pub struct ReeseHandle {
    pub detune:    Shared,
    pub sub_level: Shared,
    pub drive:     Shared,
    pub cutoff:    Shared,
    pub resonance: Shared,
    pub lfo_rate:  Shared,
    pub lfo_depth: Shared,
    pub width:     Shared,
}

impl ReeseHandle {
    pub fn new (params: &ReeseParams) -> ReeseHandle {
        ReeseHandle {
            detune:    shared(params.detune),
            sub_level: shared(params.sub_level),
            drive:     shared(params.drive),
            cutoff:    shared(params.cutoff),
            resonance: shared(params.resonance),
            lfo_rate:  shared(params.lfo_rate),
            lfo_depth: shared(params.lfo_depth),
            width:     shared(params.width),
        }
    }

    pub fn params (&self) -> ReeseParams {
        ReeseParams {
            detune:    self.detune.value(),
            sub_level: self.sub_level.value(),
            drive:     self.drive.value(),
            cutoff:    self.cutoff.value(),
            resonance: self.resonance.value(),
            lfo_rate:  self.lfo_rate.value(),
            lfo_depth: self.lfo_depth.value(),
            width:     self.width.value(),
        }
    }

    pub fn load (&self, params: &ReeseParams) {
        self.detune.set_value(params.detune);
        self.sub_level.set_value(params.sub_level);
        self.drive.set_value(params.drive);
        self.cutoff.set_value(params.cutoff);
        self.resonance.set_value(params.resonance);
        self.lfo_rate.set_value(params.lfo_rate);
        self.lfo_depth.set_value(params.lfo_depth);
        self.width.set_value(params.width);
    }
}

#[derive(Clone)]
pub struct ReeseVoice {
    unison:     [An<WaveSynth<U1>>; VOICES],
    unison_pan: [An<Panner<U2>>; VOICES],
    spread:     [f32; VOICES], // -1..1, fixed per-voice detune/pan weight

    sub: An<WaveSynth<U1>>,
    lfo: An<Sine<f64>>,

    filter_l: An<Svf<f64, LowpassMode<f64>>>,
    filter_r: An<Svf<f64, LowpassMode<f64>>>,

    handle: ReeseHandle,

    thump:         ThumpMod,
    thump_signal:  f32,
    filter_signal: f32,
    width_signal:  f32,
}

impl ReeseVoice {
    pub fn new (handle: ReeseHandle, thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> ReeseVoice {
        let spread: [f32; VOICES] = std::array::from_fn(|i| {
            if VOICES == 1 { 0.0 } else { (2.0 * i as f32 / (VOICES - 1) as f32) - 1.0 }
        });

        ReeseVoice {
            unison: std::array::from_fn(|_| saw()),
            unison_pan: std::array::from_fn(|_| panner()),
            spread,
            sub: saw(),
            lfo: sine(),
            filter_l: lowpass(),
            filter_r: lowpass(),
            handle,
            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            thump_signal: 0.0, filter_signal: 0.0, width_signal: 0.0,
        }
    }
}

impl AudioNode for ReeseVoice {
    const ID: u64 = 0x7A_11;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        let freq = freq * self.thump.tick(self.thump_signal);

        let lfo_val   = self.lfo.filter_mono(self.handle.lfo_rate.value()); // -1..1
        let lfo_depth = self.handle.lfo_depth.value();

        // §3/§6: detune scales with pitch automatically (ratio, not Hz offset),
        // wobbled slowly by the LFO for "motion".
        let detune = self.handle.detune.value() * (1.0 + lfo_val * lfo_depth * DETUNE_LFO_DEPTH);

        // width signal spreads/collapses the unison pan stack live -- at
        // width=0 every voice collapses to center (still beating, just mono).
        let width = (self.handle.width.value() + self.width_signal).clamp(0.0, 1.0);

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;
        for v in 0..VOICES {
            let spread = self.spread[v];
            let ratio  = cents_to_ratio(spread * detune);
            let sample = self.unison[v].filter_mono(freq * ratio);
            let lr = self.unison_pan[v].tick(&Frame::from([sample, spread * width]));
            mix_l += lr[0];
            mix_r += lr[1];
        }
        let norm = 1.0 / (VOICES as f32).sqrt();
        mix_l *= norm;
        mix_r *= norm;

        // Sub layer: unpanned/centered on purpose, keeps the low end mono-
        // compatible without a full low/high crossover split (§11).
        let sub_ratio = SUB_RATIO * cents_to_ratio(SUB_DETUNE_CENTS);
        let sub = self.sub.filter_mono(freq * sub_ratio) * self.handle.sub_level.value();
        mix_l += sub;
        mix_r += sub;

        // §8: tanh soft-clip pre-filter, for harmonic richness.
        let drive = self.handle.drive.value().max(1.0);
        let shaped_l = (mix_l * drive).tanh();
        let shaped_r = (mix_r * drive).tanh();

        // §7: live SVF lowpass, cutoff driven by the macro knob (scaled by
        // the live `filter` signal, same idiom as growl.rs) and the LFO.
        let cutoff_macro = (self.handle.cutoff.value() * self.filter_signal).clamp(0.0, 1.0);
        let cutoff_base  = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff_macro);
        let cutoff_hz    = (cutoff_base * 2f32.powf(lfo_val * lfo_depth * LFO_DEPTH_OCTAVES)).clamp(20.0, 18_000.0);
        let q = self.handle.resonance.value();

        let out_l = self.filter_l.tick(&Frame::from([shaped_l, cutoff_hz, q]))[0];
        let out_r = self.filter_r.tick(&Frame::from([shaped_r, cutoff_hz, q]))[0];

        Frame::from([out_l, out_r])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        for osc in self.unison.iter_mut() { osc.set_sample_rate(sample_rate); }
        self.sub.set_sample_rate(sample_rate);
        self.lfo.set_sample_rate(sample_rate);
        self.filter_l.set_sample_rate(sample_rate);
        self.filter_r.set_sample_rate(sample_rate);
        self.thump.set_sample_rate(sample_rate);
    }
}

impl Voice for ReeseVoice {
    const INDEX: usize = 2;
    fn name (&self) -> &'static str { "Reese" }
    fn set_signal (&mut self, signal: &SignalState) {
        self.thump_signal  = signal.thump;
        self.filter_signal = signal.filter;
        self.width_signal  = signal.width;
    }
}
