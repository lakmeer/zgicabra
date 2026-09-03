
//
// Reese
//

use std::sync::Arc;

use fundsp::prelude64::*;
use zgicabra_voice_macro::voice;

use crate::tools::linexp;
use super::signal::SharedSignal;
use super::voice::{Voice, VoiceDsp, ThumpMod, KnobPickup};
use super::crusher::{crusher, LOW_MID_HZ, MID_HIGH_HZ};
use super::sample::{Sample, SamplePlayer, play_sample};
use super::stutter::stutter;
use super::nam::load_named_model;
use super::nam_graph::nam_band;
use super::nam_node::NAM_WINDOW;

const VOICES: usize = 8; // odd -- center voice lands at zero detune/pan

static IMPACT_SAMPLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/wav/kick_dry.wav"));

fn load_impact_player () -> An<SamplePlayer> {
    let sample = Sample::parse(IMPACT_SAMPLE);
    play_sample(&Arc::new(sample))
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
const DEFAULT_RESONANCE: f32 = 2.2;  // filter Q
const DEFAULT_LFO_RATE:  f32 = 0.3;
const DEFAULT_LFO_DEPTH: f32 = 0.35;
const DEFAULT_WIDTH:     f32 = 0.7;

const WIDTH_TO_LFO_RATE: f32 = 2.0;
const WIDTH_TO_DETUNE:   f32 = 2.0;
const FILTER_TO_CUTOFF:  f32 = 6.0;

const IMPACT_ENV_ATTACK:  f32 = 0.0005; // near-instant catch on the transient
const IMPACT_ENV_RELEASE: f32 = 0.01;   // trails off with the sample's decay
const IMPACT_CUTOFF_POP:  f32 = 1000.0; // Hz added to cutoff at full impact envelope
const IMPACT_DRIVE_POP:   f32 = 1.5;    // extra drive multiplier at full impact envelope

const DEFAULT_CRUSH_DEPTH: f32 = 1.0;

const FEEDBACK_MODE_OCTAVE: f32 = 3.0;
const FEEDBACK_NAM_MODEL: &str = "lowgain";

const FEEDBACK_IR_SAMPLE_RATE: f64 = 48_000.0;
const FEEDBACK_IR_LEN_SAMPLES: usize = 400;
const FEEDBACK_RES_BW_FRACTION: f32 = 0.015; // resonator bandwidth as a fraction of its center freq -- narrow enough to ring rather than pass broadband noise
const FEEDBACK_EXCITE_LEVEL:    f32 = 0.2;   // how much of the dry (post note_env) voice signal continuously excites the resonator
const FEEDBACK_LOOP_GAIN_MAX:   f32 = 1.8;   // loop gain at feedback_attn = 1 -- comfortably above unity so the top of the knob range can self-sustain
const FEEDBACK_OUT_LEVEL:       f32 = 1.0;   // final mix trim for the loop output

const DELAY_ENV_ATTACK_SEC:    f32 = 0.0005; // near-instant, matches IMPACT_ENV_ATTACK
const DELAY_ENV_BASE_SEC:      f32 = 1.0;    // delay/decay time at DELAY_ENV_REF_FREQ
const DELAY_ENV_REF_FREQ:      f32 = 110.0;  // A2 -- octave reference for delay scaling
const DELAY_ENV_OCTAVE_FACTOR: f32 = 0.5;    // delay time multiplier per octave above reference
const DELAY_ENV_MIN_SEC:       f32 = 0.02;   // floor so it never hits zero/negative

static FEEDBACK_IR_SAMPLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/wav/mesa_ir.wav"));

fn load_feedback_ir () -> Wave {
    let sample = Sample::parse(FEEDBACK_IR_SAMPLE);
    let len = std::cmp::min(sample.length(), FEEDBACK_IR_LEN_SAMPLES);
    let data: Vec<f32> = (0..len).map(|i| sample.at(i)).collect();
    Wave::from_samples(FEEDBACK_IR_SAMPLE_RATE, &data)
}

#[derive(Clone)]
struct DelayEnv {
    trigger_seen:    f32,
    elapsed_samples: f32,
    sample_rate:     f32,
}

impl DelayEnv {
    fn new (trigger_seen: f32) -> DelayEnv {
        DelayEnv { trigger_seen, elapsed_samples: 0.0, sample_rate: DEFAULT_SR as f32 }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }

    fn tick (&mut self, trigger: f32, delay_sec: f32) -> f32 {
        if trigger != self.trigger_seen {
            self.trigger_seen = trigger;
            self.elapsed_samples = 0.0;
        }

        let t = self.elapsed_samples / self.sample_rate;
        self.elapsed_samples += 1.0;

        let attack = (t / DELAY_ENV_ATTACK_SEC).clamp(0.0, 1.0);
        let decay  = (-5.0 * t / delay_sec.max(0.001)).exp();
        attack * decay
    }
}

#[voice(index = 0, label = "Reese", new = manual)]
#[derive(Clone)]
pub struct ReeseVoice {
    #[node(each)] unison: [An<WaveSynth<U1>>; VOICES],
    unison_pan: [An<Panner<U2>>; VOICES],
    spread:     [f32; VOICES], // -1..1, fixed per-voice detune/pan weight

    #[node] sub: An<WaveSynth<U1>>,
    #[node] lfo: An<Sine<f64>>,
    #[node] stutter: An<Unit<U1, U1>>,

    #[node] feedback_res_l:  An<Resonator<f64, U3>>,
    #[node] feedback_res_r:  An<Resonator<f64, U3>>,
    #[node] feedback_nam_l:  Box<dyn AudioUnit>,
    #[node] feedback_nam_r:  Box<dyn AudioUnit>,
    #[node] feedback_conv_l: An<Convolver>,
    #[node] feedback_conv_r: An<Convolver>,

    feedback_nam_blend:   Shared, // pinned to 1.0 -- fully wet always, feedback_attn controls loop gain instead
    feedback_nam_level_l: Shared, // nam_band's post-blend peak monitor -- required by its signature, unused for now
    feedback_nam_level_r: Shared,
    feedback_loop_l: f32, // previous sample's gain-staged, clamped loopback -- this sample's resonator excitation
    feedback_loop_r: f32,

    // Unrelated second envelope: instant attack, delay/decay time shortens
    // as the note gets higher -- see DelayEnv. Gates stutter_level_input.
    delay_env: DelayEnv,
    #[live(range = 0.0..1.0)] pub delay_env_live: Shared,

    #[node] filter_l: An<Svf<f64, LowpassMode<f64>>>,
    #[node] filter_r: An<Svf<f64, LowpassMode<f64>>>,

    drive_input:                                                      Shared,
    #[knob(range = 0.0..50.0, set = |v| v * 50.0)]        pub detune_input:    Shared,
    #[knob(range = 0.0..1.0)]               pub sub_level_input: Shared,
    #[knob(range = 0.0..1.0)]               pub cutoff_input:    Shared,
    #[knob(range = 0.3..3.0,  set = |v| 0.3 + v * 2.7)]   pub resonance_input: Shared,
    #[knob(range = 0.05..3.0, set = |v| 0.05 + v * 2.95)] pub lfo_rate_input:  Shared,
    #[knob(range = 0.0..1.0)]               pub lfo_depth_input: Shared,
    #[knob(range = 0.0..1.0,  default = 0.0)] pub stutter_level_input: Shared,
    #[knob(range = 0.0..1.0,  default = 0.0)] pub feedback_attn: Shared, // feedback loop gain -- 0 is always inert; crosses unity (self-sustaining) partway up

    #[live(range = 0.0..5.0)]    pub drive_live:      Shared,
    #[live(range = 0.0..5.0)]    pub lfo_rate_live:   Shared,
    #[live(range = 0.0..200.0)]  pub detune_live:     Shared,
    #[live(range = 0.0..6000.0)] pub cutoff_live:     Shared,
    #[live(range = 0.0..1.0)]    pub loop_gain_live: Shared,

    impact_player:           An<SamplePlayer>,
    #[node] impact_env:      An<AFollow<f64>>, // tracks impact sample's amplitude, drives cutoff/drive pop
    impact_trigger:          Shared, // clone of thump_trigger -- bumped once per NoteStart
    impact_trigger_seen:     f32,
    #[knob(range = 0.0..1.0)] pub impact_level_input: Shared, // persisted, no CC of its own

    #[node] crusher_l: Box<dyn AudioUnit>,
    #[node] crusher_r: Box<dyn AudioUnit>,

    #[live(range = -60.0..0.0)] pub crush_env_live: Shared,
    #[live(range = -60.0..0.0)] pub crush_out_live: Shared,
    #[live(range = -60.0..0.0)] pub crush_gr_live:  Shared,

    thump: ThumpMod,
    sig:   SharedSignal,
}

impl ReeseVoice {
    pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared, signal: SharedSignal) -> ReeseVoice {
        let spread: [f32; VOICES] = std::array::from_fn(|i| {
            if VOICES == 1 { 0.0 } else { (2.0 * i as f32 / (VOICES - 1) as f32) - 1.0 }
        });

        let crush_env_live = shared(0.0);
        let crush_out_live = shared(0.0);
        let crush_gr_live  = shared(0.0);

        let feedback_nam_slot_l = load_named_model(FEEDBACK_NAM_MODEL).unwrap();
        let feedback_nam_slot_r = load_named_model(FEEDBACK_NAM_MODEL).unwrap();

        let feedback_nam_blend   = shared(1.0);
        let feedback_nam_level_l = shared(0.0);
        let feedback_nam_level_r = shared(0.0);

        let feedback_nam_l: Box<dyn AudioUnit> = Box::new(nam_band(
            &feedback_nam_slot_l, &feedback_nam_blend, &feedback_nam_level_l, NAM_WINDOW,
        ));

        let feedback_nam_r: Box<dyn AudioUnit> = Box::new(nam_band(
            &feedback_nam_slot_r, &feedback_nam_blend, &feedback_nam_level_r, NAM_WINDOW,
        ));

        let feedback_ir = load_feedback_ir();

        ReeseVoice {
            unison: std::array::from_fn(|_| saw()),
            unison_pan: std::array::from_fn(|_| panner()),
            spread,
            sub: saw(),
            lfo: sine(),
            stutter: stutter(),

            feedback_res_l:  resonator(),
            feedback_res_r:  resonator(),
            feedback_nam_l,
            feedback_nam_r,
            feedback_conv_l: convolve(&feedback_ir, 0),
            feedback_conv_r: convolve(&feedback_ir, 0),
            feedback_nam_blend,
            feedback_nam_level_l,
            feedback_nam_level_r,
            feedback_loop_l: 0.0,
            feedback_loop_r: 0.0,

            delay_env: DelayEnv::new(thump_trigger.value()),
            delay_env_live: shared(0.0),

            filter_l: lowpass(),
            filter_r: lowpass(),

            detune_input:    shared(DEFAULT_DETUNE),
            sub_level_input: shared(DEFAULT_SUB_LEVEL),
            drive_input:     shared(DEFAULT_DRIVE),
            cutoff_input:    shared(DEFAULT_CUTOFF),
            resonance_input: shared(DEFAULT_RESONANCE),
            lfo_rate_input:  shared(DEFAULT_LFO_RATE),
            lfo_depth_input: shared(DEFAULT_LFO_DEPTH),
            stutter_level_input: shared(0.0),
            feedback_attn:       shared(0.0),

            drive_live:      shared(0.0),
            lfo_rate_live:   shared(0.0),
            detune_live:     shared(0.0),
            cutoff_live:     shared(0.0),
            loop_gain_live:  shared(0.0),

            impact_player:       load_impact_player(),
            impact_env:          afollow(IMPACT_ENV_ATTACK, IMPACT_ENV_RELEASE),
            impact_trigger:      thump_trigger.clone(),
            impact_trigger_seen: thump_trigger.value(),
            impact_level_input:  shared(1.0),

            crusher_l: Box::new(crusher(&shared(1.0), shared(LOW_MID_HZ), shared(MID_HIGH_HZ), crush_env_live.clone(), crush_out_live.clone(), crush_gr_live.clone(),)),
            crusher_r: Box::new(crusher(&shared(1.0), shared(LOW_MID_HZ), shared(MID_HIGH_HZ), shared(0.0), shared(0.0), shared(0.0),)),

            crush_env_live, crush_out_live, crush_gr_live,

            selected_knob: shared(0.0),
            knob_pickup:   KnobPickup::new(),

            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            sig:   signal,
        }
    }
}

impl VoiceDsp for ReeseVoice {
    fn render (&mut self, freq: f32, _thump_mult: f32) -> Frame<f32, U2> {
        self.lfo_rate_live.set_value(self.lfo_rate_input.value() + (1.0 + WIDTH_TO_LFO_RATE * self.sig.width.value()).clamp(0.0, 1.0));
        let lfo_val   = self.lfo.filter_mono(self.lfo_rate_live.value());
        let lfo_depth = self.lfo_depth_input.value();

        let width_signal = self.sig.width.value().clamp(0.0, 1.0);
        let detune = self.detune_input.value()
            * (1.0 + lfo_val * lfo_depth * DETUNE_LFO_DEPTH)
            * (1.0 + WIDTH_TO_DETUNE * width_signal);
        self.detune_live.set_value(detune);

        let trigger = self.impact_trigger.value();
        if trigger != self.impact_trigger_seen {
            self.impact_trigger_seen = trigger;
            self.impact_player.reset();
            self.feedback_loop_l = 0.0;
            self.feedback_loop_r = 0.0;
        }

        let octaves   = (freq / DELAY_ENV_REF_FREQ).log2();
        let delay_sec = (DELAY_ENV_BASE_SEC * DELAY_ENV_OCTAVE_FACTOR.powf(octaves)).max(DELAY_ENV_MIN_SEC);
        self.delay_env_live.set_value(self.delay_env.tick(trigger, delay_sec));

        let impact_raw = self.impact_player.get_mono() * self.impact_level_input.value() * self.sig.thump.value();
        let impact_env = self.impact_env.filter_mono(impact_raw.abs());

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

        let sub_ratio = SUB_RATIO * cents_to_ratio(SUB_DETUNE_CENTS);
        let sub = self.sub.filter_mono(freq * sub_ratio) * self.sub_level_input.value();
        mix_l += sub;
        mix_r += sub;

        mix_l += impact_raw;
        mix_r += impact_raw;

        let stutter = self.stutter.filter_mono(freq) * self.stutter_level_input.value() * self.delay_env_live.value();
        mix_l += stutter;
        mix_r += stutter;

        let note_env = self.sig.env.value();
        mix_l *= note_env;
        mix_r *= note_env;

        let loop_gain = self.feedback_attn.value() * self.sig.aux.value() * FEEDBACK_LOOP_GAIN_MAX;
        self.loop_gain_live.set_value(loop_gain);

        let exciter_l = mix_l * FEEDBACK_EXCITE_LEVEL;
        let exciter_r = mix_r * FEEDBACK_EXCITE_LEVEL;

        let center_hz    = freq * 2f32.powf(FEEDBACK_MODE_OCTAVE);
        let bandwidth_hz = (center_hz * FEEDBACK_RES_BW_FRACTION).max(1.0);

        let res_in_l = exciter_l + self.feedback_loop_l;
        let res_in_r = exciter_r + self.feedback_loop_r;
        let res_out_l = self.feedback_res_l.tick(&Frame::from([res_in_l, center_hz, bandwidth_hz]))[0];
        let res_out_r = self.feedback_res_r.tick(&Frame::from([res_in_r, center_hz, bandwidth_hz]))[0];

        let nam_out_l = self.feedback_nam_l.filter_mono(res_out_l);
        let nam_out_r = self.feedback_nam_r.filter_mono(res_out_r);

        let conv_out_l = self.feedback_conv_l.filter_mono(nam_out_l);
        let conv_out_r = self.feedback_conv_r.filter_mono(nam_out_r);

        // Safety clamp: this is a real recursive gain loop, so bound it
        // explicitly rather than trusting the NAM's saturation alone to
        // keep it finite for every knob combination.
        self.feedback_loop_l = (conv_out_l * loop_gain).tanh();
        self.feedback_loop_r = (conv_out_r * loop_gain).tanh();

        let feedback_mix_gain = 1.0 - note_env;
        mix_l += self.feedback_loop_l * FEEDBACK_OUT_LEVEL * feedback_mix_gain;
        mix_r += self.feedback_loop_r * FEEDBACK_OUT_LEVEL * feedback_mix_gain;

        let drive = (self.drive_input.value() * (1.0 + IMPACT_DRIVE_POP * impact_env)).max(1.0);
        self.drive_live.set_value(drive);
        let shaped_l = (mix_l * self.drive_live.value()).tanh();
        let shaped_r = (mix_r * self.drive_live.value()).tanh();

        let cutoff_base  = self.cutoff_input.value() * 2f32.powf(lfo_val * lfo_depth * LFO_DEPTH);
        let cutoff_hz  = (linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff_base * (1.0 + FILTER_TO_CUTOFF * self.sig.filter.value())) + IMPACT_CUTOFF_POP * impact_env).clamp(20.0, 18_000.0);
        self.cutoff_live.set_value(cutoff_hz);
        let q = self.resonance_input.value();

        let out_l = self.filter_l.tick(&Frame::from([shaped_l, cutoff_hz, q]))[0];
        let out_r = self.filter_r.tick(&Frame::from([shaped_r, cutoff_hz, q]))[0];

        let wet_l = self.crusher_l.filter_mono(out_l);
        let wet_r = self.crusher_r.filter_mono(out_r);

        Frame::from([wet_l * 0.6, wet_r * 0.6])
    }

    fn on_set_sample_rate (&mut self, sample_rate: f64) {
        self.delay_env.set_sample_rate(sample_rate);
    }
}
