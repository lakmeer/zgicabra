
//
// Reese -- classic detuned-unison-saw bass, per ref/reese-bass-dsp-guide.md.
// VOICES band-limited saws spread symmetrically in cents (odd count so one
// voice anchors at zero detune/center pan), each panned via fundsp's
// equal-power panner() so live `width` spreads/collapses the stack. A
// detuned sub-octave layer adds weight and stays unpanned/centered for a
// phase-coherent low end. A slow LFO animates detune spread and filter
// cutoff together; tanh soft-clip sits pre-filter for harmonic richness.
//

use std::sync::Arc;

use fundsp::prelude64::*;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::voice::{Voice, ThumpMod};

const VOICES: usize = 8; // odd -- center voice lands at zero detune/pan

// Impact/kick sample embedded straight into the binary at compile time --
// same reasoning as nam.rs's NAM_MODELS (see there): the boot-time systemd
// service execs zgicabra from target/release with no reliable runtime wav/
// folder alongside it.
static IMPACT_SAMPLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/wav/kick_dry.wav"));

// Decodes the embedded kick sample into a one-shot mono WavePlayer via
// fundsp's own playwave() builtin (no loop_point -- it plays through once,
// then sits silent until reset()).
fn load_impact_player () -> An<WavePlayer> {
    let wave = Wave::load_slice(IMPACT_SAMPLE).expect("failed to decode embedded wav/kick_dry.wav");
    playwave(&Arc::new(wave), 0, None)
}

const SUB_RATIO: f32 = 0.5;         // one octave down
const SUB_DETUNE_CENTS: f32 = -6.0; // keeps the sub from phase-locking to voice 0

const CUTOFF_LO: f32 = 80.0;
const CUTOFF_HI: f32 = 6000.0;
const LFO_DEPTH: f32 = 0.15;
const DETUNE_LFO_DEPTH: f32 = 0.35; // detune spread wobble, fraction of base detune

fn cents_to_ratio (cents: f32) -> f32 { 2f32.powf(cents / 1200.0) }

const DEFAULT_DETUNE:    f32 = 24.0; // unison spread, cents
const DEFAULT_SUB_LEVEL: f32 = 0.5;
const DEFAULT_DRIVE:     f32 = 3.0;  // pre-filter tanh saturation
const DEFAULT_CUTOFF:    f32 = 0.6;
const DEFAULT_RESONANCE: f32 = 1.2;  // filter Q
const DEFAULT_LFO_RATE:  f32 = 0.3;
const DEFAULT_LFO_DEPTH: f32 = 0.35;
const DEFAULT_WIDTH:     f32 = 0.7;

const WIDTH_TO_LFO_RATE: f32 = 2.0;
const WIDTH_TO_DETUNE:   f32 = 2.0;
const FILTER_TO_CUTOFF:  f32 = 6.0;

#[derive(Clone)]
pub struct ReeseVoice {
    unison:     [An<WaveSynth<U1>>; VOICES],
    unison_pan: [An<Panner<U2>>; VOICES],
    spread:     [f32; VOICES], // -1..1, fixed per-voice detune/pan weight

    sub: An<WaveSynth<U1>>,
    lfo: An<Sine<f64>>,

    filter_l: An<Svf<f64, LowpassMode<f64>>>,
    filter_r: An<Svf<f64, LowpassMode<f64>>>,

    pub detune_input:    Shared,
    pub sub_level_input: Shared,
    pub drive_input:     Shared,
    pub cutoff_input:    Shared,
    pub resonance_input: Shared,
    pub lfo_rate_input:  Shared,
    pub lfo_depth_input: Shared,

    pub drive_live:      Shared,
    pub lfo_rate_live:   Shared,
    pub detune_live:     Shared,
    pub cutoff_live:     Shared,

    impact_player:           An<WavePlayer>,
    impact_trigger:          Shared, // clone of thump_trigger -- bumped once per NoteStart
    impact_trigger_seen:     f32,
    pub impact_level_input:  Shared,

    thump:         ThumpMod,
    thump_signal:  f32,
    filter_signal: f32,
    width_signal:  f32,
    fuzz_signal:   f32,
}

// Read-only-from-outside view onto ReeseVoice's Shared cells -- see
// GrowlView's doc in growl.rs for why this exists. `_input` fields are the
// authored knob values; `_live` fields are read-only, written by ReeseVoice
// each tick, and show the actual post-modulation values the DSP is using --
// for visualisation only.
#[derive(Clone)]
pub struct ReeseView {
    pub detune_input:    Shared,
    pub sub_level_input: Shared,
    pub drive_input:     Shared,
    pub cutoff_input:    Shared,
    pub resonance_input: Shared,
    pub lfo_rate_input:  Shared,
    pub lfo_depth_input: Shared,

    pub drive_live:      Shared,
    pub lfo_rate_live:   Shared,
    pub detune_live:     Shared,
    pub cutoff_live:     Shared,

    pub impact_level_input: Shared,
}

impl ReeseView {
    // CC-settable fields only (see apply_cc below) -- `_live` fields are
    // computed per-tick from the live signal, not persisted.
    pub fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("detune_input",    self.detune_input.value()),
            ("sub_level_input", self.sub_level_input.value()),
            ("drive_input",     self.drive_input.value()),
            ("cutoff_input",    self.cutoff_input.value()),
            ("resonance_input", self.resonance_input.value()),
            ("lfo_rate_input",  self.lfo_rate_input.value()),
            ("lfo_depth_input", self.lfo_depth_input.value()),
            ("impact_level_input", self.impact_level_input.value()),
        ]
    }

    pub fn apply (&self, fields: &[(String, f32)]) {
        for (name, value) in fields {
            match name.as_str() {
                "detune_input"       => self.detune_input.set_value(*value),
                "sub_level_input"    => self.sub_level_input.set_value(*value),
                "drive_input"        => self.drive_input.set_value(*value),
                "cutoff_input"       => self.cutoff_input.set_value(*value),
                "resonance_input"    => self.resonance_input.set_value(*value),
                "lfo_rate_input"     => self.lfo_rate_input.set_value(*value),
                "lfo_depth_input"    => self.lfo_depth_input.set_value(*value),
                "impact_level_input" => self.impact_level_input.set_value(*value),
                _ => {},
            }
        }
    }
}

impl ReeseVoice {
    pub fn view (&self) -> ReeseView {
        ReeseView {
            detune_input:    self.detune_input.clone(),
            sub_level_input: self.sub_level_input.clone(),
            drive_input:     self.drive_input.clone(),
            cutoff_input:    self.cutoff_input.clone(),
            resonance_input: self.resonance_input.clone(),
            lfo_rate_input:  self.lfo_rate_input.clone(),
            lfo_depth_input: self.lfo_depth_input.clone(),
            drive_live:      self.drive_live.clone(),
            lfo_rate_live:   self.lfo_rate_live.clone(),
            detune_live:     self.detune_live.clone(),
            cutoff_live:     self.cutoff_live.clone(),
            impact_level_input: self.impact_level_input.clone(),
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

            detune_input:    shared(DEFAULT_DETUNE),
            sub_level_input: shared(DEFAULT_SUB_LEVEL),
            drive_input:     shared(DEFAULT_DRIVE),
            cutoff_input:    shared(DEFAULT_CUTOFF),
            resonance_input: shared(DEFAULT_RESONANCE),
            lfo_rate_input:  shared(DEFAULT_LFO_RATE),
            lfo_depth_input: shared(DEFAULT_LFO_DEPTH),

            drive_live:      shared(0.0),
            lfo_rate_live:   shared(0.0),
            detune_live:     shared(0.0),
            cutoff_live:     shared(0.0),

            impact_player:       load_impact_player(),
            impact_trigger:      thump_trigger.clone(),
            impact_trigger_seen: thump_trigger.value(),
            impact_level_input:  shared(1.0),

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

        let freq_mult = self.thump.tick(self.thump_signal);
        let freq = freq * freq_mult;

        self.lfo_rate_live.set_value(self.lfo_rate_input.value() + (1.0 + WIDTH_TO_LFO_RATE * self.width_signal).clamp(0.0, 1.0));
        let lfo_val   = self.lfo.filter_mono(self.lfo_rate_live.value());
        let lfo_depth = self.lfo_depth_input.value();

        let width_signal = self.width_signal.clamp(0.0, 1.0);
        let detune = self.detune_input.value()
            * (1.0 + lfo_val * lfo_depth * DETUNE_LFO_DEPTH)
            * (1.0 + WIDTH_TO_DETUNE * width_signal);
        self.detune_live.set_value(detune);

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for v in 0..VOICES {
            let spread = self.spread[v];
            let ratio  = cents_to_ratio(spread * detune);
            let sample = self.unison[v].filter_mono(freq * ratio);
            let lr = self.unison_pan[v].tick(&Frame::from([sample, spread * DEFAULT_WIDTH]));
            mix_l += lr[0];
            mix_r += lr[1];
        }

        let norm = 1.0 / (VOICES as f32).sqrt();
        mix_l *= norm;
        mix_r *= norm;

        // Sub layer stays unpanned/centered -- keeps the low end mono-compatible.
        let sub_ratio = SUB_RATIO * cents_to_ratio(SUB_DETUNE_CENTS);
        let sub = self.sub.filter_mono(freq * sub_ratio) * self.sub_level_input.value();
        mix_l += sub;
        mix_r += sub;

        // Live `fuzz` signal boosts drive on top of the macro knob.
        let drive = (self.drive_input.value() * (1.0 + 2.0 * self.fuzz_signal.clamp(0.0, 1.0))).max(1.0);
        self.drive_live.set_value(drive);
        let shaped_l = (mix_l * self.drive_live.value()).tanh();
        let shaped_r = (mix_r * self.drive_live.value()).tanh();

        // Cutoff driven by the macro knob (scaled by live `filter` signal) and the LFO.
        let cutoff_base  = self.cutoff_input.value() * 2f32.powf(lfo_val * lfo_depth * LFO_DEPTH);
        let cutoff_hz  = linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff_base * (1.0 + FILTER_TO_CUTOFF * self.filter_signal)).clamp(20.0, 18_000.0);
        self.cutoff_live.set_value(cutoff_hz);
        let q = self.resonance_input.value();

        let out_l = self.filter_l.tick(&Frame::from([shaped_l, cutoff_hz, q]))[0];
        let out_r = self.filter_r.tick(&Frame::from([shaped_r, cutoff_hz, q]))[0];

        let trigger = self.impact_trigger.value();
        if trigger != self.impact_trigger_seen {
            self.impact_trigger_seen = trigger;
            self.impact_player.reset();
        }

        let impact_out = self.impact_player.get_mono() * self.impact_level_input.value() * self.thump_signal;

        Frame::from([out_l + impact_out, out_r + impact_out])
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
    // bank, so it's only reachable via CC27 here. impact_level_input has no
    // CC of its own.
    fn apply_cc (&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            1 => self.detune_input.set_value(value * 50.0),
            2 => self.sub_level_input.set_value(value),
            3 => self.drive_input.set_value(1.0 + value * 7.0),
            4 => self.cutoff_input.set_value(value),
            5 => self.resonance_input.set_value(0.3 + value * 2.7),
            6 => self.lfo_rate_input.set_value(0.05 + value * 2.95),
            7 => self.lfo_depth_input.set_value(value),
            _ => {},
        }
    }
}
