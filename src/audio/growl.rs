
use std::sync::Arc;

use fundsp::prelude64::*;
use fundsp::fft::inverse_fft;
use num_complex::Complex32;

use zgicabra_voice_macro::Voice;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::gen_node::GenNode;
use super::voice::{Voice, VoiceDsp, ThumpMod};
use super::nam::{NamStage, NAM_BLOCK_CAP};

const FRAME_LEN: usize = 256;    // power of two, required by fundsp::fft
const NUM_HARMONICS: usize = 32; // harmonics 1..=32 tracked per oscillator

const OSC_A_VOICES: usize = 1;
const OSC_A_PITCH_RATIO: f32 = 0.5; // octave down (osc_1 transpose = -12)
const OSC_A_LEVEL: f32 = 0.497;
const OSC_A_LFO_WEIGHT: f32 = 0.293;
const OSC_A_WOBBLE_DEPTH: f32 = 0.14; // random_1 -> osc_1_spectral_morph_amount

const OSC_B_VOICES: usize = 12;
const OSC_B_LEVEL: f32 = 0.517;
const OSC_B_LFO_WEIGHT: f32 = 0.413;
const OSC_B_BASE_SMEAR: f32 = 0.055;
const OSC_B_WARP_SMEAR_ADD: f32 = 0.135; // macro_control_4 (WARP) -> osc_2_spectral_morph_amount
//
const DETUNE_RANGE: f32 = 2.0;   // osc_2_detune_range
const UNISON_DETUNE: f32 = 2.9;  // osc_2_unison_detune

const NAM_MODEL: &str = "mesa";

const CROSSFADE_HZ: f32 = 0.15;  // stand-in for lfo_1 (tempo-synced ~1Hz in the patch)
const WOBBLE_HZ: f32 = 0.2;      // smoothing rate for the noise-based Perlin stand-in
const CONTROL_RATE_DIV: usize = 64; // Smear resynthesis runs at this coarser rate, not per-sample

const BASE_DRIVE: f32 = 3.1;
const BASE_HIGHPASS_HZ: f32 = 681.0;

const BASS_SHELF_HZ: f32 = 200.0;
const BASS_SHELF_GAIN: f32 = 6.0;
const BASS_DRIVE_EXTRA: f32 = 6.0;

const FILTER_CUTOFF_LO: f32 = 30.0;
const FILTER_CUTOFF_HI: f32 = 12000.0;
const FILTER_RESONANCE: f32 = 0.3;

const REVERB_ROOM_SIZE: f32 = 15.0;
const REVERB_TIME: f32 = 2.0;
const REVERB_DAMPING: f32 = 0.5;

// Recursive harmonic-magnitude carry across the harmonic index -- ported
// directly from Vital's smearMorph (spectral_morph.h:217-239). At
// smear=0 the spectrum is unchanged; increasing smear progressively
// overrides higher harmonics with energy carried up from lower ones.
fn smear_resynth (base: &[Complex32; NUM_HARMONICS], smear: f32) -> [f32; FRAME_LEN] {
    let mut spectrum = [Complex32::ZERO; FRAME_LEN];
    let mut running = base[0].norm() * (1.0 - smear);
    for h in 0..NUM_HARMONICS {
        let mag = base[h].norm();
        let phase_dir = if mag > 1e-9 { base[h] / mag } else { Complex32::new(1.0, 0.0) };
        let amp = mag + (running - mag) * smear;
        spectrum[h + 1] = amp * phase_dir;
        running = amp * (h as f32 + 1.25) / (h as f32 + 1.0);
    }

    inverse_fft(&mut spectrum);

    let z = FRAME_LEN as f32;
    let mut table = [0.0f32; FRAME_LEN];
    let mut peak = 0.0f32;
    for i in 0..FRAME_LEN {
        let s = spectrum[i].im * z;
        table[i] = s;
        peak = peak.max(s.abs());
    }
    if peak > 1e-6 {
        let g = 0.9 / peak;
        for i in 0..FRAME_LEN { table[i] *= g; }
    }
    table
}

// One Smear-morphed unison stack: a fixed base spectrum (stands in for
// Vital's hand-drawn "Quad Saw"/"Basic Shapes" wavetable frames -- porting
// the actual frame geometry is out of scope), resynthesized at a coarser
// control rate (see CONTROL_RATE_DIV) and played back by `phases.len()`
// detuned voices sharing that one table.
#[derive(Clone)]
struct SmearVoice {
    base_spectrum: [Complex32; NUM_HARMONICS],
    table: [f32; FRAME_LEN],
    last_smear: f32,
    phases: Vec<f32>,
    detune_ratios: Vec<f32>,
}

impl SmearVoice {
    fn new (voices: usize, detune_ratios: Vec<f32>) -> SmearVoice {
        let base_spectrum: [Complex32; NUM_HARMONICS] =
            std::array::from_fn(|h| Complex32::new(1.0 / (h + 1) as f32, 0.0));
        let mut v = SmearVoice {
            base_spectrum,
            table: [0.0; FRAME_LEN],
            last_smear: -1.0, // force initial resynth
            phases: vec![0.0; voices],
            detune_ratios,
        };
        v.resynth(0.0);
        v
    }

    fn resynth (&mut self, smear: f32) {
        self.table = smear_resynth(&self.base_spectrum, smear);
        self.last_smear = smear;
    }

    fn lookup (&self, phase: f32) -> f32 {
        let pos  = phase.rem_euclid(1.0) * FRAME_LEN as f32;
        let i0   = pos as usize % FRAME_LEN;
        let i1   = (i0 + 1) % FRAME_LEN;
        let frac = pos - pos.floor();
        self.table[i0] + (self.table[i1] - self.table[i0]) * frac
    }

    fn tick_voices (&mut self, freq: f32, sr: f32) -> f32 {
        let n = self.phases.len();
        let mut sum = 0.0f32;
        for v in 0..n {
            sum += self.lookup(self.phases[v]);
            let inc = freq * self.detune_ratios[v] / sr;
            self.phases[v] = (self.phases[v] + inc).rem_euclid(1.0);
        }
        sum / n as f32
    }
}

// Vital's real unison detune formula (synth_oscillator.cpp:609-631),
// simplified: linear spread across voices instead of the exact nonlinear
// detune_power curve (flagged approximation, see plan).
fn osc_b_detune_ratios () -> Vec<f32> {
    (0..OSC_B_VOICES).map(|v| {
        let t = (2 * v as i32 - (OSC_B_VOICES as i32 - 1)) as f32 / (OSC_B_VOICES as i32 - 1) as f32;
        let cents = DETUNE_RANGE * UNISON_DETUNE * t;
        2f32.powf(cents / 1200.0)
    }).collect()
}

pub struct WavetableGen {
    osc_a: SmearVoice,
    osc_b: SmearVoice,
    crossfade_lfo: An<Sine<f64>>,
    wobble: Box<dyn AudioUnit>,      // white() >> lowpass_hz(..) -- smoothed-noise Perlin stand-in
    base_highpass: Box<dyn AudioUnit>, // filter_2 stand-in
    bass_shelf: Box<dyn AudioUnit>,    // p1 BASS DRIVE
    filter_p2: An<Moog<f64, U3>>,      // p2 FILTER
    reverb: Box<dyn AudioUnit>,        // p3 SPACE
    chorus: Box<dyn AudioUnit>,        // p4 WARP
    sample_rate: f64,
    sample_counter: usize,
}

impl WavetableGen {
    pub fn new () -> WavetableGen {
        WavetableGen {
            osc_a: SmearVoice::new(OSC_A_VOICES, vec![1.0]),
            osc_b: SmearVoice::new(OSC_B_VOICES, osc_b_detune_ratios()),
            crossfade_lfo: sine(),
            wobble: Box::new(white() >> lowpass_hz(WOBBLE_HZ, 1.0)),
            base_highpass: Box::new(highpass_hz(BASE_HIGHPASS_HZ, 0.7)),
            bass_shelf: Box::new(lowshelf_hz(BASS_SHELF_HZ, 0.7, BASS_SHELF_GAIN)),
            filter_p2: moog(),
            reverb: Box::new(reverb_stereo(REVERB_ROOM_SIZE, REVERB_TIME, REVERB_DAMPING)),
            chorus: Box::new(chorus(0, 0.015, 0.005, 0.5)),
            sample_rate: 44100.0,
            sample_counter: 0,
        }
    }
}

impl Clone for WavetableGen {
    fn clone (&self) -> WavetableGen { WavetableGen::new() }
}

impl AudioNode for WavetableGen {
    const ID: u64 = 0x7A_15;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];
        let p1    = input[2]; // BASS DRIVE
        let p2    = input[3]; // FILTER
        let p3    = input[4]; // SPACE
        let p4    = input[5]; // WARP

        let sr = self.sample_rate as f32;

        self.sample_counter += 1;
        if self.sample_counter >= CONTROL_RATE_DIV {
            self.sample_counter = 0;

            let wobble = self.wobble.get_mono() * 0.5 + 0.5; // -1..1 -> 0..1
            self.osc_a.resynth((wobble * OSC_A_WOBBLE_DEPTH).clamp(0.0, 1.0));

            let smear_b = (OSC_B_BASE_SMEAR + p4 * OSC_B_WARP_SMEAR_ADD).clamp(0.0, 1.0);
            if (smear_b - self.osc_b.last_smear).abs() > 1e-4 {
                self.osc_b.resynth(smear_b);
            }
        }

        let a = self.osc_a.tick_voices(freq * OSC_A_PITCH_RATIO, sr);
        let b = self.osc_b.tick_voices(freq, sr);

        let cross = self.crossfade_lfo.filter_mono(CROSSFADE_HZ) * 0.5 + 0.5; // 0..1
        let mut mono = a * (OSC_A_LEVEL + cross * OSC_A_LFO_WEIGHT)
                     + b * (OSC_B_LEVEL + cross * OSC_B_LFO_WEIGHT);

        // baked base character: filter_1 (mostly just drive) + filter_2 (highpass-leaning)
        mono = (mono * BASE_DRIVE).tanh();
        mono = self.base_highpass.filter_mono(mono);

        // p1 BASS DRIVE: low shelf boost + extra saturation, wet-mixed
        let shelved = self.bass_shelf.filter_mono(mono);
        let driven  = (mono * BASS_DRIVE_EXTRA).tanh();
        mono = mono + (shelved - mono) * p1 + (driven - mono) * p1;

        // p2 FILTER: moog lowpass swept from near-closed to open
        let cutoff = linexp(0.0, 1.0, FILTER_CUTOFF_LO, FILTER_CUTOFF_HI, p2.clamp(0.0, 1.0));
        mono = self.filter_p2.tick(&Frame::from([mono, cutoff, FILTER_RESONANCE]))[0];

        // p3 SPACE: reverb wet mix
        let mut rev_out = [0.0f32; 2];
        self.reverb.tick(&[mono, mono], &mut rev_out);
        let rev_mono = (rev_out[0] + rev_out[1]) * 0.5;
        mono = mono + (rev_mono - mono) * p3;

        // p4 WARP: chorus wet mix (osc_b Smear boost already applied above)
        let chorused = self.chorus.filter_mono(mono);
        mono = mono + (chorused - mono) * p4;

        mono *= level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.crossfade_lfo.set_sample_rate(sample_rate);
        self.wobble.set_sample_rate(sample_rate);
        self.base_highpass.set_sample_rate(sample_rate);
        self.bass_shelf.set_sample_rate(sample_rate);
        self.filter_p2.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
        self.chorus.set_sample_rate(sample_rate);
    }
}

impl GenNode for WavetableGen {
    fn name (&self) -> &'static str { "Wavetable" }
    fn param_names (&self) -> [&'static str; 4] { ["bass_drive", "filter", "space", "warp"] }
}

//
// GrowlVoice -- wraps WavetableGen above with a live Shared per param, same
// as everything else in this engine, plus a bolted-on NAM amp stage.
//

// Fixed defaults, formerly GrowlParams::default() -- seeded directly into
// the Shared cells below now that there's no separate snapshot/handle shape.
const DEFAULT_BASS_DRIVE:    f32 = 0.8;
const DEFAULT_FILTER:        f32 = 0.9;
const DEFAULT_SPACE:         f32 = 0.25;
const DEFAULT_WARP:          f32 = 0.3;
const DEFAULT_NAM_CROSSOVER: f32 = 0.0;

const TRI_BASE_LEVEL:  f32 = 0.0;
const TRI_FIFTH_LEVEL: f32 = 0.0;
const TRI_FIFTH_RATIO: f32 = 1.4983071250770082; // equal-tempered perfect fifth (+7 semitones)

const TRI_ENV_SUSTAIN:   f32 = 0.3;  // decays to and holds at this fraction
const TRI_ENV_DECAY_SEC: f32 = 0.3;

// Retriggers off the same thump_trigger Shared as ThumpMod (bumped on every
// note-on, see mod.rs), decaying 1.0 -> TRI_ENV_SUSTAIN over TRI_ENV_DECAY_SEC
// then holding -- same -5*t/decay time-constant convention as ThumpMod.
#[derive(Clone)]
struct TriEnv {
    trigger: Shared,
    last_trigger:    f32,
    elapsed_samples: f32,
    sample_rate:     f32,
}

impl TriEnv {
    fn new (trigger: Shared) -> TriEnv {
        TriEnv { trigger, last_trigger: 0.0, elapsed_samples: 0.0, sample_rate: 44100.0 }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }

    fn tick (&mut self) -> f32 {
        let trigger = self.trigger.value();
        if trigger != self.last_trigger {
            self.last_trigger = trigger;
            self.elapsed_samples = 0.0;
        }

        let t = self.elapsed_samples / self.sample_rate;
        self.elapsed_samples += 1.0;

        let decay = (-5.0 * t / TRI_ENV_DECAY_SEC).exp();
        TRI_ENV_SUSTAIN + (1.0 - TRI_ENV_SUSTAIN) * decay
    }
}

// Audio-thread owner: the real WavetableGen plus one Shared cell per
// externally-visible param (GUI reads these read-only; MIDI CC, via
// apply_cc, is the only writer -- see voice.rs's module doc). `_input`
// fields are the authored knob values; `_live` fields are read-only,
// written by GrowlVoice each tick, and show the actual post-modulation
// values the DSP is using -- for visualisation only. Note WavetableGen's
// Clone impl resets to a fresh, un-warmed-up instance (see above).
#[derive(Clone, Voice)]
#[voice(index = 1, id = 0x7A_40, label = "Growl", new = manual)]
pub struct GrowlVoice {
    #[node] inner:   WavetableGen,
    #[node] nam:     NamStage,
    scratch: Vec<f32>,
    pos:     usize,

    #[node] tri_base:  An<WaveSynth<U1>>,
    #[node] tri_fifth: An<WaveSynth<U1>>,
    #[node] tri_env:   TriEnv,

    #[input(cc = "1", range = 0.0..1.0,     set = |v| v)]           pub bass_drive_input:    Shared,
    #[input(cc = "2", range = 0.0..1.0,     set = |v| v)]           pub filter_input:        Shared,
    #[input(cc = "3", range = 0.0..1.0,     set = |v| v)]           pub space_input:         Shared,
    #[input(cc = "4", range = 0.0..1.0,     set = |v| v)]           pub warp_input:          Shared,
    #[input(cc = "5", range = 0.0..10000.0, set = |v| v * 10000.0)] pub nam_crossover_input: Shared,

    #[live(range = 0.0..1.0)] pub filter_live:    Shared,
    #[live(range = 0.0..1.0)] pub warp_live:      Shared,
    #[live(range = 0.0..2.0)] pub freq_mult_live: Shared,

    thump: ThumpMod,
    sig:   SignalState,
}

// GrowlView + view()/fields()/apply()/UI_RANGES + the AudioNode/Voice impls
// are generated by #[derive(Voice)]. new() stays hand-written (new = manual)
// because it loads the NAM model and sizes the scratch buffer before the
// struct literal.
impl GrowlVoice {
    pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> GrowlVoice {
        let model = super::nam::load_named_model(NAM_MODEL).unwrap();
        let nam = NamStage::new(vec![Some(model)], shared(0.0));

        GrowlVoice {
            inner: WavetableGen::new(),
            nam,
            scratch: vec![0.0; NAM_BLOCK_CAP],
            pos: 0,

            tri_base:  triangle(),
            tri_fifth: triangle(),
            tri_env:   TriEnv::new(thump_trigger.clone()),

            bass_drive_input:    shared(DEFAULT_BASS_DRIVE),
            filter_input:        shared(DEFAULT_FILTER),
            space_input:         shared(DEFAULT_SPACE),
            warp_input:          shared(DEFAULT_WARP),
            nam_crossover_input: shared(DEFAULT_NAM_CROSSOVER),

            filter_live:    shared(0.0),
            warp_live:      shared(0.0),
            freq_mult_live: shared(0.0),

            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            sig:   SignalState::new(),
        }
    }
}

impl VoiceDsp for GrowlVoice {
    fn render (&mut self, freq: f32, thump_mult: f32) -> Frame<f32, U2> {
        self.freq_mult_live.set_value(thump_mult);

        self.filter_live.set_value((self.filter_input.value() * self.sig.filter).clamp(0.0, 1.0));
        self.warp_live.set_value((self.warp_input.value() * (1.0 - self.sig.width)).clamp(0.0, 1.0));

        let drive = self.bass_drive_input.value();
        let space = self.space_input.value();

        let mut raw = self.inner.tick(&Frame::from([
            freq, 1.0, drive, self.filter_live.value(), space, self.warp_live.value()
        ]))[0];

        let tri_env = self.tri_env.tick();
        raw += self.tri_base.filter_mono(freq) * TRI_BASE_LEVEL * tri_env;
        raw += self.tri_fifth.filter_mono(freq * TRI_FIFTH_RATIO) * TRI_FIFTH_LEVEL * tri_env;

        let wet = self.scratch.get(self.pos).copied().unwrap_or(0.0);
        if let Some(cell) = self.scratch.get_mut(self.pos) { *cell = raw; }
        self.pos += 1;

        Frame::from([wet, wet])
    }

    fn on_block_start (&mut self, block_len: usize) {
        let n = std::cmp::min(block_len, self.scratch.len());
        let crossover_hz = self.nam_crossover_input.value();
        self.nam.process_block(&mut self.scratch[..n], 1.0, self.sig.fuzz, 1.0, crossover_hz);
        self.pos = 0;
    }

    // Growl keeps its NAM scratch cursor advancing while gated out so a
    // mid-block voice switch stays aligned.
    fn on_silence (&mut self) {
        if let Some(cell) = self.scratch.get_mut(self.pos) { *cell = 0.0; }
        self.pos += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sanity check for the Smear resynthesis and the full macro-gated
    // signal chain -- the only branching/recursive logic in this file.
    #[test]
    fn smear_and_full_chain_produce_finite_bounded_signal () {
        // smear=1.0 is a known edge case, faithful to Vital's own algorithm:
        // the carry seeds from DC*(1-smear), which is exactly 0 there, and
        // 0 stays 0 through pure multiplication -- silence at the literal
        // extreme is correct ported behavior, not a bug.
        for smear in [0.0, 0.25, 0.5, 0.75] {
            let base: [Complex32; NUM_HARMONICS] =
                std::array::from_fn(|h| Complex32::new(1.0 / (h + 1) as f32, 0.0));
            let table = smear_resynth(&base, smear);
            let peak = table.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(peak.is_finite() && peak > 0.0 && peak <= 1.0 + 1e-4, "smear={smear} peak={peak}");
        }
        let table_at_one = smear_resynth(&std::array::from_fn(|h| Complex32::new(1.0 / (h + 1) as f32, 0.0)), 1.0);
        assert!(table_at_one.iter().all(|s| s.is_finite()));

        let mut g = WavetableGen::new();
        g.set_sample_rate(48000.0);

        let mut out = Frame::from([0.0f32; 6]);
        out[0] = 55.0; out[1] = 1.0; out[2] = 0.5; out[3] = 0.5; out[4] = 0.5; out[5] = 0.5;
        for _ in 0..48000 {
            let result = AudioNode::tick(&mut g, &out);
            assert!(result[0].is_finite() && result[1].is_finite());
            assert!(result[0].abs() <= 4.0, "unbounded output: {}", result[0]);
        }
    }
}
