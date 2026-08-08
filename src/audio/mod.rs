
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
//   declick(_s)           - Fade-in declick
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
//   pluck                 - Karplus-Strong plucked string
//   shape / shape_fn       - Waveshaper distortion
//   clip / clip_to         - Signal clipping
//   meter                  - Signal metering
//   monitor                - Monitoring pass-through
//   hold(_hz)              - Sample-and-hold
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

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::output::DeltaConsumer;
use crate::tools::linexp;
use crate::zgicabra::{DeltaEvent, SignalState};

mod nam;
mod stutter;
mod audition;
mod reese;
mod fm;
mod dsf;
mod crusher;
pub mod snapshot;

use nam::NamStage;
use audition::AuditionVoice;
use crusher::Crusher;
pub use nam::NamModelCycler;
pub use audition::{AuditionCycler, AUDITION_PARAM_SLOTS, AUDITION_PARAM_LO, AUDITION_PARAM_HI};

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const NAM_SAMPLE_RATE: u32 = 48_000;
// Fixed pitch ratio for the bypass sub-oscillator (was a modulatable
// ParamSpec, bypass_sub_ratio -- removed by request, not meant to be
// modulated).
const BYPASS_SUB_RATIO: f32 = 0.5;



#[derive(Clone, Copy, PartialEq)]
pub enum Curve { Linear, Exp }

impl Curve {
    fn to_f32 (self) -> f32 {
        match self { Curve::Linear => 0.0, Curve::Exp => 1.0 }
    }

    fn from_f32 (v: f32) -> Curve {
        if v > 0.5 { Curve::Exp } else { Curve::Linear }
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
    fn new (name: &'static str, default: f32, range: (f32, f32), curve: Curve, weights: SignalWeights) -> ParamSpec {
        ParamSpec {
            name,
            default: Shared::new(default),
            lo:      Shared::new(range.0),
            hi:      Shared::new(range.1),
            curve:   Shared::new(curve.to_f32()),
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

    // (column label, cell) pairs for every draggable numeric field, in display order.
    pub fn cells (&self) -> [(&'static str, &Shared); 14] {
        [
            ("default",      &self.default),
            ("lo",           &self.lo),
            ("hi",           &self.hi),
            ("pitch",        &self.weight_pitch),
            ("width",        &self.weight_width),
            ("filter",       &self.weight_filter),
            ("fuzz",         &self.weight_fuzz),
            ("thump",        &self.weight_thump),
            ("velocity",     &self.weight_velocity),
            ("acceleration", &self.weight_acceleration),
            ("lfo1", &self.weight_lfo[0]),
            ("lfo2", &self.weight_lfo[1]),
            ("lfo3", &self.weight_lfo[2]),
            ("lfo4", &self.weight_lfo[3]),
        ]
    }

    pub fn curve (&self) -> Curve {
        Curve::from_f32(self.curve.value())
    }

    pub fn toggle_curve (&self) {
        let next = if self.curve() == Curve::Linear { Curve::Exp } else { Curve::Linear };
        self.curve.set_value(next.to_f32());
    }
}

// Blends a param's default toward its range endpoints, weighted by how much
// each live signal should influence it. All-zero weights => always `default`,
// which is exactly today's plain-const behaviour.
fn param_factor (spec: &ParamSpec, signal: &SignalState) -> f32 {
    let lo = spec.lo.value();
    let hi = spec.hi.value();
    let default = spec.default.value();
    let curve = Curve::from_f32(spec.curve.value());

    let interp = |t: f32| match curve {
        Curve::Linear => lo + (hi - lo) * t,
        Curve::Exp    => linexp(0.0, 1.0, lo, hi, t),
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
// widely shared Arc<VoiceParams>.
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

pub struct VoiceParams {
    attack:           ParamSpec,
    release:          ParamSpec,
    amp:              ParamSpec,
    filter_q:         ParamSpec,
    filter_cutoff_hz: ParamSpec,
    sub_level:        ParamSpec,
    bypass_sub_level: ParamSpec,
    thump_decay_sec:  ParamSpec,
    thump_pitch_mult: ParamSpec,
    audition_a_level: ParamSpec,
    audition_b_level: ParamSpec,
    audition_c_level: ParamSpec,
    audition_d_level: ParamSpec,
    reverb_room_size: ParamSpec,
    reverb_time:      ParamSpec,
    reverb_damping:   ParamSpec,
    reverb_level:     ParamSpec,
    comp_a_threshold_down: ParamSpec, comp_a_ratio_down: ParamSpec,
    comp_a_threshold_up:   ParamSpec, comp_a_ratio_up:   ParamSpec,
    comp_a_attack: ParamSpec, comp_a_release: ParamSpec,
    comp_a_depth:  ParamSpec, comp_a_makeup_gain: ParamSpec, comp_a_mix: ParamSpec,
    comp_b_threshold_down: ParamSpec, comp_b_ratio_down: ParamSpec,
    comp_b_threshold_up:   ParamSpec, comp_b_ratio_up:   ParamSpec,
    comp_b_attack: ParamSpec, comp_b_release: ParamSpec,
    comp_b_depth:  ParamSpec, comp_b_makeup_gain: ParamSpec, comp_b_mix: ParamSpec,
    // 4 general-purpose LFOs: rate (Hz) and depth (0..1, scales the raw
    // -1..1 sine) are themselves weight-matrix rows, and each LFO's live
    // output becomes a `lfo1`..`lfo4` weight-matrix column every other
    // param (including other LFOs, one tick delayed -- see
    // VoiceEngine::tick) can route into. Post-NAM params (filter_q,
    // filter_cutoff_hz, amp) don't see it -- see build_post_nam.
    lfo_rate:  [ParamSpec; 4],
    lfo_depth: [ParamSpec; 4],
}

const LFO_RATE_NAMES:  [&str; 4] = ["lfo1_rate",  "lfo2_rate",  "lfo3_rate",  "lfo4_rate"];
const LFO_DEPTH_NAMES: [&str; 4] = ["lfo1_depth", "lfo2_depth", "lfo3_depth", "lfo4_depth"];

impl VoiceParams {
    // `filter_q` and `amp` are read live every ~2ms via
    // envelope() (see build_post_nam / VoiceEngine::new) rather than baked
    // in once, so GUI edits to their default/lo/hi/curve take effect
    // immediately -- their SignalWeights are always 0 today though, so they
    // only ever track `default` (no live signal drives them yet).
    // `attack` and `release` are still baked into fundsp AudioUnits at
    // graph-construction time (adsr_live's times take a fixed value once)
    // -- editing them only takes effect on the next AudioOutput::new()
    // (process restart). Making them truly live means forking adsr_live's
    // closure, since fundsp doesn't expose those as live audio-rate inputs.
    pub fn new () -> VoiceParams {
        VoiceParams {
            attack:           ParamSpec::new("attack",           0.003,  (0.001, 1.0),     Curve::Linear, SignalWeights::NONE),
            release:          ParamSpec::new("release",          0.1,    (0.01, 2.0),      Curve::Linear, SignalWeights::NONE),
            amp:              ParamSpec::new("amp",               0.3,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            filter_q:         ParamSpec::new("filter_q",          0.6,   (0.1, 4.0),       Curve::Linear, SignalWeights::NONE),
            // range must start at exactly 100.0 -- with weight=1.0 `default` fully cancels (see param_factor)
            filter_cutoff_hz: ParamSpec::new("filter_cutoff_hz", 100.0,  (100.0, 14000.0), Curve::Exp,    SignalWeights { filter: 1.0, ..SignalWeights::NONE }),
            sub_level:        ParamSpec::new("sub_level",        0.35,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            bypass_sub_level: ParamSpec::new("bypass_sub_level", 0.35,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            thump_decay_sec:  ParamSpec::new("thump_decay_sec",  0.18,   (0.02, 1.0),      Curve::Linear, SignalWeights::NONE),
            // range must start at exactly 0.0 -- weight=1.0 reproduces today's `thump * THUMP_PITCH_MULT`
            thump_pitch_mult: ParamSpec::new("thump_pitch_mult", 1.5,    (0.0, 1.5),       Curve::Linear, SignalWeights { thump: 1.0, ..SignalWeights::NONE }),
            // Audition voices A/B/C: cycle through fundsp's Generators (see AuditionVoice) for
            // experimenting with raw oscillator/noise character. Default to silent (index 0 =
            // Bypass on all three, see AudioOutput::new) so a fresh run isn't a wall of noise.
            audition_a_level: ParamSpec::new("audition_a_level", 0.3,    (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            audition_b_level: ParamSpec::new("audition_b_level", 0.3,    (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            audition_c_level: ParamSpec::new("audition_c_level", 0.3,    (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            audition_d_level: ParamSpec::new("audition_d_level", 0.3,    (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            // Reverb: room_size/time/damping are baked into fundsp's FDN at
            // construction (not live audio-rate inputs) -- same
            // restart-to-apply caveat as attack/release above. Only level
            // (dry/wet mix) is read live, per sample.
            reverb_room_size: ParamSpec::new("reverb_room_size", 10.0,   (10.0, 30.0),     Curve::Linear, SignalWeights::NONE),
            reverb_time:      ParamSpec::new("reverb_time",      0.6,    (0.1, 4.0),       Curve::Exp,    SignalWeights::NONE),
            reverb_damping:   ParamSpec::new("reverb_damping",   0.5,    (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            reverb_level:     ParamSpec::new("reverb_level",     0.12,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            // Crusher (OTT-style up+down compressor): comp_a squashes the raw
            // voice mix before filter1/NAM, comp_b is a gentler pass after
            // filter2/nam2. See crusher.rs.
            comp_a_threshold_down: ParamSpec::new("comp_a_threshold_down", -18.0, (-60.0, 0.0), Curve::Linear, SignalWeights::NONE),
            comp_a_ratio_down:     ParamSpec::new("comp_a_ratio_down",       6.0, (1.0, 20.0),  Curve::Exp,    SignalWeights::NONE),
            comp_a_threshold_up:   ParamSpec::new("comp_a_threshold_up",   -40.0, (-60.0, 0.0), Curve::Linear, SignalWeights::NONE),
            comp_a_ratio_up:       ParamSpec::new("comp_a_ratio_up",         3.0, (1.0, 20.0),  Curve::Exp,    SignalWeights::NONE),
            comp_a_attack:         ParamSpec::new("comp_a_attack",         0.005, (0.0005, 0.2), Curve::Exp,   SignalWeights::NONE),
            comp_a_release:        ParamSpec::new("comp_a_release",        0.15, (0.01, 2.0),  Curve::Exp,    SignalWeights::NONE),
            comp_a_depth:          ParamSpec::new("comp_a_depth",           0.7, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
            comp_a_makeup_gain:    ParamSpec::new("comp_a_makeup_gain",     3.0, (-24.0, 24.0),Curve::Linear, SignalWeights::NONE),
            comp_a_mix:            ParamSpec::new("comp_a_mix",             1.0, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
            comp_b_threshold_down: ParamSpec::new("comp_b_threshold_down", -12.0, (-60.0, 0.0), Curve::Linear, SignalWeights::NONE),
            comp_b_ratio_down:     ParamSpec::new("comp_b_ratio_down",       2.5, (1.0, 20.0),  Curve::Exp,    SignalWeights::NONE),
            comp_b_threshold_up:   ParamSpec::new("comp_b_threshold_up",   -36.0, (-60.0, 0.0), Curve::Linear, SignalWeights::NONE),
            comp_b_ratio_up:       ParamSpec::new("comp_b_ratio_up",         1.8, (1.0, 20.0),  Curve::Exp,    SignalWeights::NONE),
            comp_b_attack:         ParamSpec::new("comp_b_attack",          0.01, (0.0005, 0.2), Curve::Exp,  SignalWeights::NONE),
            comp_b_release:        ParamSpec::new("comp_b_release",         0.25, (0.01, 2.0),  Curve::Exp,   SignalWeights::NONE),
            comp_b_depth:          ParamSpec::new("comp_b_depth",           0.4, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
            comp_b_makeup_gain:    ParamSpec::new("comp_b_makeup_gain",     0.0, (-24.0, 24.0),Curve::Linear, SignalWeights::NONE),
            comp_b_mix:            ParamSpec::new("comp_b_mix",             1.0, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
            // Rate in a musically-useful sub-audio range; depth defaults to
            // 0 (silent/inert) so a fresh run isn't wobbling everything --
            // same "off by default" convention as audition_*_level/Bypass.
            lfo_rate:  std::array::from_fn(|i| ParamSpec::new(LFO_RATE_NAMES[i],  2.0, (0.01, 20.0), Curve::Exp,    SignalWeights::NONE)),
            lfo_depth: std::array::from_fn(|i| ParamSpec::new(LFO_DEPTH_NAMES[i], 0.0, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE)),
        }
    }

    // All params in a stable display order, for building the UI grid.
    pub fn entries (&self) -> Vec<&ParamSpec> {
        let mut entries: Vec<&ParamSpec> = vec![
            &self.attack, &self.release, &self.amp, &self.filter_q, &self.filter_cutoff_hz,
            &self.sub_level,
            &self.bypass_sub_level,
            &self.thump_decay_sec, &self.thump_pitch_mult,
            &self.audition_a_level, &self.audition_b_level, &self.audition_c_level, &self.audition_d_level,
            &self.reverb_room_size, &self.reverb_time, &self.reverb_damping, &self.reverb_level,
            &self.comp_a_threshold_down, &self.comp_a_ratio_down, &self.comp_a_threshold_up, &self.comp_a_ratio_up,
            &self.comp_a_attack, &self.comp_a_release, &self.comp_a_depth, &self.comp_a_makeup_gain, &self.comp_a_mix,
            &self.comp_b_threshold_down, &self.comp_b_ratio_down, &self.comp_b_threshold_up, &self.comp_b_ratio_up,
            &self.comp_b_attack, &self.comp_b_release, &self.comp_b_depth, &self.comp_b_makeup_gain, &self.comp_b_mix,
        ];
        entries.extend(self.lfo_rate.iter());
        entries.extend(self.lfo_depth.iter());
        entries
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
    pub voice_params:   Arc<VoiceParams>,
    pub nam_models:     NamModelCycler,
    // Dry/wet blend for the always-on, hardcoded-to-"lowgain" second NAM
    // stage, now run after filter2 (see AudioOutput::new/build_stream).
    pub lowgain_blend:  Shared,
    pub audition_note:  AuditionNote,
    pub audition_a:     AuditionCycler,
    pub audition_b:     AuditionCycler,
    pub audition_c:     AuditionCycler,
    pub audition_d:     AuditionCycler,
    // Per-stage bypass toggles for tuning -- see build_stream.
    pub comp_a_bypass:    Arc<AtomicBool>,
    pub filter1_bypass:   Arc<AtomicBool>,
    pub filter2_bypass:   Arc<AtomicBool>,
    pub nam2_bypass:      Arc<AtomicBool>,
    pub comp_b_bypass:    Arc<AtomicBool>,
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
    lowgain_blend:     Shared,
    audition_a_selected: Shared,
    audition_b_selected: Shared,
    audition_c_selected: Shared,
    audition_d_selected: Shared,
    audition_a_params: [Shared; AUDITION_PARAM_SLOTS],
    audition_b_params: [Shared; AUDITION_PARAM_SLOTS],
    audition_c_params: [Shared; AUDITION_PARAM_SLOTS],
    audition_d_params: [Shared; AUDITION_PARAM_SLOTS],
    comp_a_bypass:    Arc<AtomicBool>,
    filter1_bypass:   Arc<AtomicBool>,
    filter2_bypass:   Arc<AtomicBool>,
    nam2_bypass:      Arc<AtomicBool>,
    comp_b_bypass:    Arc<AtomicBool>,
    voice_params:      Arc<VoiceParams>,
    stream:            cpal::Stream,
}

impl AudioOutput {
    // Every handle a UI needs to drive/display this engine, bundled. Cheap
    // to build (every field is an Arc'd atomic cell or Arc'd name list).
    pub fn handles (&self) -> AudioHandles {
        AudioHandles {
            voice_params:  self.voice_params.clone(),
            nam_models:    NamModelCycler::new(self.nam_selected.clone(), self.nam_model_names.clone()),
            lowgain_blend: self.lowgain_blend.clone(),
            audition_note: AuditionNote { freq: self.freq.clone(), gate: self.gate.clone() },
            audition_a:    AuditionCycler::new(self.audition_a_selected.clone(), self.audition_a_params.clone()),
            audition_b:    AuditionCycler::new(self.audition_b_selected.clone(), self.audition_b_params.clone()),
            audition_c:    AuditionCycler::new(self.audition_c_selected.clone(), self.audition_c_params.clone()),
            audition_d:    AuditionCycler::new(self.audition_d_selected.clone(), self.audition_d_params.clone()),
            comp_a_bypass:  self.comp_a_bypass.clone(),
            filter1_bypass: self.filter1_bypass.clone(),
            filter2_bypass: self.filter2_bypass.clone(),
            nam2_bypass:    self.nam2_bypass.clone(),
            comp_b_bypass:  self.comp_b_bypass.clone(),
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
        // Default to Bypass (index 0) on all three -- see VoiceParams::new's
        // audition_*_level comment for why.
        let audition_a_selected = shared(0.0);
        let audition_b_selected = shared(0.0);
        let audition_c_selected = shared(0.0);
        let audition_d_selected = shared(0.0);
        // Extra-input slots for whichever generator each audition slot has
        // selected -- see AuditionVoice/AuditionCycler (audition.rs). 0.5
        // is a neutral starting point for 0..1-range params (roughness,
        // pulse width); Hz/cents-range params just get dragged up from
        // there in the GUI.
        let audition_a_params: [Shared; AUDITION_PARAM_SLOTS] = std::array::from_fn(|_| shared(0.5));
        let audition_b_params: [Shared; AUDITION_PARAM_SLOTS] = std::array::from_fn(|_| shared(0.5));
        let audition_c_params: [Shared; AUDITION_PARAM_SLOTS] = std::array::from_fn(|_| shared(0.5));
        let audition_d_params: [Shared; AUDITION_PARAM_SLOTS] = std::array::from_fn(|_| shared(0.5));
        let voice_params  = Arc::new(VoiceParams::new());

        let mut voice_engine = VoiceEngine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(),
            filter.clone(), fuzz.clone(), thump_amt.clone(), thump_trigger.clone(),
            velocity.clone(), acceleration.clone(),
            audition_a_selected.clone(), audition_b_selected.clone(), audition_c_selected.clone(), audition_d_selected.clone(),
            audition_a_params.clone(), audition_b_params.clone(), audition_c_params.clone(), audition_d_params.clone(),
            voice_params.clone(),
        );
        let mut filter1 = build_filter1(&filter, voice_params.clone());
        let mut filter2 = build_filter2(&filter, voice_params.clone());

        let mut comp_a = Crusher::new(
            voice_params.comp_a_threshold_down.clone(), voice_params.comp_a_ratio_down.clone(),
            voice_params.comp_a_threshold_up.clone(),   voice_params.comp_a_ratio_up.clone(),
            voice_params.comp_a_attack.clone(), voice_params.comp_a_release.clone(),
            voice_params.comp_a_depth.clone(), voice_params.comp_a_makeup_gain.clone(), voice_params.comp_a_mix.clone(),
        );
        let mut comp_b = Crusher::new(
            voice_params.comp_b_threshold_down.clone(), voice_params.comp_b_ratio_down.clone(),
            voice_params.comp_b_threshold_up.clone(),   voice_params.comp_b_ratio_up.clone(),
            voice_params.comp_b_attack.clone(), voice_params.comp_b_release.clone(),
            voice_params.comp_b_depth.clone(), voice_params.comp_b_makeup_gain.clone(), voice_params.comp_b_mix.clone(),
        );

        // New stages default bypassed so a fresh build sounds like today
        // until opted into; filter2/nam2 default active since both ran
        // unconditionally before this refactor.
        let comp_a_bypass  = Arc::new(AtomicBool::new(true));
        let filter1_bypass = Arc::new(AtomicBool::new(true));
        let filter2_bypass = Arc::new(AtomicBool::new(false));
        let nam2_bypass    = Arc::new(AtomicBool::new(false));
        let comp_b_bypass  = Arc::new(AtomicBool::new(true));

        // room_size/time/damping are baked into fundsp's FDN here, at
        // construction time -- see VoiceParams::new's reverb comment.
        // SignalWeights::NONE on all three means the signal passed in
        // doesn't matter; this just reads each param's current `default`.
        let rest = SignalState::new();
        let mut reverb: Box<dyn AudioUnit> = Box::new(reverb_stereo(
            param_factor(&voice_params.reverb_room_size, &rest),
            param_factor(&voice_params.reverb_time, &rest),
            param_factor(&voice_params.reverb_damping, &rest),
        ));

        println!("║ Loading NAM models... ");
        let (nam_model_list, nam_model_name_list) = nam::load_nam_models()?;
        let nam_model_names = Arc::new(nam_model_name_list);
        let default_nam_index = nam::default_model_index(&nam_model_names);
        let nam_selected = shared(default_nam_index as f32);
        println!("║ NAM models loaded: {}", nam_model_names.join(", "));

        let nam = NamStage::new(nam_model_list, nam_selected.clone(), fuzz.clone());

        // Second NAM stage hardcoded to "lowgain", now run after filter2
        // (see build_stream) -- separate Model instance/state from the
        // cycled `nam` stage above (sharing one would interleave two
        // different audio streams through the same WaveNet history).
        let lowgain_blend = shared(0.5);
        let lowgain_slot = nam::load_nam_model("lowgain")?;
        let lowgain_selected = shared(1.0);
        let nam2 = NamStage::new(vec![None, Some(lowgain_slot)], lowgain_selected, lowgain_blend.clone());

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        voice_engine.set_sample_rate(config.sample_rate as f64);
        filter1.set_sample_rate(config.sample_rate as f64);
        filter2.set_sample_rate(config.sample_rate as f64);
        comp_a.set_sample_rate(config.sample_rate as f64);
        comp_b.set_sample_rate(config.sample_rate as f64);
        reverb.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, voice_engine, comp_a, filter1, nam, filter2, nam2, comp_b, reverb, comp_a_bypass.clone(), filter1_bypass.clone(), filter2_bypass.clone(), nam2_bypass.clone(), comp_b_bypass.clone(), err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, voice_engine, comp_a, filter1, nam, filter2, nam2, comp_b, reverb, comp_a_bypass.clone(), filter1_bypass.clone(), filter2_bypass.clone(), nam2_bypass.clone(), comp_b_bypass.clone(), err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, voice_engine, comp_a, filter1, nam, filter2, nam2, comp_b, reverb, comp_a_bypass.clone(), filter1_bypass.clone(), filter2_bypass.clone(), nam2_bypass.clone(), comp_b_bypass.clone(), err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native audio backend OK.");

        Ok(AudioOutput {
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            nam_selected, nam_model_names, lowgain_blend,
            audition_a_selected, audition_b_selected, audition_c_selected, audition_d_selected,
            audition_a_params, audition_b_params, audition_c_params, audition_d_params,
            comp_a_bypass, filter1_bypass, filter2_bypass, nam2_bypass, comp_b_bypass,
            voice_params, stream,
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

struct VoiceEngine {
    freq:          Shared,
    gate:          Shared,
    bend:          Shared,
    width:         Shared,
    filter:        Shared,
    fuzz:          Shared,
    thump_amt:     Shared,
    thump_trigger: Shared,
    velocity:      Shared,
    acceleration:  Shared,
    params:        Arc<VoiceParams>,

    sub:    An<Sine<f64>>,
    bypass_sub: An<Sine<f64>>,
    audition_a: AuditionVoice,
    audition_b: AuditionVoice,
    audition_c: AuditionVoice,
    audition_d: AuditionVoice,
    envelope: Box<dyn AudioUnit>,
    lfos:     [An<Sine<f64>>; 4],
    last_lfo: [f32; 4],

    thump_last_trigger:    f32,
    thump_elapsed_samples: f32,
    sample_rate:           f32,
}

impl VoiceEngine {
    fn new (
        freq: Shared, gate: Shared, bend: Shared, width: Shared,
        filter: Shared, fuzz: Shared, thump_amt: Shared, thump_trigger: Shared,
        velocity: Shared, acceleration: Shared,
        audition_a_selected: Shared, audition_b_selected: Shared, audition_c_selected: Shared, audition_d_selected: Shared,
        audition_a_params: [Shared; AUDITION_PARAM_SLOTS], audition_b_params: [Shared; AUDITION_PARAM_SLOTS], audition_c_params: [Shared; AUDITION_PARAM_SLOTS], audition_d_params: [Shared; AUDITION_PARAM_SLOTS],
        params: Arc<VoiceParams>,
    ) -> VoiceEngine {
        let rest = SignalState::new();

        VoiceEngine {
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            sub:    sine(),
            bypass_sub: sine(),
            audition_a: AuditionVoice::new(audition_a_selected, audition_a_params),
            audition_b: AuditionVoice::new(audition_b_selected, audition_b_params),
            audition_c: AuditionVoice::new(audition_c_selected, audition_c_params),
            audition_d: AuditionVoice::new(audition_d_selected, audition_d_params),
            envelope: Box::new(adsr_live(
                param_factor(&params.attack, &rest), 0.0, 1.0,
                param_factor(&params.release, &rest),
            )),
            lfos:     std::array::from_fn(|_| sine()),
            last_lfo: [0.0; 4],
            thump_last_trigger:    0.0,
            thump_elapsed_samples: 0.0,
            sample_rate:           DEFAULT_SR as f32,
            params,
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        self.sub.set_sample_rate(sr);
        self.bypass_sub.set_sample_rate(sr);
        self.audition_a.set_sample_rate(sr);
        self.audition_b.set_sample_rate(sr);
        self.audition_c.set_sample_rate(sr);
        self.audition_d.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        for lfo in self.lfos.iter_mut() { lfo.set_sample_rate(sr); }
        self.sample_rate = sr as f32;
    }

    // `dry` => the full voice signal for the effect chain
    // `bypass` => effect bypass (sub-osc)
    // `reverb_level` => live dry/wet mix for the final reverb stage
    fn tick (&mut self) -> (f32, f32, f32) {
        let mut signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            // Last tick's LFO output, so an LFO's own rate/depth can be
            // weighted by another LFO (or itself) without a same-tick
            // circular dependency -- one sample behind, same idea as any
            // audio-rate feedback loop.
            lfo: self.last_lfo,
            ..SignalState::new()
        };

        let params = self.params.clone();
        for i in 0..4 {
            let rate  = param_factor(&params.lfo_rate[i], &signal);
            let depth = param_factor(&params.lfo_depth[i], &signal);
            self.last_lfo[i] = self.lfos[i].filter_mono(rate) * depth;
        }
        // Refresh with this tick's values so every param computed below
        // (not just other LFOs) reacts without a tick of lag.
        signal.lfo = self.last_lfo;

        let bend_mult    = 2f32.powf(signal.bend);
        let base_freq    = self.freq.value() * bend_mult * self.tick_thump(&signal);

        let mut freq_input: Frame<f32, U1> = Frame::default();
        freq_input[0] = base_freq;

        let audition_a_level = param_factor(&params.audition_a_level, &signal);
        let audition_b_level = param_factor(&params.audition_b_level, &signal);
        let audition_c_level = param_factor(&params.audition_c_level, &signal);
        let audition_d_level = param_factor(&params.audition_d_level, &signal);
        let audition_sum =
            self.audition_a.tick(&freq_input)[0] * audition_a_level +
            self.audition_b.tick(&freq_input)[0] * audition_b_level +
            self.audition_c.tick(&freq_input)[0] * audition_c_level +
            self.audition_d.tick(&freq_input)[0] * audition_d_level;

        let sub_level = param_factor(&params.sub_level, &signal);
        let sub       = self.sub.filter_mono(base_freq * 0.5) * sub_level;

        let dry = audition_sum + sub;
        let env = self.envelope.filter_mono(self.gate.value());

        let bypass_level = param_factor(&params.bypass_sub_level, &signal);
        let bypass = self.bypass_sub.filter_mono(base_freq * BYPASS_SUB_RATIO) * bypass_level * env;

        let reverb_level = param_factor(&params.reverb_level, &signal);

        (dry * env, bypass, reverb_level)
    }

    fn tick_thump (&mut self, signal: &SignalState) -> f32 {
        let trigger = self.thump_trigger.value();
        if trigger != self.thump_last_trigger {
            self.thump_last_trigger = trigger;
            self.thump_elapsed_samples = 0.0;
        }

        let t = self.thump_elapsed_samples / self.sample_rate;
        self.thump_elapsed_samples += 1.0;

        let decay_sec  = param_factor(&self.params.thump_decay_sec, signal);
        // already includes the live thump amount (weights.thump = 1.0), i.e. == thump * THUMP_PITCH_MULT
        let pitch_bump = param_factor(&self.params.thump_pitch_mult, signal);
        let decay = (-5.0 * t / decay_sec).exp();
        1.0 + decay * pitch_bump
    }
}

// Pre-NAM steep filter: 4-pole Moog ladder lowpass. Tracks the same
// filter_cutoff_hz/filter_q ParamSpecs as filter2 (build_filter2 below) --
// only the filter type + bypass differ between the two stages.
fn build_filter1 (filter: &Shared, params: Arc<VoiceParams>) -> Box<dyn AudioUnit> {
    let cutoff_params = params.clone();
    let cutoff_hz = var(filter) >> map(move |i: &Frame<f32, U1>| {
        let signal = SignalState { filter: i[0], ..SignalState::new() };
        param_factor(&cutoff_params.filter_cutoff_hz, &signal)
    });

    let q_params = params.clone();
    let q = envelope(move |_t: f64| param_factor(&q_params.filter_q, &SignalState::new()) as f64);

    Box::new((pass() | cutoff_hz | q) >> moog())
}

// Post-NAM gentler filter (2-pole, lets more harmonics through than filter1)
// plus the final amp stage -- this used to be the only post-NAM box.
fn build_filter2 (filter: &Shared, params: Arc<VoiceParams>) -> Box<dyn AudioUnit> {
    let cutoff_params = params.clone();
    let cutoff_hz = var(filter) >> map(move |i: &Frame<f32, U1>| {
        let signal = SignalState { filter: i[0], ..SignalState::new() };
        param_factor(&cutoff_params.filter_cutoff_hz, &signal)
    });

    // filter_q has no live SignalWeights today, so its live value only
    // tracks GUI edits to default/lo/hi/curve -- envelope() re-evaluates
    // that at control rate (~2ms) instead of baking it in once here (which
    // needed a process restart to pick up an edit).
    let q_params = params.clone();
    let q = envelope(move |_t: f64| param_factor(&q_params.filter_q, &SignalState::new()) as f64);
    let filtered = (pass() | cutoff_hz | q) >> lowpass();

    let amp_params = params.clone();
    let amp = envelope(move |_t: f64| param_factor(&amp_params.amp, &SignalState::new()) as f64);
    Box::new(filtered * amp)
}

const NAM_BLOCK_CAP: usize = 4096;

fn build_stream<T> (
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut pre_nam: VoiceEngine,
    mut comp_a: Crusher,
    mut filter1: Box<dyn AudioUnit>,
    mut nam: NamStage,
    mut filter2: Box<dyn AudioUnit>,
    mut nam2: NamStage,
    mut comp_b: Crusher,
    mut reverb: Box<dyn AudioUnit>,
    comp_a_bypass:  Arc<AtomicBool>,
    filter1_bypass: Arc<AtomicBool>,
    filter2_bypass: Arc<AtomicBool>,
    nam2_bypass:    Arc<AtomicBool>,
    comp_b_bypass:  Arc<AtomicBool>,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch = [0.0f32; NAM_BLOCK_CAP];
    let mut bypass_scratch = [0.0f32; NAM_BLOCK_CAP];
    let mut reverb_level_scratch = [0.0f32; NAM_BLOCK_CAP];
    let mut nam_input  = BufferVec::new(1);
    let mut nam_output = BufferVec::new(1);

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels;
            let mut done = 0;

            while done < frames {
                let n = std::cmp::min(frames - done, NAM_BLOCK_CAP);
                let block        = &mut scratch[..n];
                let bypass_block = &mut bypass_scratch[..n];
                let reverb_level_block = &mut reverb_level_scratch[..n];

                for i in 0..n {
                    let (dry, bypass, reverb_level) = pre_nam.tick();
                    block[i] = dry;
                    bypass_block[i] = bypass;
                    reverb_level_block[i] = reverb_level;
                }

                // Bypass checks are once-per-block (Relaxed atomic load),
                // not per-sample -- plenty responsive for a tuning toggle.
                if !comp_a_bypass.load(Ordering::Relaxed) {
                    for s in block.iter_mut() { *s = comp_a.tick(&Frame::from([*s]))[0]; }
                }
                if !filter1_bypass.load(Ordering::Relaxed) {
                    for s in block.iter_mut() { *s = filter1.filter_mono(*s); }
                }

                // AudioNode::process() must be driven in <=MAX_BUFFER_SIZE
                // chunks -- see nam.rs.
                for chunk in block.chunks_mut(MAX_BUFFER_SIZE) {
                    nam_input.channel_f32_mut(0)[..chunk.len()].copy_from_slice(chunk);
                    nam.process(chunk.len(), &nam_input.buffer_ref(), &mut nam_output.buffer_mut());
                    chunk.copy_from_slice(&nam_output.channel_f32_mut(0)[..chunk.len()]);
                }

                for s in block.iter_mut() {
                    // NamStage already normalizes toward TARGET_LOUDNESS_DB and
                    // gain-matches input, so this only needs to catch rare
                    // transient peaks -- soft-clip instead of a hard wall so
                    // those peaks saturate instead of digitally clipping.
                    // bypass_block is added straight to the output below,
                    // after the reverb -- it skips both NAM stages, both
                    // filters, both compressors, and the reverb entirely.
                    *s = s.tanh();
                }

                if !filter2_bypass.load(Ordering::Relaxed) {
                    for s in block.iter_mut() { *s = filter2.filter_mono(*s); }
                }
                for s in block.iter_mut() { *s = s.clamp(-1.0, 1.0); }

                // Second NAM stage: hardcoded to "lowgain", now run after
                // filter2 (see AudioOutput::new). Same <=MAX_BUFFER_SIZE
                // chunking requirement as the first stage.
                if !nam2_bypass.load(Ordering::Relaxed) {
                    for chunk in block.chunks_mut(MAX_BUFFER_SIZE) {
                        nam_input.channel_f32_mut(0)[..chunk.len()].copy_from_slice(chunk);
                        nam2.process(chunk.len(), &nam_input.buffer_ref(), &mut nam_output.buffer_mut());
                        chunk.copy_from_slice(&nam_output.channel_f32_mut(0)[..chunk.len()]);
                    }
                }

                if !comp_b_bypass.load(Ordering::Relaxed) {
                    for s in block.iter_mut() { *s = comp_b.tick(&Frame::from([*s]))[0]; }
                }

                for (i, &mixed) in block.iter().enumerate() {
                    // Reverb tail: fed from the mono mix, dry/wet blended per
                    // channel (live per-sample via VoiceParams::reverb_level,
                    // same weight-matrix mechanism as every other param),
                    // written out as true stereo instead of the uniform mono
                    // broadcast used everywhere upstream.
                    let mix = reverb_level_block[i].clamp(0.0, 1.0);
                    let mut wet = [0.0f32; 2];
                    reverb.tick(&[mixed, mixed], &mut wet);
                    let bypass = bypass_block[i];
                    let left  = T::from_sample((mixed * (1.0 - mix) + wet[0] * mix + bypass).clamp(-1.0, 1.0));
                    let right = T::from_sample((mixed * (1.0 - mix) + wet[1] * mix + bypass).clamp(-1.0, 1.0));

                    let frame_start = (done + i) * channels;
                    for ch in 0..channels {
                        data[frame_start + ch] = if ch % 2 == 0 { left } else { right };
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
            // selection is decoupled from it now -- see NamModelCycler.
            _ => {},
        }
    }
}
