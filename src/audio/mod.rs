
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

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::output::DeltaConsumer;
use crate::tools::linexp;
use crate::zgicabra::{DeltaEvent, SignalState};

mod nam;
mod reese;
mod fm;

use nam::NamStage;
use reese::ReeseVoice;
use fm::FmVoice;
pub use nam::{NamModelCycler, IrCycler};

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const FM_OSCS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];
const REESE_OSCS: [f32; 4] = [-1.0, -0.3333333, 0.3333333, 1.0];

const NAM_SAMPLE_RATE: u32 = 48_000;


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
}

impl SignalWeights {
    const NONE: SignalWeights = SignalWeights {
        pitch: 0.0, width: 0.0, filter: 0.0, fuzz: 0.0, thump: 0.0, velocity: 0.0, acceleration: 0.0,
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
        }
    }

    // (column label, cell) pairs for every draggable numeric field, in display order.
    pub fn cells (&self) -> [(&'static str, &Shared); 10] {
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
    ratio_a:          ParamSpec,
    ratio_b:          ParamSpec,
    ratio_c:          ParamSpec,
    index_b:          ParamSpec,
    index_c:          ParamSpec,
    detune_cents_max: ParamSpec,
    sub_level:        ParamSpec,
    noise_level:      ParamSpec,
    noise_lpf_hz:     ParamSpec,
    bypass_sub_ratio: ParamSpec,
    bypass_sub_level: ParamSpec,
    octave_shift:     ParamSpec,
    thump_decay_sec:  ParamSpec,
    thump_pitch_mult: ParamSpec,
    reese_detune_cents_max: ParamSpec,
    reese_level:            ParamSpec,
    sputter_level:          ParamSpec,
}

impl VoiceParams {
    // `filter_q`, `noise_lpf_hz` and `amp` are read live every ~2ms via
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
            ratio_a:          ParamSpec::new("ratio_a",          1.0,    (0.5, 2.0),       Curve::Linear, SignalWeights::NONE),
            ratio_b:          ParamSpec::new("ratio_b",          1.007,  (0.5, 2.0),       Curve::Linear, SignalWeights::NONE),
            ratio_c:          ParamSpec::new("ratio_c",          2.003,  (0.5, 4.0),       Curve::Linear, SignalWeights::NONE),
            index_b:          ParamSpec::new("index_b",          2.2,    (0.0, 8.0),       Curve::Linear, SignalWeights::NONE),
            index_c:          ParamSpec::new("index_c",          3.5,    (0.0, 8.0),       Curve::Linear, SignalWeights::NONE),
            // range must start at exactly 0.0 -- weight=1.0 reproduces today's `width * DETUNE_CENTS_MAX`
            detune_cents_max: ParamSpec::new("detune_cents_max", 25.0,   (0.0, 25.0),      Curve::Linear, SignalWeights { width: 1.0, ..SignalWeights::NONE }),
            sub_level:        ParamSpec::new("sub_level",        0.35,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            noise_level:      ParamSpec::new("noise_level",      0.05,   (0.0, 0.5),       Curve::Linear, SignalWeights::NONE),
            noise_lpf_hz:     ParamSpec::new("noise_lpf_hz",     4000.0, (200.0, 12000.0), Curve::Linear, SignalWeights::NONE),
            bypass_sub_ratio: ParamSpec::new("bypass_sub_ratio", 0.5,    (0.25, 1.0),      Curve::Linear, SignalWeights::NONE),
            bypass_sub_level: ParamSpec::new("bypass_sub_level", 0.35,   (0.0, 1.0),       Curve::Linear, SignalWeights::NONE),
            octave_shift:     ParamSpec::new("octave_shift",     1.0,    (0.25, 2.0),      Curve::Linear, SignalWeights::NONE),
            thump_decay_sec:  ParamSpec::new("thump_decay_sec",  0.18,   (0.02, 1.0),      Curve::Linear, SignalWeights::NONE),
            // range must start at exactly 0.0 -- weight=1.0 reproduces today's `thump * THUMP_PITCH_MULT`
            thump_pitch_mult: ParamSpec::new("thump_pitch_mult", 1.5,    (0.0, 1.5),       Curve::Linear, SignalWeights { thump: 1.0, ..SignalWeights::NONE }),
            // Reese bass: 4-voice detuned-saw beating stack (see ../zgi-sc/experiments/exp2.scd).
            // range must start at exactly 12.0 -- with weight=1.0 reproduces exp2's width.linlin(0,1,12,35)
            reese_detune_cents_max: ParamSpec::new("reese_detune_cents_max", 12.0, (12.0, 35.0), Curve::Linear, SignalWeights { width: 1.0, ..SignalWeights::NONE }),
            reese_level:            ParamSpec::new("reese_level",            0.25, (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
            // Sputter: triangle sub osc ring-modulated by white noise, gates the noise into a
            // sputtering/crackling texture that tracks base_freq instead of sitting at a fixed pitch.
            sputter_level:          ParamSpec::new("sputter_level",          0.1,  (0.0, 1.0),   Curve::Linear, SignalWeights::NONE),
        }
    }

    // All params in a stable display order, for building the UI grid.
    pub fn entries (&self) -> [&ParamSpec; 22] {
        [
            &self.attack, &self.release, &self.amp, &self.filter_q, &self.filter_cutoff_hz,
            &self.ratio_a, &self.ratio_b, &self.ratio_c, &self.index_b, &self.index_c,
            &self.detune_cents_max, &self.sub_level, &self.noise_level, &self.noise_lpf_hz,
            &self.bypass_sub_ratio, &self.bypass_sub_level, &self.octave_shift,
            &self.thump_decay_sec, &self.thump_pitch_mult,
            &self.reese_detune_cents_max, &self.reese_level, &self.sputter_level,
        ]
    }
}

pub struct AudioOutput {
    freq:            Shared,
    gate:            Shared,
    bend:            Shared,
    width:           Shared,
    filter:          Shared,
    fuzz:            Shared,
    thump_amt:       Shared,
    thump_trigger:   Shared,
    velocity:        Shared,
    acceleration:    Shared,
    nam_selected:    Shared,
    nam_model_names: Arc<Vec<String>>,
    ir_selected:     Shared,
    ir_names:        Arc<Vec<String>>,
    voice_params:    Arc<VoiceParams>,
    stream:          cpal::Stream,
}

impl AudioOutput {
    // Handle to the live parameter matrix, for a UI to read/write. Cheap to
    // clone (each field is an Arc'd atomic cell); the audio thread reads the
    // same cells lock-free every sample.
    pub fn voice_params (&self) -> Arc<VoiceParams> {
        self.voice_params.clone()
    }

    // Handle to the NAM model cycler, for a UI to drive/display. Cheap to
    // clone (an Arc'd atomic cell plus an Arc'd name list).
    pub fn nam_models (&self) -> NamModelCycler {
        NamModelCycler::new(self.nam_selected.clone(), self.nam_model_names.clone())
    }

    // Handle to the IR cycler, for a UI to drive/display. Cheap to clone,
    // same shape as nam_models().
    pub fn nam_irs (&self) -> IrCycler {
        IrCycler::new(self.ir_selected.clone(), self.ir_names.clone())
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
        let voice_params  = Arc::new(VoiceParams::new());

        let mut voice_engine = VoiceEngine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(),
            filter.clone(), fuzz.clone(), thump_amt.clone(), thump_trigger.clone(),
            velocity.clone(), acceleration.clone(), voice_params.clone(),
        );
        let mut post_nam = build_post_nam(&filter, voice_params.clone());

        println!("║ Loading NAM models... ");
        let (nam_model_list, nam_model_name_list) = nam::load_nam_models()?;
        let nam_model_names = Arc::new(nam_model_name_list);
        let default_nam_index = nam::default_model_index(&nam_model_names);
        let nam_selected = shared(default_nam_index as f32);
        println!("║ NAM models loaded: {}", nam_model_names.join(", "));

        println!("║ Loading IR files... ");
        let (ir_list, ir_name_list) = nam::load_irs()?;
        let ir_names = Arc::new(ir_name_list);
        let ir_selected = shared(0.0);
        println!("║ IRs loaded: {}", ir_names.join(", "));

        let nam = NamStage::new(nam_model_list, nam_selected.clone(), fuzz.clone(), ir_list, ir_selected.clone());

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        voice_engine.set_sample_rate(config.sample_rate as f64);
        post_nam.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, voice_engine, post_nam, nam, err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, voice_engine, post_nam, nam, err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, voice_engine, post_nam, nam, err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native audio backend OK.");

        Ok(AudioOutput { freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration, nam_selected, nam_model_names, ir_selected, ir_names, voice_params, stream })
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

    fm_voices: Vec<FmVoice>,
    reese_voices: Vec<ReeseVoice>,
    sub:    An<Sine<f64>>,
    bypass_sub: An<Sine<f64>>,
    noise:  Box<dyn AudioUnit>,
    sputter_tri:   An<WaveSynth<U1>>,
    sputter_noise: Box<dyn AudioUnit>,
    envelope: Box<dyn AudioUnit>,

    thump_last_trigger:    f32,
    thump_elapsed_samples: f32,
    sample_rate:           f32,
}

impl VoiceEngine {
    fn new (
        freq: Shared, gate: Shared, bend: Shared, width: Shared,
        filter: Shared, fuzz: Shared, thump_amt: Shared, thump_trigger: Shared,
        velocity: Shared, acceleration: Shared, params: Arc<VoiceParams>,
    ) -> VoiceEngine {
        let rest = SignalState::new();

        // noise_lpf_hz has no live SignalWeights today, so its live value
        // only tracks GUI edits to default/lo/hi/curve -- envelope()
        // re-evaluates that at control rate (~2ms) instead of baking it in
        // once here, same idea as build_post_nam's filter_q/amp.
        let noise_params = params.clone();
        let noise_cutoff_hz = envelope(move |_t: f64| param_factor(&noise_params.noise_lpf_hz, &SignalState::new()) as f64);

        VoiceEngine {
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            fm_voices: FM_OSCS.iter().map(|&frac| FmVoice::new(frac)).collect(),
            reese_voices: REESE_OSCS.iter().map(|&frac| ReeseVoice::new(frac)).collect(),
            sub:    sine(),
            bypass_sub: sine(),
            noise:  Box::new((white() | noise_cutoff_hz) >> lowpass_q(1.0)),
            sputter_tri:   triangle(),
            sputter_noise: Box::new(white()),
            envelope: Box::new(adsr_live(
                param_factor(&params.attack, &rest), 0.0, 1.0,
                param_factor(&params.release, &rest),
            )),
            thump_last_trigger:    0.0,
            thump_elapsed_samples: 0.0,
            sample_rate:           DEFAULT_SR as f32,
            params,
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        for voice in self.fm_voices.iter_mut() { voice.set_sample_rate(sr); }
        for voice in self.reese_voices.iter_mut() { voice.set_sample_rate(sr); }
        self.sub.set_sample_rate(sr);
        self.bypass_sub.set_sample_rate(sr);
        self.noise.set_sample_rate(sr);
        self.sputter_tri.set_sample_rate(sr);
        self.sputter_noise.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        self.sample_rate = sr as f32;
    }

    // `dry` => the full -voice signal for effect chain
    // `bypass` => effect bypass (sub-osc)
    fn tick (&mut self) -> (f32, f32) {
        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            ..SignalState::new()
        };

        let params       = self.params.clone();
        let bend_mult    = 2f32.powf(signal.bend);
        let octave_shift = param_factor(&params.octave_shift, &signal);
        let base_freq    = self.freq.value() * bend_mult * self.tick_thump(&signal) * octave_shift;

        // Same for every voice in the stack -- computed once here rather
        // than inside each voice's tick(), unlike the per-voice `frac`
        // detune baked into each FmVoice/ReeseVoice at construction.
        let mut fm_input: Frame<f32, U7> = Frame::default();
        fm_input[0] = base_freq;
        fm_input[1] = param_factor(&params.ratio_a, &signal);
        fm_input[2] = param_factor(&params.ratio_b, &signal);
        fm_input[3] = param_factor(&params.ratio_c, &signal);
        fm_input[4] = param_factor(&params.index_b, &signal);
        fm_input[5] = param_factor(&params.index_c, &signal);
        fm_input[6] = param_factor(&params.detune_cents_max, &signal);

        let fm_voice_count = self.fm_voices.len() as f32;
        let fm_sum: f32 = self.fm_voices.iter_mut()
            .map(|voice| voice.tick(&fm_input)[0])
            .sum::<f32>() / fm_voice_count;

        let mut reese_input: Frame<f32, U2> = Frame::default();
        reese_input[0] = base_freq;
        reese_input[1] = param_factor(&params.reese_detune_cents_max, &signal);

        let reese_voice_count = self.reese_voices.len() as f32;
        let reese_level = param_factor(&params.reese_level, &signal);
        let reese_sum: f32 = self.reese_voices.iter_mut()
            .map(|voice| voice.tick(&reese_input)[0])
            .sum::<f32>() / reese_voice_count * reese_level;

        let sub_level   = param_factor(&params.sub_level, &signal);
        let noise_level = param_factor(&params.noise_level, &signal);
        let sub   = self.sub.filter_mono(base_freq * 0.5) * sub_level;
        let noise = self.noise.get_mono() * noise_level;

        // Sputter: triangle sub osc at base_freq ring-modulated by white noise --
        // gates the noise on/off with the pitch instead of a fixed-frequency hiss.
        let sputter_level = param_factor(&params.sputter_level, &signal);
        let sputter = self.sputter_tri.filter_mono(base_freq) * self.sputter_noise.get_mono() * sputter_level;

        let dry = fm_sum + reese_sum + sub + noise + sputter;
        let env = self.envelope.filter_mono(self.gate.value());

        let bypass_ratio = param_factor(&params.bypass_sub_ratio, &signal);
        let bypass_level = param_factor(&params.bypass_sub_level, &signal);
        let bypass = self.bypass_sub.filter_mono(base_freq * bypass_ratio) * bypass_level * env;

        (dry * env, bypass)
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

// FX after the NAM stage
// see NamStage::process_buffer -- so this is just filter + amp)
fn build_post_nam (filter: &Shared, params: Arc<VoiceParams>) -> Box<dyn AudioUnit> {
    let cutoff_params = params.clone();
    let cutoff_hz = var(filter) >> map(move |i: &Frame<f32, U1>| {
        let signal = SignalState { filter: i[0], ..SignalState::new() };
        param_factor(&cutoff_params.filter_cutoff_hz, &signal)
    });

    // filter_q and amp have no live SignalWeights today, so their live
    // value only tracks GUI edits to default/lo/hi/curve -- envelope()
    // re-evaluates that at control rate (~2ms) instead of baking it in
    // once here (which needed a process restart to pick up an edit).
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
    mut post_nam: Box<dyn AudioUnit>,
    mut nam: NamStage,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch = [0.0f32; NAM_BLOCK_CAP];
    let mut bypass_scratch = [0.0f32; NAM_BLOCK_CAP];
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

                for i in 0..n {
                    let (dry, bypass) = pre_nam.tick();
                    block[i] = dry;
                    bypass_block[i] = bypass;
                }

                // AudioNode::process() must be driven in <=MAX_BUFFER_SIZE
                // chunks -- see nam.rs.
                for chunk in block.chunks_mut(MAX_BUFFER_SIZE) {
                    nam_input.channel_f32_mut(0)[..chunk.len()].copy_from_slice(chunk);
                    nam.process(chunk.len(), &nam_input.buffer_ref(), &mut nam_output.buffer_mut());
                    chunk.copy_from_slice(&nam_output.channel_f32_mut(0)[..chunk.len()]);
                }

                for (i, &s) in block.iter().enumerate() {
                    // nam-rs's raw output isn't loudness-normalized
                    let filtered = post_nam.filter_mono(s.clamp(-1.0, 1.0));
                    let mixed = (filtered + bypass_block[i]).clamp(-1.0, 1.0);
                    let sample = T::from_sample(mixed);
                    let frame_start = (done + i) * channels;
                    for ch in 0..channels {
                        data[frame_start + ch] = sample;
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
