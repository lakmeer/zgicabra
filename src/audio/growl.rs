
//
// growl.vital, baked in. Not a general-purpose wavetable oscillator --
// this GenNode reproduces one specific Vital patch's whole audio graph
// (two Smear-morphed oscillators, their internal LFO/noise modulation,
// and its four macro-gated effects), with only the patch's 4 macros
// exposed as p1-p4. Read from growl.vital (JSON) and cross-checked
// against Vital's own source (github.com/mtytel/vital, GPLv3).
//
// osc_a: 1 voice, octave down, static saw-ish spectrum, its Smear amount
//        wobbled 0..0.14 by smoothed noise (stand-in for Vital's Perlin
//        random_1) -- the main "growl" motion.
// osc_b: 12-voice unison (Vital's real per-voice cents formula), static
//        saw-ish spectrum, Smear amount 0.055 + up to +0.135 from WARP.
// Both cross-faded in level by a slow internal sine LFO (stand-in for
// Vital's tempo-synced lfo_1).
//
// p1 BASS DRIVE -- low shelf boost + extra saturation, wet-mixed
// p2 FILTER     -- moog lowpass cutoff sweep (closed -> open)
// p3 SPACE      -- reverb wet mix
// p4 WARP       -- osc_b Smear boost (real ported mod-matrix effect) + chorus wet mix
//

use std::sync::Arc;

use fundsp::prelude64::*;
use fundsp::fft::inverse_fft;
use num_complex::Complex32;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::gen_node::GenNode;
use super::voice::{Voice, VoiceParams, ThumpMod};
use super::nam::{NamStage, NamModelCycler, NamModelSlot, NAM_BLOCK_CAP, default_model_index};

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
const DETUNE_RANGE: f32 = 2.0;   // osc_2_detune_range
const UNISON_DETUNE: f32 = 2.9;  // osc_2_unison_detune

const NAM_MODEL: &str = "mesa";

const CROSSFADE_HZ: f32 = 0.15;  // stand-in for lfo_1 (tempo-synced ~1Hz in the patch)
const WOBBLE_HZ: f32 = 0.2;      // smoothing rate for the noise-based Perlin stand-in
const CONTROL_RATE_DIV: usize = 64; // Smear resynthesis runs at this coarser rate, not per-sample

// filter_1 (kDirty, cutoff ~10.8kHz -> effectively just its drive) and
// filter_2 (kAnalog, cutoff ~681Hz, blend past bandpass toward highpass)
// baked in as always-on base character -- not macro-gated in the patch.
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

#[derive(Clone, Copy)]
pub struct GrowlParams {
    pub bass_drive:    f32,
    pub filter:        f32,
    pub space:         f32,
    pub warp:          f32,
    pub nam_crossover: f32,
}

impl Default for GrowlParams {
    fn default () -> GrowlParams {
        GrowlParams {
            bass_drive:    0.8,
            filter:        0.9,
            space:         0.25,
            warp:          0.3,
            nam_crossover: 0.0 
        }
    }
}

impl VoiceParams for GrowlParams {
    fn voice_name () -> &'static str { "growl" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("bass_drive",    self.bass_drive),
            ("filter",        self.filter),
            ("space",         self.space),
            ("warp",          self.warp),
            ("nam_crossover", self.nam_crossover),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> GrowlParams {
        let mut params = GrowlParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "bass_drive"    => params.bass_drive    = *value,
                "filter"        => params.filter        = *value,
                "space"         => params.space         = *value,
                "warp"          => params.warp          = *value,
                "nam_crossover" => params.nam_crossover = *value,
                _ => {},
            }
        }
        params
    }
}

// GUI/AudioHandles-facing handle: just the live Shared cells, no DSP state --
// cheap to clone (Arc bump), safe to hand to the GUI thread.
#[derive(Clone)]
pub struct GrowlHandle {
    pub bass_drive:    Shared,
    pub filter:        Shared,
    pub space:         Shared,
    pub warp:          Shared,
    pub nam_crossover: Shared,
}

impl GrowlHandle {
    pub fn new (params: &GrowlParams) -> GrowlHandle {
        GrowlHandle {
            bass_drive:    shared(params.bass_drive),
            filter:        shared(params.filter),
            space:         shared(params.space),
            warp:          shared(params.warp),
            nam_crossover: shared(params.nam_crossover),
        }
    }

    pub fn params (&self) -> GrowlParams {
        GrowlParams {
            bass_drive:    self.bass_drive.value(),
            filter:        self.filter.value(),
            space:         self.space.value(),
            warp:          self.warp.value(),
            nam_crossover: self.nam_crossover.value(),
        }
    }

    pub fn load (&self, params: &GrowlParams) {
        self.bass_drive.set_value(params.bass_drive);
        self.filter.set_value(params.filter);
        self.space.set_value(params.space);
        self.warp.set_value(params.warp);
        self.nam_crossover.set_value(params.nam_crossover);
    }
}

// Audio-thread owner: the real WavetableGen plus the same Shared cells the
// handle above holds (same underlying Arc -- edits sync). Note WavetableGen's
// Clone impl resets to a fresh, un-warmed-up instance (see above).
#[derive(Clone)]
pub struct GrowlVoice {
    inner:  WavetableGen,
    handle: GrowlHandle,
    nam:    NamStage,
    scratch: Vec<f32>,
    pos:     usize,

    thump:   ThumpMod,
    thump_signal: f32,

    filter_signal: f32,
    fuzz_signal: f32,
    width_signal: f32,
}

impl GrowlVoice {
    pub fn new (handle: GrowlHandle, thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> GrowlVoice {
        let model = super::nam::load_named_model(NAM_MODEL).unwrap();
        let nam = NamStage::new(vec![Some(model)], shared(0.0));

        GrowlVoice {
            inner: WavetableGen::new(),
            handle,
            nam,
            scratch: vec![0.0; NAM_BLOCK_CAP],
            pos: 0,
            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay), thump_signal: 0.0,
            filter_signal: 0.0,
            fuzz_signal:   0.0,
            width_signal:  0.0,
        }
    }
}

impl AudioNode for GrowlVoice {
    const ID: u64 = 0x7A_40;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;

        if selected != Self::INDEX {
            if let Some(cell) = self.scratch.get_mut(self.pos) { *cell = 0.0; }
            self.pos += 1;
            return Frame::from([0.0, 0.0]);
        }

        let freq = freq * self.thump.tick(self.thump_signal);
        let filter_cutoff = (self.handle.filter.value() * self.filter_signal).clamp(0.0, 1.0);
        let drive = self.handle.bass_drive.value();
        let space = self.handle.space.value();
        let warp = (self.handle.warp.value() * (1.0 - self.width_signal)).clamp(0.0, 1.0);

        let raw = self.inner.tick(&Frame::from([ freq, 1.0, drive, filter_cutoff, space, warp ]))[0];

        let wet = self.scratch.get(self.pos).copied().unwrap_or(0.0);
        if let Some(cell) = self.scratch.get_mut(self.pos) { *cell = raw; }
        self.pos += 1;

        Frame::from([wet, wet])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.inner.set_sample_rate(sample_rate);
        self.nam.set_sample_rate(sample_rate);
        self.thump.set_sample_rate(sample_rate);
    }
}

impl Voice for GrowlVoice {
    const INDEX: usize = 1;
    fn name (&self) -> &'static str { "Growl" }
    fn set_signal (&mut self, _bend: f32, filter: f32, fuzz: f32, width: f32, thump: f32) {
        self.thump_signal  = thump;
        self.filter_signal = filter;
        self.fuzz_signal   = fuzz;
        self.width_signal  = width;
    }

    // Runs the NAM model over last block's buffered raw output (`scratch`
    // above) before this block's tick() calls start reading it. Fixed
    // level=1/boost=1 -- Bypass (model index 0) already gives dry passthrough.
    fn on_block_start (&mut self, block_len: usize) {
        let n = std::cmp::min(block_len, self.scratch.len());
        let crossover_hz = self.handle.nam_crossover.value();
        self.nam.process_block(&mut self.scratch[..n], 1.0, self.fuzz_signal, 1.0, crossover_hz);
        self.pos = 0;
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
