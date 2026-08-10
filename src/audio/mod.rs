
// fundsp AudioNodes (https://github.com/SamiPerttu/fundsp)
//
// Generators
//   brown                 - Brownian noise source
//   dsf_saw / dsf_saw_r   - Discrete summation formula sawtooth oscillator
//   dsf_square / _r       - Discrete summation formula square oscillator
//   hammond / hammond_hz  - Hammond organ-like oscillator
//   impulse               - Multichannel impulse signal
//   mls / mls_bits        - Maximum length sequence noise
//   noise / white         - White noise source
//   organ / organ_hz      - Organ wave oscillator
//   pink                  - Pink noise source
//   poly_pulse(_hz)       - PolyBLEP pulse wave
//   poly_saw(_hz)         - PolyBLEP sawtooth wave
//   poly_square(_hz)      - PolyBLEP square wave
//   pulse                 - Bandlimited pulse wave
//   ramp / ramp_hz        - Non-bandlimited sawtooth ramp
//   saw / saw_hz          - Bandlimited sawtooth wave
//   sine / sine_hz        - Sine oscillator
//   soft_saw(_hz)         - Soft sawtooth oscillator
//   square / square_hz    - Bandlimited square wave
//   triangle / triangle_hz- Bandlimited triangle wave
//   zero / multizero      - Silence signal
//   constant / dc         - Constant signal value
//
// Linear Filters
//   allpass(_hz/_q)       - 2nd order allpass filter
//   allpole / allpole_delay - 1st order allpass filter
//   bandpass(_hz/_q)      - 2nd order bandpass filter
//   bell(_hz/_q)          - Peaking/bell equalizer filter
//   biquad                - Arbitrary biquad filter with coefficients
//   butterpass(_hz)       - Butterworth lowpass filter
//   dcblock(_hz)          - DC blocking filter
//   highpass(_hz/_q)      - 2nd order highpass filter
//   highpole(_hz)         - 1st order highpass filter
//   highshelf(_hz/_q)     - High shelf equalizer
//   lowpass(_hz/_q)       - 2nd order lowpass filter
//   lowpole(_hz)          - 1st order lowpass filter
//   lowshelf(_hz/_q)      - Low shelf equalizer
//   morph(_hz)            - Morphing filter (lowpass/peak/highpass)
//   notch(_hz/_q)         - Notch filter
//   peak(_hz/_q)          - Peaking filter
//   pinkpass              - Pink noise shaping filter
//   resonator(_hz)        - Constant-gain bandpass resonator
//   allnest(_c)           - Nested allpass filter
//   fir                   - FIR filter with specified weights
//   fir3                  - Symmetric 3-point FIR filter
//
// Nonlinear Filters
//   bandrez(_hz/_q)       - Resonant bandpass filter
//   dbell(_hz)            - Dirty biquad bell equalizer
//   dhighpass(_hz)        - Dirty biquad highpass
//   dlowpass(_hz)         - Dirty biquad lowpass
//   dresonator(_hz)       - Dirty biquad resonator
//   fbell(_hz)            - Feedback biquad bell equalizer
//   fhighpass(_hz)        - Feedback biquad highpass
//   flowpass(_hz)         - Feedback biquad lowpass
//   fresonator(_hz)       - Feedback biquad resonator
//   lowrez(_hz/_q)        - Resonant lowpass filter
//   moog(_hz/_q)          - Moog ladder lowpass filter
//
// Delay & Time Effects
//   delay                 - Delay by specified time
//   tap / tap_linear      - Tapped delay with interpolation
//   multitap(_linear)     - Multi-tap delay line
//   tick / multitick      - Single sample delay
//   flanger               - Flanging effect
//   phaser                - Phaser effect
//
// Dynamics & Envelopes
//   adsr_live             - ADSR envelope with live control
//   afollow               - Asymmetric smoothing filter
//   follow                - Smoothing filter with response time
//   limiter / limiter_stereo - Look-ahead limiter
//   declick(_s)            - Fade-in declick
//
// Reverb & Spatial
//   reverb_stereo         - FDN stereo reverb
//   reverb2_stereo        - Hybrid FDN stereo reverb
//   reverb3_stereo        - Allpass loop stereo reverb
//   pan                   - Fixed pan to stereo
//   panner                - Dynamic mono-to-stereo panner
//   rotate                - Stereo rotation with gain
//
// Special Processing
//   resynth               - Frequency domain resynthesis
//   convolve               - Convolution filter
//   pluck                  - Karplus-Strong plucked string
//   shape / shape_fn        - Waveshaper distortion
//   clip / clip_to          - Signal clipping
//   meter                   - Signal metering
//   monitor                 - Monitoring pass-through
//   hold(_hz)               - Sample-and-hold
//
// Oscillator Modulation
//   lorenz                - Lorenz system oscillator
//   rossler               - Rössler system oscillator
//   envelope / lfo         - Time-varying control
//   envelope2 / lfo2        - Input-dependent control
//   envelope3 / lfo3        - 2-input dependent control
//   envelope_in / lfo_in    - Frame-based control
//
// Signal Routing & Combination
//   pass / multipass       - Pass-through signal
//   sink / multisink        - Consume signal
//   split / multisplit      - Split to multichannel
//   join / multijoin        - Join multichannel
//   reverse                 - Reverse channel order
//   add / sub / mul          - Arithmetic operations
//   product / sum            - Multiply/sum two nodes
//   pipe(i/f)                - Serial chaining
//   branch(i/f)               - Parallel branching
//   bus(i/f)                  - Signal busing
//   stack(i/f)                - Parallel stacking
//   thru                      - Pass-through with parameter adjustment
//
// Wave & Sample Playback
//   playwave(_at)             - Play back wave data
//   resample                  - Resample generator at variable speed
//   resample_fir               - FIR-based sinc resampling
//
// Control & Feedback
//   feedback / feedback2       - Single-sample feedback loop
//   fdn / fdn2                 - Feedback Delay Network
//   listen                     - Setting listener wrapper
//   update                     - Update node with interval
//   var / var_fn                - Shared variable output
//   timer                       - Stream time tracking
//   oversample                  - 2x oversampling
//   biquad_bank                 - SIMD-accelerated biquad bank
//   chorus                      - Chorus effect
//   map                         - Custom channel mapping
//   unit                        - Convert AudioUnit to AudioNode
//
//
// Audio Engine
//

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::output::DeltaConsumer;
use crate::tools::linexp;
use crate::zgicabra::{DeltaEvent, SignalState};

mod nam;
mod stutter;
mod gen_node;
mod fx_node;
mod reese;
mod fm;
mod filter;
mod reverb;
mod crusher;
pub mod snapshot;

use nam::{NamStage, NAM_BLOCK_CAP};
use gen_node::GenSlot;
use fx_node::FxSlot;
use filter::LowpassFx;
use reverb::ReverbFx;
use crusher::Crusher;
pub use nam::NamModelCycler;
pub use gen_node::GenCycler;
pub use fx_node::FxCycler;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const NAM_SAMPLE_RATE: u32 = 48_000;

// Fixed envelope times -- there's no matrix row for these (the row list has
// none), so unlike everything else here they're not GUI/matrix editable.
// Baked into adsr_live at construction time -- editing means a process
// restart, same as reverb_size/decay/damp below.
const ENVELOPE_ATTACK:  f32 = 0.003;
const ENVELOPE_RELEASE: f32 = 0.1;

const CAPTURE_SECONDS: f32 = 0.1;

// Test seam: lets an external caller (see main.rs's --test self-test) tap a
// snapshot of the raw cpal output stream to check it's actually producing
// signal, diagnosing "no audio output" independent of the OS/device layer.
// Captures mono (left channel) samples starting from the next audio
// callback after `start()`, stops once `cap` samples are collected.
#[derive(Clone)]
pub struct AudioCapture {
    enabled: Arc<AtomicBool>,
    buffer:  Arc<Mutex<Vec<f32>>>,
    cap:     usize,
}

impl AudioCapture {
    fn new (cap: usize) -> AudioCapture {
        AudioCapture { enabled: Arc::new(AtomicBool::new(false)), buffer: Arc::new(Mutex::new(Vec::with_capacity(cap))), cap }
    }

    pub fn start (&self) {
        self.buffer.lock().unwrap().clear();
        self.enabled.store(true, Ordering::Relaxed);
    }

    pub fn is_full (&self) -> bool {
        self.buffer.lock().unwrap().len() >= self.cap
    }

    pub fn samples (&self) -> Vec<f32> {
        self.buffer.lock().unwrap().clone()
    }

    // Called from the audio callback -- cheap no-op once disabled/full.
    fn push (&self, sample: f32) {
        if !self.enabled.load(Ordering::Relaxed) { return; }
        let mut buf = self.buffer.lock().unwrap();
        if buf.len() < self.cap {
            buf.push(sample);
        } else {
            self.enabled.store(false, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, Copy)]
struct SignalWeights {
    pitch:        f32,
    width:        f32,
    filter:       f32,
    fuzz:         f32,
    thump:        f32,
    velocity:     f32,
    acceleration: f32,
    lfo:          [f32; 4],
}

impl SignalWeights {
    const NONE: SignalWeights = SignalWeights {
        pitch: 0.0, width: 0.0, filter: 0.0, fuzz: 0.0, thump: 0.0, velocity: 0.0, acceleration: 0.0,
        lfo: [0.0; 4],
    };
}

#[derive(Clone)]
pub struct ParamSpec {
    pub name: &'static str,
    default:  Shared,
    lo:       Shared,
    hi:       Shared,
    // 0..1, continuously blends the lo/hi interpolation curve between
    // linear (0) and exponential (1) -- see param_factor.
    curve:    Shared,
    weight_pitch:        Shared,
    weight_width:        Shared,
    weight_filter:       Shared,
    weight_fuzz:         Shared,
    weight_thump:        Shared,
    weight_velocity:     Shared,
    weight_acceleration: Shared,
    weight_lfo:          [Shared; 4],
}

impl ParamSpec {
    fn new (name: &'static str, default: f32, range: (f32, f32), curve: f32, weights: SignalWeights) -> ParamSpec {
        ParamSpec {
            name,
            default: Shared::new(default),
            lo:      Shared::new(range.0),
            hi:      Shared::new(range.1),
            curve:   Shared::new(curve),
            weight_pitch:        Shared::new(weights.pitch),
            weight_width:        Shared::new(weights.width),
            weight_filter:       Shared::new(weights.filter),
            weight_fuzz:         Shared::new(weights.fuzz),
            weight_thump:        Shared::new(weights.thump),
            weight_velocity:     Shared::new(weights.velocity),
            weight_acceleration: Shared::new(weights.acceleration),
            weight_lfo:          std::array::from_fn(|i| Shared::new(weights.lfo[i])),
        }
    }

    // (column label, cell) pairs for every draggable numeric field, in
    // display order: default/lo/hi first, then exactly ModMatrix::COLS.
    pub fn cells (&self) -> [(&'static str, &Shared); 15] {
        [
            ("default", &self.default),
            ("lo",      &self.lo),
            ("hi",      &self.hi),
            ("curve",   &self.curve),
            ("pitch",   &self.weight_pitch),
            ("width",   &self.weight_width),
            ("filter",  &self.weight_filter),
            ("fuzz",    &self.weight_fuzz),
            ("thump",   &self.weight_thump),
            ("vel",     &self.weight_velocity),
            ("acc",     &self.weight_acceleration),
            ("lfo_1",   &self.weight_lfo[0]),
            ("lfo_2",   &self.weight_lfo[1]),
            ("lfo_3",   &self.weight_lfo[2]),
            ("lfo_4",   &self.weight_lfo[3]),
        ]
    }
}

// Blends a param's default toward its range endpoints, weighted by how much
// each live signal should influence it. All-zero weights => always
// `default`. `curve` continuously blends the interpolation shape between
// linear and exponential (0 = fully linear, 1 = fully exponential) rather
// than switching discretely between them.
fn param_factor (spec: &ParamSpec, signal: &SignalState) -> f32 {
    let lo = spec.lo.value();
    let hi = spec.hi.value();
    let default = spec.default.value();
    let curve_amt = spec.curve.value().clamp(0.0, 1.0);

    // linexp's exponential mapping is only well-defined for lo/hi that
    // don't touch zero or flip sign; many rows (anything ranging from 0,
    // e.g. main_sub_lvl, gen_N_lvl) are curve=0 (fully linear) precisely
    // because of this, so skip evaluating the exp branch entirely there --
    // NaN * 0.0 is still NaN, not 0.0, so a short-circuit is required, not
    // just a zero curve weight.
    let interp = |t: f32| {
        let linear = lo + (hi - lo) * t;
        if curve_amt <= 0.0 { return linear; }
        let exp = linexp(0.0, 1.0, lo, hi, t);
        linear + (exp - linear) * curve_amt
    };

    let mut result = default;
    result += spec.weight_pitch.value()        * (interp(signal.bend)         - default);
    result += spec.weight_width.value()        * (interp(signal.width)        - default);
    result += spec.weight_filter.value()       * (interp(signal.filter)       - default);
    result += spec.weight_fuzz.value()         * (interp(signal.fuzz)         - default);
    result += spec.weight_thump.value()        * (interp(signal.thump)        - default);
    result += spec.weight_velocity.value()     * (interp(signal.velocity)     - default);
    result += spec.weight_acceleration.value() * (interp(signal.acceleration) - default);
    for (weight, &lfo) in spec.weight_lfo.iter().zip(signal.lfo.iter()) {
        result += weight.value() * (interp(lfo) - default);
    }

    result.clamp(lo.min(hi), lo.max(hi))
}

// Thin AudioNode adapter over param_factor: makes ParamSpec graph-notation-
// capable (composable via >>/|/etc, boxable as Box<dyn AudioUnit>) without
// changing param_factor's existing call sites, which stay on the cheap
// &self path rather than needing a live &mut ParamSpec pulled out of the
// widely shared Arc<ModMatrix>.
// Input order matches SignalWeights: [pitch(bend), width, filter, fuzz, thump, velocity, acceleration].
impl AudioNode for ParamSpec {
    const ID: u64 = 0x7A_20;
    type Inputs = U7;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U1> {
        let signal = SignalState {
            bend: input[0], width: input[1], filter: input[2], fuzz: input[3],
            thump: input[4], velocity: input[5], acceleration: input[6],
            ..SignalState::new()
        };
        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = param_factor(self, &signal);
        output
    }
}

// The mod matrix: a named row/column grid of weighted params. Every row is
// a ParamSpec (default/lo/hi/curve + one weight per COLS entry); rows are
// looked up by name rather than held as individual struct fields, so
// swappable gen1-4/fx1-4 slots can share one uniform row template
// regardless of which GenNode/FxNode currently occupies them.
pub const ROWS: [&str; 66] = [
    "amp_blend", "amp_boost", "main_sub_lvl", "main_sub_wave", "dry_sub_lvl",
    "thump_peak", "thump_decay",
    "comp_thresh", "comp_attack", "comp_depth",
    "crush_thresh", "crush_attack", "crush_depth", "crush_boost",
    "reverb_size", "reverb_decay", "reverb_damp", "reverb_wet",

    "gen_1_lvl", "gen_1_p1", "gen_1_p2", "gen_1_p3", "gen_1_p4",
    "gen_2_lvl", "gen_2_p1", "gen_2_p2", "gen_2_p3", "gen_2_p4",
    "gen_3_lvl", "gen_3_p1", "gen_3_p2", "gen_3_p3", "gen_3_p4",
    "gen_4_lvl", "gen_4_p1", "gen_4_p2", "gen_4_p3", "gen_4_p4",
    "fx_1_lvl", "fx_1_p1", "fx_1_p2", "fx_1_p3", "fx_1_p4",
    "fx_2_lvl", "fx_2_p1", "fx_2_p2", "fx_2_p3", "fx_2_p4",
    "fx_3_lvl", "fx_3_p1", "fx_3_p2", "fx_3_p3", "fx_3_p4",
    "fx_4_lvl", "fx_4_p1", "fx_4_p2", "fx_4_p3", "fx_4_p4",
    "lfo_1_rate", "lfo_1_depth",
    "lfo_2_rate", "lfo_2_depth",
    "lfo_3_rate", "lfo_3_depth",
    "lfo_4_rate", "lfo_4_depth",
];

pub const COLS: [&str; 12] = [
    "curve",
    "pitch", "width", "filter", "fuzz", "thump", "vel", "acc",
    "lfo_1", "lfo_2", "lfo_3", "lfo_4",
];

const LFO_RATE_NAMES:  [&str; 4] = ["lfo_1_rate",  "lfo_2_rate",  "lfo_3_rate",  "lfo_4_rate"];
const LFO_DEPTH_NAMES: [&str; 4] = ["lfo_1_depth", "lfo_2_depth", "lfo_3_depth", "lfo_4_depth"];
const GEN_ROWS: [[&str; 5]; 4] = [
    ["gen_1_lvl", "gen_1_p1", "gen_1_p2", "gen_1_p3", "gen_1_p4"],
    ["gen_2_lvl", "gen_2_p1", "gen_2_p2", "gen_2_p3", "gen_2_p4"],
    ["gen_3_lvl", "gen_3_p1", "gen_3_p2", "gen_3_p3", "gen_3_p4"],
    ["gen_4_lvl", "gen_4_p1", "gen_4_p2", "gen_4_p3", "gen_4_p4"],
];
const FX_ROWS: [[&str; 5]; 4] = [
    ["fx_1_lvl", "fx_1_p1", "fx_1_p2", "fx_1_p3", "fx_1_p4"],
    ["fx_2_lvl", "fx_2_p1", "fx_2_p2", "fx_2_p3", "fx_2_p4"],
    ["fx_3_lvl", "fx_3_p1", "fx_3_p2", "fx_3_p3", "fx_3_p4"],
    ["fx_4_lvl", "fx_4_p1", "fx_4_p2", "fx_4_p3", "fx_4_p4"],
];

pub struct ModMatrix {
    rows: HashMap<&'static str, ParamSpec>,
}

impl ModMatrix {
    pub fn new () -> ModMatrix {
        let mut rows = HashMap::new();
        for &name in ROWS.iter() {
            rows.insert(name, ModMatrix::default_spec(name));
        }
        ModMatrix { rows }
    }

    // Per-row default/range/curve/weights. The 18 named rows are
    // individually tuned (pulled from this project's previous per-field
    // defaults where a mapping exists -- see the audio engine refactor
    // plan); the gen1-4/fx1-4 lvl/p1-4 rows and the 8 LFO rows share one
    // uniform template per group, matched by name suffix.
    fn default_spec (name: &'static str) -> ParamSpec {
        let n = SignalWeights::NONE;
        match name {
            "amp_blend"     => ParamSpec::new(name, 0.0,   (0.0, 1.0),    0.0, SignalWeights { fuzz: 1.0, ..n }),
            "amp_boost"     => ParamSpec::new(name, 1.0,   (1.0, 4.0),    0.0, n),
            "main_sub_lvl"  => ParamSpec::new(name, 0.35,  (0.0, 1.0),    0.0, n),
            "main_sub_wave" => ParamSpec::new(name, 0.0,   (0.0, 1.0),    0.0, n),
            "dry_sub_lvl"   => ParamSpec::new(name, 0.35,  (0.0, 1.0),    0.0, n),
            "thump_peak"    => ParamSpec::new(name, 1.5,   (0.0, 1.5),    0.0, SignalWeights { thump: 1.0, ..n }),
            "thump_decay"   => ParamSpec::new(name, 0.18,  (0.02, 1.0),   0.0, n),
            "comp_thresh"   => ParamSpec::new(name, -18.0, (-60.0, 0.0),  0.0, n),
            "comp_attack"   => ParamSpec::new(name, 0.005, (0.0005, 0.2), 1.0, n),
            "comp_depth"    => ParamSpec::new(name, 0.7,   (0.0, 1.0),    0.0, n),
            "crush_thresh"  => ParamSpec::new(name, -12.0, (-60.0, 0.0),  0.0, n),
            "crush_attack"  => ParamSpec::new(name, 0.01,  (0.0005, 0.2), 1.0, n),
            "crush_depth"   => ParamSpec::new(name, 0.4,   (0.0, 1.0),    0.0, n),
            "crush_boost"   => ParamSpec::new(name, 0.0,   (-24.0, 24.0), 0.0, n),
            "reverb_size"   => ParamSpec::new(name, 10.0,  (10.0, 30.0),  0.0, n),
            "reverb_decay"  => ParamSpec::new(name, 0.6,   (0.1, 4.0),    1.0, n),
            "reverb_damp"   => ParamSpec::new(name, 0.5,   (0.0, 1.0),    0.0, n),
            "reverb_wet"    => ParamSpec::new(name, 0.12,  (0.0, 1.0),    0.0, n),
            _ if name.ends_with("_lvl")   => ParamSpec::new(name, 0.3,  (0.0, 1.0),   0.0, n),
            _ if name.ends_with("_rate")  => ParamSpec::new(name, 2.0,  (0.01, 20.0), 1.0, n),
            _ if name.ends_with("_depth") => ParamSpec::new(name, 0.0,  (0.0, 1.0),   0.0, n),
            _ /* gen_N_pX / fx_N_pX */    => ParamSpec::new(name, 0.5,  (0.0, 1.0),   0.0, n),
        }
    }

    pub fn get (&self, row: &str) -> &ParamSpec {
        self.rows.get(row).unwrap_or_else(|| panic!("unknown ModMatrix row: {row}"))
    }

    // All rows in ROWS order, for building the UI grid / snapshotting.
    pub fn entries (&self) -> Vec<&ParamSpec> {
        ROWS.iter().map(|&name| self.get(name)).collect()
    }
}

// Direct handle onto the note gate, for a "hold note" audition button to
// drive from the GUI thread -- bypasses DeltaEvent/DeltaConsumer entirely,
// same shared-atomic mechanism as everything else here. Note: holding this
// open at the same time as a real controller note will fight over the same
// `freq`/`gate` cells; it's a manual audition tool, not a second voice.
#[derive(Clone)]
pub struct AuditionNote {
    freq: Shared,
    gate: Shared,
}

impl AuditionNote {
    pub fn hold (&self, note: u8) {
        self.freq.set_value(midi_hz(note as f32));
        self.gate.set_value(GATE_ON);
    }

    pub fn release (&self) {
        self.gate.set_value(GATE_OFF);
    }
}

// Every GUI-facing handle onto a running AudioOutput, bundled so main.rs/
// gui.rs thread one Option through instead of one per feature.
#[derive(Clone)]
pub struct AudioHandles {
    pub mod_matrix:    Arc<ModMatrix>,
    pub nam_models:    NamModelCycler,
    pub audition_note: AuditionNote,
    pub gen_1: GenCycler, pub gen_2: GenCycler, pub gen_3: GenCycler, pub gen_4: GenCycler,
    pub fx_1:  FxCycler,  pub fx_2:  FxCycler,  pub fx_3:  FxCycler,  pub fx_4:  FxCycler,
    // Fixed-stage bypass -- level=0/1, same Shared the audio thread reads
    // as each stage's FxNode `level` input. No separate flag underneath.
    pub compressor_level: Shared,
    pub lowpass_level:    Shared,
    pub nam_level:        Shared,
    pub crusher_level:    Shared,
    pub reverb_level:     Shared,
    pub capture: AudioCapture,
}

pub struct AudioOutput {
    freq:              Shared,
    gate:              Shared,
    bend:              Shared,
    width:             Shared,
    filter:            Shared,
    fuzz:              Shared,
    thump_amt:         Shared,
    thump_trigger:     Shared,
    velocity:          Shared,
    acceleration:      Shared,
    nam_selected:      Shared,
    nam_model_names:   Arc<Vec<String>>,
    gen_1_selected: Shared, gen_1_extra: Shared,
    gen_2_selected: Shared, gen_2_extra: Shared,
    gen_3_selected: Shared, gen_3_extra: Shared,
    gen_4_selected: Shared, gen_4_extra: Shared,
    fx_1_selected: Shared, fx_2_selected: Shared, fx_3_selected: Shared, fx_4_selected: Shared,
    compressor_level: Shared,
    lowpass_level:    Shared,
    nam_level:        Shared,
    crusher_level:    Shared,
    reverb_level:     Shared,
    capture:           AudioCapture,
    mod_matrix:        Arc<ModMatrix>,
    stream:            cpal::Stream,
}

impl AudioOutput {
    // Every handle a UI needs to drive/display this engine, bundled. Cheap
    // to build (every field is an Arc'd atomic cell or Arc'd name list).
    pub fn handles (&self) -> AudioHandles {
        AudioHandles {
            mod_matrix:    self.mod_matrix.clone(),
            nam_models:    NamModelCycler::new(self.nam_selected.clone(), self.nam_model_names.clone()),
            audition_note: AuditionNote { freq: self.freq.clone(), gate: self.gate.clone() },
            gen_1: GenCycler::new(self.gen_1_selected.clone(), self.gen_1_extra.clone()),
            gen_2: GenCycler::new(self.gen_2_selected.clone(), self.gen_2_extra.clone()),
            gen_3: GenCycler::new(self.gen_3_selected.clone(), self.gen_3_extra.clone()),
            gen_4: GenCycler::new(self.gen_4_selected.clone(), self.gen_4_extra.clone()),
            fx_1: FxCycler::new(self.fx_1_selected.clone()),
            fx_2: FxCycler::new(self.fx_2_selected.clone()),
            fx_3: FxCycler::new(self.fx_3_selected.clone()),
            fx_4: FxCycler::new(self.fx_4_selected.clone()),
            compressor_level: self.compressor_level.clone(),
            lowpass_level:    self.lowpass_level.clone(),
            nam_level:        self.nam_level.clone(),
            crusher_level:    self.crusher_level.clone(),
            reverb_level:     self.reverb_level.clone(),
            capture:          self.capture.clone(),
        }
    }

    pub fn new () -> io::Result<AudioOutput> {
        println!("║ Starting native audio backend... ");

        let freq          = shared(110.0);
        let gate          = shared(GATE_OFF);
        let bend          = shared(0.0);
        let width         = shared(0.0);
        let filter        = shared(0.0);
        let fuzz          = shared(0.0);
        let thump_amt     = shared(0.0);
        let thump_trigger = shared(0.0);
        let velocity      = shared(0.0);
        let acceleration  = shared(0.0);

        // Default to Bypass (index 0) on all four gen slots and all four fx
        // slots -- a fresh run isn't a wall of noise.
        let gen_1_selected = shared(0.0); let gen_1_extra = shared(0.0);
        let gen_2_selected = shared(0.0); let gen_2_extra = shared(0.0);
        let gen_3_selected = shared(0.0); let gen_3_extra = shared(0.0);
        let gen_4_selected = shared(0.0); let gen_4_extra = shared(0.0);
        let fx_1_selected = shared(0.0);
        let fx_2_selected = shared(0.0);
        let fx_3_selected = shared(0.0);
        let fx_4_selected = shared(0.0);

        let mod_matrix = Arc::new(ModMatrix::new());
        let capture = AudioCapture::new((NAM_SAMPLE_RATE as f32 * CAPTURE_SECONDS) as usize);

        println!("║ Loading NAM models... ");
        let (nam_model_list, nam_model_name_list) = nam::load_nam_models()?;
        let nam_model_names = Arc::new(nam_model_name_list);
        let default_nam_index = nam::default_model_index(&nam_model_names);
        let nam_selected = shared(default_nam_index as f32);
        println!("║ NAM models loaded: {}", nam_model_names.join(", "));

        // Fixed-stage bypass levels -- defaults reproduce today's audible
        // behavior (compressor/crusher started bypassed, lowpass/nam ran).
        let compressor_level = shared(0.0);
        let lowpass_level    = shared(1.0);
        let nam_level         = shared(1.0);
        let crusher_level     = shared(0.0);
        let reverb_level      = shared(1.0);

        let mut audition_node = AuditionNode::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(), filter.clone(), fuzz.clone(),
            thump_amt.clone(), thump_trigger.clone(), velocity.clone(), acceleration.clone(),
            [gen_1_selected.clone(), gen_2_selected.clone(), gen_3_selected.clone(), gen_4_selected.clone()],
            [gen_1_extra.clone(), gen_2_extra.clone(), gen_3_extra.clone(), gen_4_extra.clone()],
            [fx_1_selected.clone(), fx_2_selected.clone(), fx_3_selected.clone(), fx_4_selected.clone()],
            nam_model_list, nam_selected.clone(),
            compressor_level.clone(), lowpass_level.clone(), nam_level.clone(), crusher_level.clone(), reverb_level.clone(),
            mod_matrix.clone(),
        );

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        println!("║ Output device: {device}");
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;
        println!("║ Output config: {supported:?}");

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        audition_node.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, audition_node, capture.clone(), err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, audition_node, capture.clone(), err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, audition_node, capture.clone(), err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native audio backend OK.");

        Ok(AudioOutput {
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            nam_selected, nam_model_names,
            gen_1_selected, gen_1_extra, gen_2_selected, gen_2_extra,
            gen_3_selected, gen_3_extra, gen_4_selected, gen_4_extra,
            fx_1_selected, fx_2_selected, fx_3_selected, fx_4_selected,
            compressor_level, lowpass_level, nam_level, crusher_level, reverb_level,
            capture, mod_matrix, stream,
        })
    }
}

// Picks an output config at exactly `target_rate` to match NAM A2 models
fn pick_output_config (device: &cpal::Device, target_rate: u32) -> io::Result<cpal::SupportedStreamConfig> {
    let mut ranges = device.supported_output_configs()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("no output configs available: {e}")))?;

    if let Some(range) = ranges.find(|r| r.min_sample_rate() <= target_rate && r.max_sample_rate() >= target_rate) {
        return Ok(range.with_sample_rate(target_rate));
    }

    println!("║ ⚠ Output device has no config supporting {target_rate}Hz (the rate every NAM model in nam/ was captured at) -- NAM output will be pitched/timed wrong. Falling back to the device default.");

    device.default_output_config()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("no usable output config: {e}")))
}

// Owns the full per-note graph: main_sub + gen1-4 -> compressor -> fx1-4 ->
// lowpass -> nam -> crusher -> reverb, combined with dry_sub at the very
// end. Box<dyn GenNode>/Box<dyn FxNode> can't use fundsp's static >>/|
// combinators (those need concrete generic types), so this stays a manual
// sequential dispatch through each owned child, same imperative shape the
// old build_stream had, just through the new GenNode/FxNode-shaped slots
// instead of hardcoded concrete types.
struct AuditionNode {
    freq: Shared, gate: Shared, bend: Shared, width: Shared, filter: Shared, fuzz: Shared,
    thump_amt: Shared, thump_trigger: Shared, velocity: Shared, acceleration: Shared,

    main_sub_tri: An<WaveSynth<U1>>,
    main_sub_saw: An<WaveSynth<U1>>,
    dry_sub:      An<Sine<f64>>,
    gens:         [GenSlot; 4],
    envelope:     Box<dyn AudioUnit>,
    lfos:         [An<Sine<f64>>; 4],
    last_lfo:     [f32; 4],

    compressor:       Crusher,
    compressor_level: Shared,
    fxs:              [FxSlot; 4],
    lowpass:          LowpassFx,
    lowpass_level:    Shared,
    nam:              NamStage,
    nam_level:        Shared,
    crusher:          Crusher,
    crusher_level:    Shared,
    reverb:           ReverbFx,
    reverb_level:     Shared,

    thump_last_trigger:    f32,
    thump_elapsed_samples: f32,
    sample_rate:           f32,

    // Every ParamSpec this node reads, cloned out of the shared ModMatrix
    // once at construction instead of looked up by name every sample.
    // ParamSpec::clone() is cheap (just bumps each field's Shared/Arc
    // refcount) and still shares the same live cells the GUI edits -- this
    // exists purely to avoid a HashMap<&str,_> probe (string hash + lookup)
    // ~66 times per sample, which was audibly stuttering the output.
    spec_main_sub_lvl:  ParamSpec,
    spec_main_sub_wave: ParamSpec,
    spec_dry_sub_lvl:   ParamSpec,
    spec_thump_peak:    ParamSpec,
    spec_thump_decay:   ParamSpec,
    spec_comp_thresh:   ParamSpec,
    spec_comp_attack:   ParamSpec,
    spec_comp_depth:    ParamSpec,
    spec_crush_thresh:  ParamSpec,
    spec_crush_attack:  ParamSpec,
    spec_crush_depth:   ParamSpec,
    spec_crush_boost:   ParamSpec,
    spec_amp_blend:     ParamSpec,
    spec_amp_boost:     ParamSpec,
    spec_reverb_wet:    ParamSpec,
    spec_lfo_rate:      [ParamSpec; 4],
    spec_lfo_depth:     [ParamSpec; 4],
    spec_gen:           [[ParamSpec; 5]; 4], // [lvl, p1, p2, p3, p4]
    spec_fx:            [[ParamSpec; 5]; 4],
}

impl AuditionNode {
    fn new (
        freq: Shared, gate: Shared, bend: Shared, width: Shared, filter: Shared, fuzz: Shared,
        thump_amt: Shared, thump_trigger: Shared, velocity: Shared, acceleration: Shared,
        gen_selected: [Shared; 4], gen_extra: [Shared; 4], fx_selected: [Shared; 4],
        nam_models: Vec<Option<nam::NamModelSlot>>, nam_selected: Shared,
        compressor_level: Shared, lowpass_level: Shared, nam_level: Shared, crusher_level: Shared, reverb_level: Shared,
        matrix: Arc<ModMatrix>,
    ) -> AuditionNode {
        let rest = SignalState::new();

        let gens: [GenSlot; 4] = std::array::from_fn(|i| GenSlot::new(gen_selected[i].clone(), gen_extra[i].clone()));
        let fxs:  [FxSlot; 4]  = std::array::from_fn(|i| FxSlot::new(fx_selected[i].clone()));

        // Fixed per-position constants for the two Crusher instances --
        // pinned to this project's previous comp_a/comp_b defaults (see the
        // audio engine refactor plan's compressor/crusher knob mapping).
        let compressor = Crusher::new(6.0, -40.0, 3.0, 0.15, 1.0);
        let crusher     = Crusher::new(2.5, -36.0, 1.8, 0.25, 1.0);

        let reverb_size  = param_factor(matrix.get("reverb_size"), &rest);
        let reverb_decay = param_factor(matrix.get("reverb_decay"), &rest);
        let reverb_damp  = param_factor(matrix.get("reverb_damp"), &rest);

        let spec = |name: &str| matrix.get(name).clone();
        let spec_gen: [[ParamSpec; 5]; 4] = std::array::from_fn(|i| GEN_ROWS[i].map(|name| spec(name)));
        let spec_fx:  [[ParamSpec; 5]; 4] = std::array::from_fn(|i| FX_ROWS[i].map(|name| spec(name)));

        AuditionNode {
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            main_sub_tri: triangle(),
            main_sub_saw: saw(),
            dry_sub:      sine(),
            gens,
            envelope: Box::new(adsr_live(ENVELOPE_ATTACK, 0.0, 1.0, ENVELOPE_RELEASE)),
            lfos:     std::array::from_fn(|_| sine()),
            last_lfo: [0.0; 4],

            compressor, compressor_level,
            fxs,
            lowpass: LowpassFx::new(), lowpass_level,
            nam: NamStage::new(nam_models, nam_selected), nam_level,
            crusher, crusher_level,
            reverb: ReverbFx::new(reverb_size, reverb_decay, reverb_damp), reverb_level,

            thump_last_trigger:    0.0,
            thump_elapsed_samples: 0.0,
            sample_rate:           DEFAULT_SR as f32,

            spec_main_sub_lvl:  spec("main_sub_lvl"),
            spec_main_sub_wave: spec("main_sub_wave"),
            spec_dry_sub_lvl:   spec("dry_sub_lvl"),
            spec_thump_peak:    spec("thump_peak"),
            spec_thump_decay:   spec("thump_decay"),
            spec_comp_thresh:   spec("comp_thresh"),
            spec_comp_attack:   spec("comp_attack"),
            spec_comp_depth:    spec("comp_depth"),
            spec_crush_thresh:  spec("crush_thresh"),
            spec_crush_attack:  spec("crush_attack"),
            spec_crush_depth:   spec("crush_depth"),
            spec_crush_boost:   spec("crush_boost"),
            spec_amp_blend:     spec("amp_blend"),
            spec_amp_boost:     spec("amp_boost"),
            spec_reverb_wet:    spec("reverb_wet"),
            spec_lfo_rate:      std::array::from_fn(|i| spec(LFO_RATE_NAMES[i])),
            spec_lfo_depth:     std::array::from_fn(|i| spec(LFO_DEPTH_NAMES[i])),
            spec_gen, spec_fx,
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub_tri.set_sample_rate(sr);
        self.main_sub_saw.set_sample_rate(sr);
        self.dry_sub.set_sample_rate(sr);
        for gen in self.gens.iter_mut() { gen.set_sample_rate(sr); }
        self.envelope.set_sample_rate(sr);
        for lfo in self.lfos.iter_mut() { lfo.set_sample_rate(sr); }
        self.compressor.set_sample_rate(sr);
        for fx in self.fxs.iter_mut() { fx.set_sample_rate(sr); }
        self.lowpass.set_sample_rate(sr);
        self.nam.set_sample_rate(sr);
        self.crusher.set_sample_rate(sr);
        self.reverb.set_sample_rate(sr);
        self.sample_rate = sr as f32;
    }

    // Everything up to (not including) the NAM stage: generators through
    // the compressor/fx chain and the fixed lowpass. Returns (dry, dry_sub)
    // -- dry_sub bypasses NAM/crusher/reverb entirely and is re-added at the
    // very end by tick_post_nam. Split out of the old single-sample tick()
    // so build_stream can batch every sample's `dry` into a block and run
    // the NAM stage once per block instead of once per sample -- see
    // run_nam and NamStage::process_block in nam.rs for why.
    fn tick_pre_nam (&mut self) -> (f32, f32) {
        let mut signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            lfo: self.last_lfo,
            ..SignalState::new()
        };

        for i in 0..4 {
            let rate  = param_factor(&self.spec_lfo_rate[i], &signal);
            let depth = param_factor(&self.spec_lfo_depth[i], &signal);
            self.last_lfo[i] = self.lfos[i].filter_mono(rate) * depth;
        }
        signal.lfo = self.last_lfo;

        let bend_mult = 2f32.powf(signal.bend);
        let base_freq = self.freq.value() * bend_mult * self.tick_thump(&signal);

        let main_sub_lvl  = param_factor(&self.spec_main_sub_lvl, &signal);
        let main_sub_wave = param_factor(&self.spec_main_sub_wave, &signal);
        let tri = self.main_sub_tri.filter_mono(base_freq);
        let saw = self.main_sub_saw.filter_mono(base_freq);
        let main_sub = (tri * (1.0 - main_sub_wave) + saw * main_sub_wave) * main_sub_lvl;

        let mut gen_sum = main_sub;
        for i in 0..4 {
            let rows = &self.spec_gen[i];
            let lvl = param_factor(&rows[0], &signal);
            let p = [
                param_factor(&rows[1], &signal),
                param_factor(&rows[2], &signal),
                param_factor(&rows[3], &signal),
                param_factor(&rows[4], &signal),
            ];
            let (l, _r) = self.gens[i].tick(base_freq, lvl, p);
            gen_sum += l;
        }

        let env = self.envelope.filter_mono(self.gate.value());
        let mut dry = gen_sum * env;

        // dry_sub: hardcoded one octave below base_freq (was a modulatable-
        // in-name-only BYPASS_SUB_RATIO const, removed by request).
        let dry_sub_lvl = param_factor(&self.spec_dry_sub_lvl, &signal);
        let dry_sub = self.dry_sub.filter_mono(base_freq * 0.5) * dry_sub_lvl * env;

        let comp_thresh = param_factor(&self.spec_comp_thresh, &signal);
        let comp_attack = param_factor(&self.spec_comp_attack, &signal);
        let comp_depth  = param_factor(&self.spec_comp_depth, &signal);
        let comp_out = self.compressor.tick(&Frame::from([
            dry, dry, self.compressor_level.value(), comp_thresh, comp_attack, comp_depth, 3.0,
        ]));
        dry = comp_out[0];

        for i in 0..4 {
            let rows = &self.spec_fx[i];
            let lvl = param_factor(&rows[0], &signal);
            let p = [
                param_factor(&rows[1], &signal),
                param_factor(&rows[2], &signal),
                param_factor(&rows[3], &signal),
                param_factor(&rows[4], &signal),
            ];
            let (l, _r) = self.fxs[i].tick(dry, dry, lvl, p);
            dry = l;
        }

        // lowpass: fixed position, before nam. p1 tracks the live `filter`
        // hardware signal directly (no dedicated matrix row for it).
        let lowpass_out = self.lowpass.tick(&Frame::from([
            dry, dry, self.lowpass_level.value(), signal.filter, 0.0, 0.0, 0.0,
        ]));
        dry = lowpass_out[0];

        (dry, dry_sub)
    }

    // amp_blend/amp_boost read once per block (not per sample) right before
    // the batched NAM call -- see run_nam. Both are slow knob-rate values
    // (amp_blend tracks the live `fuzz` hardware signal, amp_boost has no
    // live weights), so block-rate resolution costs nothing audible; this
    // reads whatever self.last_lfo/bend/etc were left at by the most recent
    // sample of the block just finished in tick_pre_nam.
    fn nam_block_params (&self) -> (f32, f32, f32) {
        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            lfo: self.last_lfo,
            ..SignalState::new()
        };
        let blend = param_factor(&self.spec_amp_blend, &signal);
        let boost = param_factor(&self.spec_amp_boost, &signal);
        (self.nam_level.value(), blend, boost)
    }

    // Runs the NAM stage over a whole block in place -- see
    // NamStage::process_block for why this must be a block call, not a
    // per-sample one.
    fn run_nam (&mut self, block: &mut [f32]) {
        let (level, blend, boost) = self.nam_block_params();
        self.nam.process_block(block, level, blend, boost);
    }

    // Everything after the NAM stage: crusher, reverb, final mix with
    // dry_sub (which bypassed NAM entirely). `dry` is this sample's
    // already-batched NAM output (see run_nam).
    fn tick_post_nam (&mut self, dry: f32, dry_sub: f32) -> (f32, f32) {
        // Soft-clip instead of a hard wall so rare transient peaks
        // saturate instead of digitally clipping.
        let mut dry = dry.tanh();

        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            lfo: self.last_lfo,
            ..SignalState::new()
        };

        let crush_thresh = param_factor(&self.spec_crush_thresh, &signal);
        let crush_attack = param_factor(&self.spec_crush_attack, &signal);
        let crush_depth  = param_factor(&self.spec_crush_depth, &signal);
        let crush_boost  = param_factor(&self.spec_crush_boost, &signal);
        let crush_out = self.crusher.tick(&Frame::from([
            dry, dry, self.crusher_level.value(), crush_thresh, crush_attack, crush_depth, crush_boost,
        ]));
        dry = crush_out[0].clamp(-1.0, 1.0);

        let reverb_wet = param_factor(&self.spec_reverb_wet, &signal);
        let reverb_out = self.reverb.tick(&Frame::from([
            dry, dry, self.reverb_level.value(), reverb_wet, 0.0, 0.0, 0.0,
        ]));

        (
            (reverb_out[0] + dry_sub).clamp(-1.0, 1.0),
            (reverb_out[1] + dry_sub).clamp(-1.0, 1.0),
        )
    }

    fn tick_thump (&mut self, signal: &SignalState) -> f32 {
        let trigger = self.thump_trigger.value();
        if trigger != self.thump_last_trigger {
            self.thump_last_trigger = trigger;
            self.thump_elapsed_samples = 0.0;
        }

        let t = self.thump_elapsed_samples / self.sample_rate;
        self.thump_elapsed_samples += 1.0;

        let decay_sec  = param_factor(&self.spec_thump_decay, signal);
        // already includes the live thump amount (weights.thump = 1.0), i.e. == thump * thump_peak
        let pitch_bump = param_factor(&self.spec_thump_peak, signal);
        let decay = (-5.0 * t / decay_sec).exp();
        1.0 + decay * pitch_bump
    }
}

fn build_stream<T> (
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut node: AuditionNode,
    capture: AudioCapture,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;

    // Scratch for the pre-NAM dry signal (and the dry_sub that bypasses NAM
    // entirely) -- sized once here, never reallocated on the audio thread.
    // Chunking by NAM_BLOCK_CAP is just a fixed-size-scratch safety net;
    // real cpal callback sizes are always far smaller.
    let mut dry_scratch:    Vec<f32> = vec![0.0; NAM_BLOCK_CAP];
    let mut drysub_scratch: Vec<f32> = vec![0.0; NAM_BLOCK_CAP];

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels;
            let mut done = 0;

            while done < frames {
                let n = std::cmp::min(frames - done, NAM_BLOCK_CAP);
                let dry_block    = &mut dry_scratch[..n];
                let drysub_block = &mut drysub_scratch[..n];

                for i in 0..n {
                    let (dry, dry_sub) = node.tick_pre_nam();
                    dry_block[i]    = dry;
                    drysub_block[i] = dry_sub;
                }

                // Batched, not per-sample -- see NamStage::process_block.
                node.run_nam(dry_block);

                for i in 0..n {
                    let (left, right) = node.tick_post_nam(dry_block[i], drysub_block[i]);
                    capture.push(left);
                    let frame_start = (done + i) * channels;
                    for ch in 0..channels {
                        data[frame_start + ch] = T::from_sample(if ch % 2 == 0 { left } else { right });
                    }
                }

                done += n;
            }
        },
        err_fn,
        None,
    )
}

impl DeltaConsumer for AudioOutput {
    fn panic (&mut self) {
        self.gate.set_value(GATE_OFF);
    }

    fn handle_signal (&mut self, signal: &SignalState) {
        self.bend.set_value(signal.bend);
        self.width.set_value(signal.width);
        self.filter.set_value(signal.filter);
        self.fuzz.set_value(signal.fuzz);
        self.thump_amt.set_value(signal.thump);
        self.velocity.set_value(signal.velocity);
        self.acceleration.set_value(signal.acceleration);
    }

    fn handle_event (&mut self, delta: &DeltaEvent) {
        match delta {
            DeltaEvent::NoteStart(note) => {
                self.freq.set_value(midi_hz(*note as f32));
                self.gate.set_value(GATE_ON);
                self.thump_trigger.set_value(self.thump_trigger.value() + 1.0);
            },
            DeltaEvent::NoteChange(_, new_note) => {
                self.freq.set_value(midi_hz(*new_note as f32));
                self.gate.set_value(GATE_ON);
            },
            DeltaEvent::NoteEnd(_) => self.gate.set_value(GATE_OFF),
            DeltaEvent::Panic()    => self.gate.set_value(GATE_OFF),
            // VoiceChange is intentionally unhandled here: NAM model
            // selection is decoupled from it -- see NamModelCycler.
            _ => {},
        }
    }
}
