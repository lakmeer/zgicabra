
//
// Reese -- classic detuned-unison-saw bass, per ref/reese-bass-dsp-guide.md.
// VOICES band-limited saws spread symmetrically in cents (odd count so one
// voice anchors at zero detune/center pan), each panned via fundsp's
// equal-power panner() so live `width` spreads/collapses the stack. A
// detuned sub-octave layer adds weight and stays unpanned/centered for a
// phase-coherent low end. A slow LFO animates detune spread and filter
// cutoff together; tanh soft-clip sits pre-filter for harmonic richness.
//

use fundsp::prelude64::*;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::voice::{Voice, ThumpMod};

const VOICES: usize = 7; // odd -- center voice lands at zero detune/pan

const SUB_RATIO: f32 = 0.5;         // one octave down
const SUB_DETUNE_CENTS: f32 = -6.0; // keeps the sub from phase-locking to voice 0

const CUTOFF_LO: f32 = 80.0;
const CUTOFF_HI: f32 = 6000.0;
const LFO_DEPTH_OCTAVES: f32 = 2.5; // max cutoff swing at lfo_depth = 1
const DETUNE_LFO_DEPTH: f32 = 0.35; // detune spread wobble, fraction of base detune

fn cents_to_ratio (cents: f32) -> f32 { 2f32.powf(cents / 1200.0) }

// Fixed defaults, formerly ReeseParams::default() -- seeded directly into
// the Shared cells below now that there's no separate snapshot/handle shape.
const DEFAULT_DETUNE:    f32 = 24.0; // unison spread, cents
const DEFAULT_SUB_LEVEL: f32 = 0.5;
const DEFAULT_DRIVE:     f32 = 3.0;  // pre-filter tanh saturation
const DEFAULT_CUTOFF:    f32 = 0.6;  // macro 0..1, scaled by live `filter` signal (see growl.rs precedent)
const DEFAULT_RESONANCE: f32 = 1.2;  // filter Q
const DEFAULT_LFO_RATE:  f32 = 0.3;  // Hz
const DEFAULT_LFO_DEPTH: f32 = 0.35; // 0..1, drives both cutoff and detune wobble
const DEFAULT_WIDTH:     f32 = 0.7;  // base stereo spread, 0..1, added to live `width` signal

#[derive(Clone)]
pub struct ReeseVoice {
    unison:     [An<WaveSynth<U1>>; VOICES],
    unison_pan: [An<Panner<U2>>; VOICES],
    spread:     [f32; VOICES], // -1..1, fixed per-voice detune/pan weight

    sub: An<WaveSynth<U1>>,
    lfo: An<Sine<f64>>,

    filter_l: An<Svf<f64, LowpassMode<f64>>>,
    filter_r: An<Svf<f64, LowpassMode<f64>>>,

    pub detune:    Shared,
    pub sub_level: Shared,
    pub drive:     Shared,
    pub cutoff:    Shared,
    pub resonance: Shared,
    pub lfo_rate:  Shared,
    pub lfo_depth: Shared,
    pub width:     Shared,

    thump:         ThumpMod,
    thump_signal:  f32,
    filter_signal: f32,
    width_signal:  f32,
    fuzz_signal:   f32,
}

// Read-only-from-outside view onto ReeseVoice's Shared cells -- see
// GrowlView's doc in growl.rs for why this exists.
#[derive(Clone)]
pub struct ReeseView {
    pub detune:    Shared,
    pub sub_level: Shared,
    pub drive:     Shared,
    pub cutoff:    Shared,
    pub resonance: Shared,
    pub lfo_rate:  Shared,
    pub lfo_depth: Shared,
    pub width:     Shared,
}

impl ReeseView {
    // CC-settable fields only (see apply_cc below) -- filter_signal etc. are
    // live per-tick signals, not persisted state.
    pub fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("detune",    self.detune.value()),
            ("sub_level", self.sub_level.value()),
            ("drive",     self.drive.value()),
            ("cutoff",    self.cutoff.value()),
            ("resonance", self.resonance.value()),
            ("lfo_rate",  self.lfo_rate.value()),
            ("lfo_depth", self.lfo_depth.value()),
            ("width",     self.width.value()),
        ]
    }

    pub fn apply (&self, fields: &[(String, f32)]) {
        for (name, value) in fields {
            match name.as_str() {
                "detune"    => self.detune.set_value(*value),
                "sub_level" => self.sub_level.set_value(*value),
                "drive"     => self.drive.set_value(*value),
                "cutoff"    => self.cutoff.set_value(*value),
                "resonance" => self.resonance.set_value(*value),
                "lfo_rate"  => self.lfo_rate.set_value(*value),
                "lfo_depth" => self.lfo_depth.set_value(*value),
                "width"     => self.width.set_value(*value),
                _ => {},
            }
        }
    }
}

impl ReeseVoice {
    pub fn view (&self) -> ReeseView {
        ReeseView {
            detune:    self.detune.clone(),
            sub_level: self.sub_level.clone(),
            drive:     self.drive.clone(),
            cutoff:    self.cutoff.clone(),
            resonance: self.resonance.clone(),
            lfo_rate:  self.lfo_rate.clone(),
            lfo_depth: self.lfo_depth.clone(),
            width:     self.width.clone(),
        }
    }

    pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> ReeseVoice {
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

            detune:    shared(DEFAULT_DETUNE),
            sub_level: shared(DEFAULT_SUB_LEVEL),
            drive:     shared(DEFAULT_DRIVE),
            cutoff:    shared(DEFAULT_CUTOFF),
            resonance: shared(DEFAULT_RESONANCE),
            lfo_rate:  shared(DEFAULT_LFO_RATE),
            lfo_depth: shared(DEFAULT_LFO_DEPTH),
            width:     shared(DEFAULT_WIDTH),

            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            thump_signal: 0.0, filter_signal: 0.0, width_signal: 0.0, fuzz_signal: 0.0,
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

        let lfo_val   = self.lfo.filter_mono(self.lfo_rate.value()); // -1..1
        let lfo_depth = self.lfo_depth.value();

        // Detune scales with pitch (ratio, not Hz offset), wobbled by the LFO,
        // and widened live by hand span -- wide hands, wide unison spread.
        let width_signal = self.width_signal.clamp(0.0, 1.0);
        let detune = self.detune.value()
            * (1.0 + lfo_val * lfo_depth * DETUNE_LFO_DEPTH)
            * (1.0 + width_signal);

        // At width=0 every voice collapses to center (still beating, just mono).
        let width = (self.width.value() + self.width_signal).clamp(0.0, 1.0);

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

        // Sub layer stays unpanned/centered -- keeps the low end mono-compatible.
        let sub_ratio = SUB_RATIO * cents_to_ratio(SUB_DETUNE_CENTS);
        let sub = self.sub.filter_mono(freq * sub_ratio) * self.sub_level.value();
        mix_l += sub;
        mix_r += sub;

        // Live `fuzz` signal boosts drive on top of the macro knob.
        let drive = (self.drive.value() * (1.0 + self.fuzz_signal.clamp(0.0, 1.0))).max(1.0);
        let shaped_l = (mix_l * drive).tanh();
        let shaped_r = (mix_r * drive).tanh();

        // Cutoff driven by the macro knob (scaled by live `filter` signal) and the LFO.
        let cutoff_macro = (self.cutoff.value() * self.filter_signal).clamp(0.0, 1.0);
        let cutoff_base  = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff_macro);
        let cutoff_hz    = (cutoff_base * 2f32.powf(lfo_val * lfo_depth * LFO_DEPTH_OCTAVES)).clamp(20.0, 18_000.0);
        let q = self.resonance.value();

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
    const INDEX: usize = 0;
    fn name (&self) -> &'static str { "Reese" }
    fn set_signal (&mut self, _bend: f32, filter: f32, fuzz: f32, width: f32, thump: f32) {
        self.thump_signal  = thump;
        self.filter_signal = filter;
        self.width_signal  = width;
        self.fuzz_signal   = fuzz;
    }

    // CC 20-27, 0..1 normalized input scaled to each param's own range.
    // CC2-8 is the
    // same set of knobs (CC1 being reserved for the global Mod Wheel ->
    // filter mapping, see hydra/midi.rs) so a controller with only 8 physical
    // knobs can still reach them live -- `width` doesn't fit in that 7-slot
    // bank, so it's only reachable via CC27 here.
    fn apply_cc (&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            20 | 2 => self.detune.set_value(value * 50.0),
            21 | 3 => self.sub_level.set_value(value),
            22 | 4 => self.drive.set_value(1.0 + value * 7.0),
            23 | 5 => self.cutoff.set_value(value),
            24 | 6 => self.resonance.set_value(0.3 + value * 2.7),
            25 | 7 => self.lfo_rate.set_value(0.05 + value * 2.95),
            26 | 8 => self.lfo_depth.set_value(value),
            27 => self.width.set_value(value),
            _ => {},
        }
    }
}
