
//
// gorgle.vital, baked in -- same approach as growl.rs (see that file's
// header): reproduces one specific Vital patch's whole audio graph, cross-
// checked against Vital's own source (./vital_test, GPLv3), with only the
// patch's 4 macros exposed as p1-p4. Not a general-purpose oscillator.
//
// Unlike growl.vital (2 oscillators, all Smear morph, simple filters),
// gorgle.vital uses 3 oscillators with 3 different spectral morph types, a
// phase-distorted/FM-linked pair, and two *comb* filters in series (Vital
// FilterModel::kComb, CombFilter::kBandSpread style) -- a tuned resonant
// delay line, not a subtractive filter, is the main source of the "gorgle"
// character. See vital_test/src/synthesis/filters/comb_filter.cpp:35-50
// for the ported tickComb algorithm.
//
// osc_a: 1 voice, InharmonicScale morph (spectral_morph.h's
//        inharmonicScaleMorph, harmonics nonlinearly stretched off their
//        integer multiples), phase-warped every sample by Vital's kBend
//        distortion curve (synth_oscillator.cpp:72-85), and is the FM
//        *source* for osc_c.
// osc_b: 16-voice unison, LowPass morph (a harmonic-domain brickwall that
//        slides open), the main "chorussy" body.
// osc_c: 1 voice, no spectral morph, phase-modulated by osc_a's raw sample
//        (Vital's kFmOscillatorA) -- silent at rest, only fades in as
//        GIRGLE rises.
//
// p1 WOBBLE   -- filter_fx (analog lowpass, cutoff wobbled by an internal
//                LFO standing in for lfo_7) wet mix
// p2 AMBIENCE -- crossfades osc_a down / osc_b up, chorus wet mix
// p3 GIRGLE   -- crossfades osc_b down / osc_c in, sweeps osc_a's
//                InharmonicScale amount (the real ported macro3->mod19
//                "modulation of a modulation" route, collapsed to one
//                direct sweep)
// p4 GRIND    -- echo (hand-rolled feedback delay) wet mix, extra reverb
//                wet, and a bell-EQ cut on the band that gorgle.vital's
//                own eq_band macro-3 route targets
//

use fundsp::prelude64::*;
use fundsp::fft::inverse_fft;
use num_complex::Complex32;

use crate::zgicabra::SignalState;
use super::gen_node::GenNode;
use super::voice::{Voice, VoiceParams, ThumpMod};

const FRAME_LEN: usize = 256;
const NUM_HARMONICS: usize = 32;
const CONTROL_RATE_DIV: usize = 64; // spectral resynth runs at this coarser rate

const OSC_A_VOICES: usize = 1;
const OSC_A_LEVEL: f32 = 0.90;               // osc_1_level
const OSC_A_AMBIENCE_DROP: f32 = 0.9;        // macro_control_2 -> osc_1_level (-0.96)
const OSC_A_INHARMONIC_BASE: f32 = 0.5;      // osc_1_spectral_morph_amount (0.5 = neutral)
const OSC_A_GIRGLE_MORPH_SWEEP: f32 = 0.45;  // macro3 -> modulation_19_amount -> osc_1 morph
const OSC_A_BEND_BASE: f32 = 0.5;            // osc_1_distortion_amount
const OSC_A_BEND_LFO_DEPTH: f32 = 0.45;      // lfo_5 -> osc_1_distortion_amount

const OSC_B_VOICES: usize = 16;              // osc_2_unison_voices
const OSC_B_LEVEL: f32 = 0.61;               // osc_2_level
const OSC_B_AMBIENCE_ADD: f32 = 0.39;        // macro_control_2 -> osc_2_level
const OSC_B_GIRGLE_DROP: f32 = 0.665;        // macro_control_3 -> osc_2_level
const OSC_B_LOWPASS_BASE: f32 = 0.364;       // osc_2_spectral_morph_amount
const OSC_B_LOWPASS_WOBBLE: f32 = 0.15;      // lfo_3 -> osc_2_wave_frame, stand-in
const DETUNE_RANGE: f32 = 2.0;               // osc_2_detune_range
const UNISON_DETUNE: f32 = 4.4721;           // osc_2_unison_detune

const OSC_C_LEVEL_MAX: f32 = 0.24;           // macro_control_3 -> osc_3_level
const OSC_C_FM_DEPTH: f32 = 3.0;             // osc_3_distortion_amount (0.22), scaled for audible PM

// filter_1/filter_2: both Vital FilterModel::kComb, CombFilter::kBandSpread
// style, keytracked (cutoff = played freq * ratio, ratio from cutoff-in-
// note-units minus reference note 60, see comb_filter.h/cpp).
const COMB_MAX_PERIOD: usize = 4096; // covers down to ~11Hz at 48kHz
const FILTER1_CUTOFF_RATIO: f32 = 1.0;   // filter_1_cutoff = note 60 (unison root)
const FILTER1_RESONANCE: f32 = 0.78;     // filter_1_resonance
const FILTER1_BLEND_BASE: f32 = 0.70;    // filter_1_blend
const FILTER1_BLEND_LFO_DEPTH: f32 = 0.3; // lfo_1 -> filter_1_blend_transpose
const FILTER1_MIX: f32 = 0.60;           // filter_1_mix

const FILTER2_CUTOFF_RATIO: f32 = 2.0;   // filter_2_cutoff = note 72 (octave up)
const FILTER2_RESONANCE: f32 = 0.0;      // filter_2_resonance
const FILTER2_BLEND_BASE: f32 = 0.62;    // filter_2_blend
const FILTER2_BLEND_LFO_DEPTH: f32 = 0.3; // lfo_2 -> filter_2_blend_transpose
const FILTER2_MIX: f32 = 0.47;           // filter_2_mix
const COMB_ONE_POLE_COEFF: f32 = 0.35;   // fixed tone-shaping coefficient (exact
                                          // cutoff-derived formula not ported)

const FILTER_FX_BASE_HZ: f32 = 3495.0;   // filter_fx_cutoff = note 104.86, not keytracked
const FILTER_FX_WOBBLE_DEPTH: f32 = 0.6; // lfo_7 -> filter_fx_cutoff (amount -0.62)
const LFO7_HZ: f32 = 0.6;                // stand-in for lfo_7 (tempo=10)

const LFO1_HZ: f32 = 0.3; // stand-in for lfo_1 (tempo=5)
const LFO2_HZ: f32 = 0.5; // stand-in for lfo_2 (tempo=7)
const LFO3_HZ: f32 = 0.3; // stand-in for lfo_3 (tempo=5)
const LFO5_HZ: f32 = 0.2; // stand-in for lfo_5 (tempo=3)

const ECHO_MAX_SAMPLES: usize = 96_000; // ~2s at 48kHz
const ECHO_BASE_HZ: f32 = 7.39;         // delay_frequency -> echo repeat rate
const ECHO_FEEDBACK_BASE: f32 = 0.15;   // delay_feedback
const ECHO_FEEDBACK_GRIND: f32 = 0.36;  // macro_control_4 -> delay_feedback

const EQ_BELL_HZ: f32 = 700.0; // eq_band_cutoff (~70.8 note) -> approx Hz
const EQ_BELL_Q: f32 = 0.8;
const EQ_BELL_CUT_DB: f32 = -8.0; // macro_control_4 -> eq_band_gain (approximated, see growl.rs precedent)

// Vital's inharmonicScaleMorph range (spectral_morph.h's kMaxInharmonicScale)
const INHARMONIC_MAX_MULT: f32 = 4.0;

fn finish_resynth (mut spectrum: [Complex32; FRAME_LEN]) -> [f32; FRAME_LEN] {
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

// Vital's kLowPass spectral morph (spectral_morph.h's lowPassMorph): the
// cutoff harmonic index grows exponentially with `amount`, with a one-
// harmonic linear fade at the boundary instead of a hard brickwall.
fn lowpass_resynth (base: &[Complex32; NUM_HARMONICS], amount: f32) -> [f32; FRAME_LEN] {
    let cutoff = 2f32.powf(amount * (NUM_HARMONICS as f32).log2()).clamp(1.0, NUM_HARMONICS as f32);
    let cutoff_idx = cutoff.floor() as usize;
    let frac = cutoff - cutoff.floor();
    let mut spectrum = [Complex32::ZERO; FRAME_LEN];
    for h in 0..NUM_HARMONICS {
        let mult = if h + 1 < cutoff_idx { 1.0 } else if h + 1 == cutoff_idx { frac } else { 0.0 };
        spectrum[h + 1] = base[h] * mult;
    }
    finish_resynth(spectrum)
}

// Vital's kInharmonicScale spectral morph (spectral_morph.h's
// inharmonicScaleMorph), simplified to real-valued scatter: each harmonic's
// frequency is nonlinearly stretched by `mult` (bigger stretch for higher
// harmonics per an octave-scaled power curve), landing on a non-integer
// multiple of the fundamental -- amount=0.5 is neutral (mult=1).
fn inharmonic_resynth (base: &[Complex32; NUM_HARMONICS], amount: f32) -> [f32; FRAME_LEN] {
    let mult = INHARMONIC_MAX_MULT.powf(2.0 * amount - 1.0);
    let mut spectrum = [Complex32::ZERO; FRAME_LEN];
    spectrum[1] = base[0];
    for h in 1..NUM_HARMONICS {
        let index = (h + 1) as f32;
        let power = index.log2() / (NUM_HARMONICS as f32).log2();
        let shift = mult.powf(power);
        let shifted = (shift * (index - 1.0) + 1.0).max(1.0);
        let dest = (shifted.round() as usize).clamp(1, NUM_HARMONICS);
        spectrum[dest] += base[h];
    }
    finish_resynth(spectrum)
}

fn passthrough_resynth (base: &[Complex32; NUM_HARMONICS]) -> [f32; FRAME_LEN] {
    let mut spectrum = [Complex32::ZERO; FRAME_LEN];
    for h in 0..NUM_HARMONICS { spectrum[h + 1] = base[h]; }
    finish_resynth(spectrum)
}

// Vital's real unison detune formula (synth_oscillator.cpp:609-631),
// simplified: linear spread across voices instead of the exact nonlinear
// detune_power curve (same flagged approximation growl.rs makes).
fn unison_detune_ratios (voices: usize, detune_range: f32, unison_detune: f32) -> Vec<f32> {
    if voices <= 1 { return vec![1.0]; }
    (0..voices).map(|v| {
        let t = (2 * v as i32 - (voices as i32 - 1)) as f32 / (voices as i32 - 1) as f32;
        let cents = detune_range * unison_detune * t;
        2f32.powf(cents / 1200.0)
    }).collect()
}

// Vital's kBend oscillator distortion (synth_oscillator.cpp:72-85, ported
// to a plain 0..1 phase domain instead of fixed-point sample indices): a
// cubic warp of the phase, symmetric around the midpoint.
fn bend_phase (phase: f32, distortion: f32) -> f32 {
    let fp = phase.rem_euclid(1.0) - 0.5;
    let fp2 = fp * fp;
    let fp3 = fp * fp2;
    let offset = (distortion - distortion * distortion) * 2.0;
    let scale = distortion * 3.0;
    let mid1 = (scale + offset) * (fp2 - fp3);
    let mid2 = (scale - offset) * (fp - fp2 * 2.0 + fp3);
    (fp3 + mid1 + mid2 + 0.5).rem_euclid(1.0)
}

#[derive(Clone, Copy, PartialEq)]
enum Morph { LowPass, Inharmonic, None }

// One spectrally-morphed unison stack -- same fixed-base-spectrum, coarser-
// control-rate-resynth idiom as growl.rs's SmearVoice, generalized over
// which of Vital's spectral morph algorithms drives it.
#[derive(Clone)]
struct MorphVoice {
    base_spectrum: [Complex32; NUM_HARMONICS],
    table: [f32; FRAME_LEN],
    last_amount: f32,
    morph: Morph,
    phases: Vec<f32>,
    detune_ratios: Vec<f32>,
}

impl MorphVoice {
    fn new (morph: Morph, detune_ratios: Vec<f32>) -> MorphVoice {
        let base_spectrum: [Complex32; NUM_HARMONICS] =
            std::array::from_fn(|h| Complex32::new(1.0 / (h + 1) as f32, 0.0));
        let voices = detune_ratios.len();
        let mut v = MorphVoice {
            base_spectrum, table: [0.0; FRAME_LEN], last_amount: -1.0, morph,
            phases: vec![0.0; voices], detune_ratios,
        };
        v.resynth(0.5);
        v
    }

    fn resynth (&mut self, amount: f32) {
        self.table = match self.morph {
            Morph::LowPass    => lowpass_resynth(&self.base_spectrum, amount),
            Morph::Inharmonic => inharmonic_resynth(&self.base_spectrum, amount),
            Morph::None       => passthrough_resynth(&self.base_spectrum),
        };
        self.last_amount = amount;
    }

    fn lookup (&self, phase: f32) -> f32 {
        let pos  = phase.rem_euclid(1.0) * FRAME_LEN as f32;
        let i0   = pos as usize % FRAME_LEN;
        let i1   = (i0 + 1) % FRAME_LEN;
        let frac = pos - pos.floor();
        self.table[i0] + (self.table[i1] - self.table[i0]) * frac
    }

    // Plain unison sum (osc_b).
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

    // Single voice, phase warped by a per-sample function of itself before
    // lookup (osc_a's kBend distortion).
    fn tick_warped (&mut self, freq: f32, sr: f32, warp: impl Fn(f32) -> f32) -> f32 {
        let sample = self.lookup(warp(self.phases[0]));
        let inc = freq * self.detune_ratios[0] / sr;
        self.phases[0] = (self.phases[0] + inc).rem_euclid(1.0);
        sample
    }

    // Single voice, phase offset by an external modulator signal before
    // lookup (osc_c's kFmOscillatorA -- true phase modulation).
    fn tick_fm (&mut self, freq: f32, sr: f32, fm_offset: f32) -> f32 {
        let sample = self.lookup(self.phases[0] + fm_offset);
        let inc = freq * self.detune_ratios[0] / sr;
        self.phases[0] = (self.phases[0] + inc).rem_euclid(1.0);
        sample
    }
}

// Vital's CombFilter, kComb feedback style + kBandSpread filter style
// (vital_test/src/synthesis/filters/comb_filter.cpp:35-50), ported
// directly: a self-limiting (tanh-clamped) feedback delay line with a
// one-pole low/high split inside the loop, mixed by `blend`.
#[derive(Clone)]
struct CombFilter {
    buffer: Vec<f32>,
    write_pos: usize,
    lp1: f32,
    lp2: f32,
}

impl CombFilter {
    fn new (max_period: usize) -> CombFilter {
        CombFilter { buffer: vec![0.0; max_period], write_pos: 0, lp1: 0.0, lp2: 0.0 }
    }

    fn tick (&mut self, input: f32, period: f32, feedback: f32, blend: f32) -> f32 {
        let len = self.buffer.len();
        let period = period.clamp(2.0, (len - 1) as f32);
        let read_pos = (self.write_pos as f32 - period).rem_euclid(len as f32);
        let i0 = read_pos as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = read_pos - read_pos.floor();
        let delayed = self.buffer[i0] + (self.buffer[i1] - self.buffer[i0]) * frac;

        let combine = input * 0.5 + delayed * feedback;
        self.lp1 += COMB_ONE_POLE_COEFF * (combine - self.lp1);
        let low = self.lp1;
        let high = combine - low;
        let low_gain  = (-blend + 2.0).clamp(0.0, 1.0);
        let high_gain = blend.clamp(0.0, 1.0);
        let stage1 = low * low_gain + high * high_gain;
        self.lp2 += COMB_ONE_POLE_COEFF * (stage1 - self.lp2);
        let result = stage1 - self.lp2;

        self.buffer[self.write_pos] = result.tanh();
        self.write_pos = (self.write_pos + 1) % len;
        result
    }
}

// Hand-rolled feedback delay line for GRIND's echo -- fundsp's `delay()`
// has no built-in feedback tap, and this is a handful of lines either way.
#[derive(Clone)]
struct Echo {
    buffer: Vec<f32>,
    write_pos: usize,
}

impl Echo {
    fn new (max_samples: usize) -> Echo { Echo { buffer: vec![0.0; max_samples], write_pos: 0 } }

    fn tick (&mut self, input: f32, delay_samples: usize, feedback: f32) -> f32 {
        let len = self.buffer.len();
        let delay_samples = delay_samples.clamp(1, len - 1);
        let read_pos = (self.write_pos + len - delay_samples) % len;
        let delayed = self.buffer[read_pos];
        self.buffer[self.write_pos] = input + delayed * feedback;
        self.write_pos = (self.write_pos + 1) % len;
        delayed
    }
}

pub struct GorgleGen {
    osc_a: MorphVoice,
    osc_b: MorphVoice,
    osc_c: MorphVoice,

    lfo1: An<Sine<f64>>, // filter_1_blend wobble
    lfo2: An<Sine<f64>>, // filter_2_blend wobble
    lfo3: An<Sine<f64>>, // osc_b lowpass-amount wobble (osc_2_wave_frame stand-in)
    lfo5: An<Sine<f64>>, // osc_a bend-amount wobble
    lfo7: An<Sine<f64>>, // filter_fx cutoff wobble

    comb1: CombFilter,
    comb2: CombFilter,
    filter_fx: An<Moog<f64, U3>>, // p1 WOBBLE
    chorus: Box<dyn AudioUnit>,   // p2 AMBIENCE
    echo: Echo,                   // p4 GRIND
    eq_bell: An<Svf<f64, BellMode<f64>>>, // p4 GRIND
    reverb: Box<dyn AudioUnit>,   // p2 + p4

    sample_rate: f64,
    sample_counter: usize,
}

impl GorgleGen {
    pub fn new () -> GorgleGen {
        GorgleGen {
            osc_a: MorphVoice::new(Morph::Inharmonic, vec![1.0; OSC_A_VOICES]),
            osc_b: MorphVoice::new(Morph::LowPass, unison_detune_ratios(OSC_B_VOICES, DETUNE_RANGE, UNISON_DETUNE)),
            osc_c: MorphVoice::new(Morph::None, vec![1.0]),

            lfo1: sine(), lfo2: sine(), lfo3: sine(), lfo5: sine(), lfo7: sine(),

            comb1: CombFilter::new(COMB_MAX_PERIOD),
            comb2: CombFilter::new(COMB_MAX_PERIOD),
            filter_fx: moog(),
            chorus: Box::new(chorus(1, 0.015, 0.005, 0.5)),
            echo: Echo::new(ECHO_MAX_SAMPLES),
            eq_bell: bell(),
            reverb: Box::new(reverb_stereo(15.0, 2.0, 0.5)),

            sample_rate: 44100.0,
            sample_counter: 0,
        }
    }
}

impl Clone for GorgleGen {
    fn clone (&self) -> GorgleGen { GorgleGen::new() }
}

impl AudioNode for GorgleGen {
    const ID: u64 = 0x7A_50;
    type Inputs = U6;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U6>) -> Frame<f32, U2> {
        let freq  = input[0];
        let level = input[1];
        let p1    = input[2]; // WOBBLE
        let p2    = input[3]; // AMBIENCE
        let p3    = input[4]; // GIRGLE
        let p4    = input[5]; // GRIND

        let sr = self.sample_rate as f32;

        // Every internal LFO ticked exactly once per sample (fundsp's Sine
        // needs a freq input each call, so it can't be read with get_mono())
        // -- control-rate consumers below just reuse the latest value.
        let lfo1_uni = self.lfo1.filter_mono(LFO1_HZ) * 0.5 + 0.5;
        let lfo2_uni = self.lfo2.filter_mono(LFO2_HZ) * 0.5 + 0.5;
        let lfo3_uni = self.lfo3.filter_mono(LFO3_HZ) * 0.5 + 0.5;
        let lfo5_uni = self.lfo5.filter_mono(LFO5_HZ) * 0.5 + 0.5;
        let lfo7_uni = self.lfo7.filter_mono(LFO7_HZ) * 0.5 + 0.5;

        self.sample_counter += 1;
        if self.sample_counter >= CONTROL_RATE_DIV {
            self.sample_counter = 0;

            let morph_a = (OSC_A_INHARMONIC_BASE + p3 * OSC_A_GIRGLE_MORPH_SWEEP * (lfo5_uni * 2.0 - 1.0)).clamp(0.0, 1.0);
            if (morph_a - self.osc_a.last_amount).abs() > 1e-4 { self.osc_a.resynth(morph_a); }

            let morph_b = (OSC_B_LOWPASS_BASE + lfo3_uni * OSC_B_LOWPASS_WOBBLE).clamp(0.0, 1.0);
            if (morph_b - self.osc_b.last_amount).abs() > 1e-4 { self.osc_b.resynth(morph_b); }
        }

        // osc_a: InharmonicScale morph + kBend phase warp, base for osc_c's FM
        let bend_amount = (OSC_A_BEND_BASE + lfo5_uni * OSC_A_BEND_LFO_DEPTH).clamp(0.0, 1.0);
        let a = self.osc_a.tick_warped(freq, sr, |p| bend_phase(p, bend_amount));

        // osc_b: LowPass morph, 16-voice unison
        let b = self.osc_b.tick_voices(freq, sr);

        // osc_c: FM'd by osc_a's raw sample (Vital's kFmOscillatorA)
        let c = self.osc_c.tick_fm(freq, sr, a * OSC_C_FM_DEPTH * 0.1);

        let a_lvl = (OSC_A_LEVEL - p2 * OSC_A_AMBIENCE_DROP).max(0.0);
        let b_lvl = (OSC_B_LEVEL + p2 * OSC_B_AMBIENCE_ADD - p3 * OSC_B_GIRGLE_DROP).max(0.0);
        let c_lvl = p3 * OSC_C_LEVEL_MAX;

        // filter_1 <- osc_a + osc_b (only oscillators routed to kFilter1);
        // filter_2 <- filter_1's output (filter_2_filter_input=1, serial)
        let filters_in = a * a_lvl + b * b_lvl;

        let blend1 = (FILTER1_BLEND_BASE + lfo1_uni * FILTER1_BLEND_LFO_DEPTH).clamp(0.0, 1.0);
        let filtered1 = self.comb1.tick(filters_in, sr / (freq * FILTER1_CUTOFF_RATIO).max(1.0), FILTER1_RESONANCE, blend1);
        let stage1 = filters_in + (filtered1 - filters_in) * FILTER1_MIX;

        let blend2 = (FILTER2_BLEND_BASE + lfo2_uni * FILTER2_BLEND_LFO_DEPTH).clamp(0.0, 1.0);
        let filtered2 = self.comb2.tick(stage1, sr / (freq * FILTER2_CUTOFF_RATIO).max(1.0), FILTER2_RESONANCE, blend2);
        let stage2 = stage1 + (filtered2 - stage1) * FILTER2_MIX;

        // osc_c bypasses both filters (destination=kEffects)
        let mut mono = stage2 + c * c_lvl;

        // p1 WOBBLE: filter_fx, cutoff wobbled by lfo_7, wet mix
        let fx_cutoff = (FILTER_FX_BASE_HZ * (1.0 - lfo7_uni * FILTER_FX_WOBBLE_DEPTH)).clamp(30.0, 18_000.0);
        let filtered_fx = self.filter_fx.tick(&Frame::from([mono, fx_cutoff, 0.3]))[0];
        mono = mono + (filtered_fx - mono) * p1;

        // p2 AMBIENCE: chorus wet mix (osc crossfade already applied above)
        let chorused = self.chorus.filter_mono(mono);
        mono = mono + (chorused - mono) * p2;

        // p4 GRIND: echo wet mix
        let delay_samples = (sr / ECHO_BASE_HZ) as usize;
        let feedback = (ECHO_FEEDBACK_BASE + p4 * ECHO_FEEDBACK_GRIND).clamp(0.0, 0.92);
        let echoed = self.echo.tick(mono, delay_samples, feedback);
        mono = mono + (echoed - mono) * p4;

        // p4 GRIND: eq band cut (bell's gain input is linear amplitude, not dB)
        let bell_gain = 10f32.powf(EQ_BELL_CUT_DB * p4 / 20.0);
        let belled = self.eq_bell.tick(&Frame::from([mono, EQ_BELL_HZ, EQ_BELL_Q, bell_gain]))[0];
        mono = mono + (belled - mono) * p4;

        // p2 + p4: reverb wet mix (both AMBIENCE and GRIND feed it)
        let mut rev_out = [0.0f32; 2];
        self.reverb.tick(&[mono, mono], &mut rev_out);
        let rev_mono = (rev_out[0] + rev_out[1]) * 0.5;
        let reverb_wet = (p2 + p4 * 0.41).clamp(0.0, 1.0);
        mono = mono + (rev_mono - mono) * reverb_wet;

        mono *= level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.lfo1.set_sample_rate(sample_rate);
        self.lfo2.set_sample_rate(sample_rate);
        self.lfo3.set_sample_rate(sample_rate);
        self.lfo5.set_sample_rate(sample_rate);
        self.lfo7.set_sample_rate(sample_rate);
        self.filter_fx.set_sample_rate(sample_rate);
        self.chorus.set_sample_rate(sample_rate);
        self.eq_bell.set_sample_rate(sample_rate);
        self.reverb.set_sample_rate(sample_rate);
    }
}

impl GenNode for GorgleGen {
    fn name (&self) -> &'static str { "Gorgle" }
    fn param_names (&self) -> [&'static str; 4] { ["wobble", "ambience", "girgle", "grind"] }
}

//
// GorgleVoice -- wraps GorgleGen above with a live Shared per param, same as
// GrowlVoice in growl.rs.
//

#[derive(Clone, Copy)]
pub struct GorgleParams {
    pub wobble:   f32,
    pub ambience: f32,
    pub girgle:   f32,
    pub grind:    f32,
}

impl Default for GorgleParams {
    fn default () -> GorgleParams {
        GorgleParams { wobble: 0.3, ambience: 0.4, girgle: 0.3, grind: 0.25 }
    }
}

impl VoiceParams for GorgleParams {
    fn voice_name () -> &'static str { "gorgle" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("wobble",   self.wobble),
            ("ambience", self.ambience),
            ("girgle",   self.girgle),
            ("grind",    self.grind),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> GorgleParams {
        let mut params = GorgleParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "wobble"   => params.wobble   = *value,
                "ambience" => params.ambience = *value,
                "girgle"   => params.girgle   = *value,
                "grind"    => params.grind    = *value,
                _ => {},
            }
        }
        params
    }
}

#[derive(Clone)]
pub struct GorgleHandle {
    pub wobble:   Shared,
    pub ambience: Shared,
    pub girgle:   Shared,
    pub grind:    Shared,
}

impl GorgleHandle {
    pub fn new (params: &GorgleParams) -> GorgleHandle {
        GorgleHandle {
            wobble:   shared(params.wobble),
            ambience: shared(params.ambience),
            girgle:   shared(params.girgle),
            grind:    shared(params.grind),
        }
    }

    pub fn params (&self) -> GorgleParams {
        GorgleParams {
            wobble:   self.wobble.value(),
            ambience: self.ambience.value(),
            girgle:   self.girgle.value(),
            grind:    self.grind.value(),
        }
    }

    pub fn load (&self, params: &GorgleParams) {
        self.wobble.set_value(params.wobble);
        self.ambience.set_value(params.ambience);
        self.girgle.set_value(params.girgle);
        self.grind.set_value(params.grind);
    }
}

#[derive(Clone)]
pub struct GorgleVoice {
    inner:  GorgleGen,
    handle: GorgleHandle,

    thump:        ThumpMod,
    thump_signal: f32,
}

impl GorgleVoice {
    pub fn new (handle: GorgleHandle, thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> GorgleVoice {
        GorgleVoice {
            inner: GorgleGen::new(), handle,
            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay), thump_signal: 0.0,
        }
    }
}

impl AudioNode for GorgleVoice {
    const ID: u64 = 0x7A_51;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        let freq = freq * self.thump.tick(self.thump_signal);

        self.inner.tick(&Frame::from([
            freq, 1.0,
            self.handle.wobble.value(), self.handle.ambience.value(),
            self.handle.girgle.value(), self.handle.grind.value(),
        ]))
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.inner.set_sample_rate(sample_rate);
        self.thump.set_sample_rate(sample_rate);
    }
}

impl Voice for GorgleVoice {
    const INDEX: usize = 1;
    fn name (&self) -> &'static str { "Gorgle" }
    fn set_signal (&mut self, _bend: f32, _filter: f32, _fuzz: f32, _width: f32, thump: f32) {
        self.thump_signal = thump;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sanity check for the two spectral morph algorithms and the full
    // macro-gated signal chain -- the only branching/recursive logic here.
    #[test]
    fn morphs_and_full_chain_produce_finite_bounded_signal () {
        let base: [Complex32; NUM_HARMONICS] = std::array::from_fn(|h| Complex32::new(1.0 / (h + 1) as f32, 0.0));
        for amount in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let lp = lowpass_resynth(&base, amount);
            assert!(lp.iter().all(|s| s.is_finite()), "lowpass amount={amount}");
            let ih = inharmonic_resynth(&base, amount);
            assert!(ih.iter().all(|s| s.is_finite()), "inharmonic amount={amount}");
        }

        let mut g = GorgleGen::new();
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
