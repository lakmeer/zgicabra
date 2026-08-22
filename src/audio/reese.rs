
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
use zgicabra_voice_macro::Voice;

use crate::tools::linexp;
use super::signal::SharedSignal;
use super::voice::{Voice, VoiceDsp, ThumpMod};
use super::crusher::crusher;

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

const IMPACT_ENV_ATTACK:  f32 = 0.0005; // near-instant catch on the transient
const IMPACT_ENV_RELEASE: f32 = 0.01;   // trails off with the sample's decay
const IMPACT_CUTOFF_POP:  f32 = 1000.0; // Hz added to cutoff at full impact envelope
const IMPACT_DRIVE_POP:   f32 = 1.5;    // extra drive multiplier at full impact envelope

const DEFAULT_CRUSH_DEPTH:  f32 = 1.0; // CC8 = 1.0 -> heaviest crush, 0.0 -> bypassed

#[derive(Clone, Voice)]
#[voice(index = 0, label = "Reese", new = manual)]
pub struct ReeseVoice {
    #[node(each)] unison: [An<WaveSynth<U1>>; VOICES],
    unison_pan: [An<Panner<U2>>; VOICES],
    spread:     [f32; VOICES], // -1..1, fixed per-voice detune/pan weight

    #[node] sub: An<WaveSynth<U1>>,
    #[node] lfo: An<Sine<f64>>,

    #[node] filter_l: An<Svf<f64, LowpassMode<f64>>>,
    #[node] filter_r: An<Svf<f64, LowpassMode<f64>>>,

    #[input(cc = "1", range = 0.0..50.0,  set = |v| v * 50.0)]        pub detune_input:    Shared,
    #[input(cc = "2", range = 0.0..1.0,   set = |v| v)]               pub sub_level_input: Shared,
    #[input(cc = "3", range = 1.0..8.0,   set = |v| 1.0 + v * 7.0)]   pub drive_input:     Shared,
    #[input(cc = "4", range = 0.0..1.0,   set = |v| v)]               pub cutoff_input:    Shared,
    #[input(cc = "5", range = 0.3..3.0,   set = |v| 0.3 + v * 2.7)]   pub resonance_input: Shared,
    #[input(cc = "6", range = 0.05..3.0,  set = |v| 0.05 + v * 2.95)] pub lfo_rate_input:  Shared,
    #[input(cc = "7", range = 0.0..1.0,   set = |v| v)]               pub lfo_depth_input: Shared,
    #[input(cc = "8", range = 0.0..1.0,   set = |v| v)]               pub crush_input:     Shared,

    #[live(range = 0.0..5.0)]    pub drive_live:      Shared,
    #[live(range = 0.0..5.0)]    pub lfo_rate_live:   Shared,
    #[live(range = 0.0..200.0)]  pub detune_live:     Shared,
    #[live(range = 0.0..6000.0)] pub cutoff_live:     Shared,

    impact_player:           An<WavePlayer>,
    #[node] impact_env:      An<AFollow<f64>>, // tracks impact sample's amplitude, drives cutoff/drive pop
    impact_trigger:          Shared, // clone of thump_trigger -- bumped once per NoteStart
    impact_trigger_seen:     f32,
    #[input(range = 0.0..1.0)] pub impact_level_input: Shared, // persisted, no CC of its own

    #[node] crusher_l: Box<dyn AudioUnit>,
    #[node] crusher_r: Box<dyn AudioUnit>,

    #[live(range = -60.0..0.0)] pub crush_env_live: Shared, // meter telemetry, from crusher_l -- see ui panel
    #[live(range = -60.0..0.0)] pub crush_out_live: Shared,
    #[live(range = -60.0..0.0)] pub crush_gr_live:  Shared,

    thump: ThumpMod,
    sig:   SharedSignal,
}

// ReeseView + view()/fields()/apply()/UI_RANGES + the AudioNode/Voice impls
// are generated by #[derive(Voice)]. new() stays hand-written (new = manual)
// because the per-voice detune/pan `spread` weights are computed before the
// struct literal.
impl ReeseVoice {
    pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared, signal: SharedSignal) -> ReeseVoice {
        let spread: [f32; VOICES] = std::array::from_fn(|i| {
            if VOICES == 1 { 0.0 } else { (2.0 * i as f32 / (VOICES - 1) as f32) - 1.0 }
        });

        // Crusher writes these itself every tick -- see crusher.rs.
        let crush_env_live = shared(0.0);
        let crush_out_live = shared(0.0);
        let crush_gr_live  = shared(0.0);
        let crush_input    = shared(DEFAULT_CRUSH_DEPTH);

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
            crush_input:     crush_input.clone(),

            drive_live:      shared(0.0),
            lfo_rate_live:   shared(0.0),
            detune_live:     shared(0.0),
            cutoff_live:     shared(0.0),

            impact_player:       load_impact_player(),
            impact_env:          afollow(IMPACT_ENV_ATTACK, IMPACT_ENV_RELEASE),
            impact_trigger:      thump_trigger.clone(),
            impact_trigger_seen: thump_trigger.value(),
            impact_level_input:  shared(1.0),

            crusher_l: Box::new(crusher(&crush_input, crush_env_live.clone(), crush_out_live.clone(), crush_gr_live.clone(),)),
            // Right channel's meters not wired
            crusher_r: Box::new(crusher(&crush_input, shared(0.0), shared(0.0), shared(0.0),)),

            crush_env_live, crush_out_live, crush_gr_live,

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
        }

        // Raw impact sample plus its tracked envelope -- the envelope drives
        // the cutoff/drive pops below, the raw sample gets folded into the
        // pre-drive mix so it shares the synth's saturation and filter sweep
        // rather than sitting on top as a separate dry layer.
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

        // Sub layer stays unpanned/centered -- keeps the low end mono-compatible.
        let sub_ratio = SUB_RATIO * cents_to_ratio(SUB_DETUNE_CENTS);
        let sub = self.sub.filter_mono(freq * sub_ratio) * self.sub_level_input.value();
        mix_l += sub;
        mix_r += sub;

        mix_l += impact_raw;
        mix_r += impact_raw;

        // Live `fuzz` signal and the impact envelope both boost drive on top
        // of the macro knob -- the kick's transient briefly adds extra grit.
        let drive = (self.drive_input.value()
            * (1.0 + 2.0 * self.sig.fuzz.value().clamp(0.0, 1.0))
            * (1.0 + IMPACT_DRIVE_POP * impact_env)).max(1.0);
        self.drive_live.set_value(drive);
        let shaped_l = (mix_l * self.drive_live.value()).tanh();
        let shaped_r = (mix_r * self.drive_live.value()).tanh();

        // Cutoff driven by the macro knob (scaled by live `filter` signal), the
        // LFO, and a pop from the impact envelope that opens the filter on hit.
        let cutoff_base  = self.cutoff_input.value() * 2f32.powf(lfo_val * lfo_depth * LFO_DEPTH);
        let cutoff_hz  = (linexp(0.0, 1.0, CUTOFF_LO, CUTOFF_HI, cutoff_base * (1.0 + FILTER_TO_CUTOFF * self.sig.filter.value())) + IMPACT_CUTOFF_POP * impact_env).clamp(20.0, 18_000.0);
        self.cutoff_live.set_value(cutoff_hz);
        let q = self.resonance_input.value();

        let out_l = self.filter_l.tick(&Frame::from([shaped_l, cutoff_hz, q]))[0];
        let out_r = self.filter_r.tick(&Frame::from([shaped_r, cutoff_hz, q]))[0];

        let mut wet_l = [0.0f32];
        let mut wet_r = [0.0f32];
        self.crusher_l.tick(&[out_l], &mut wet_l);
        self.crusher_r.tick(&[out_r], &mut wet_r);

        Frame::from([wet_l[0], wet_r[0]])
    }
}
