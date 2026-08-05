
//
// Audio Engine
//

use std::io;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;
use nam_rs::{Model, NamModel};

use crate::output::DeltaConsumer;
use crate::tools::linexp;
use crate::zgicabra::{DeltaEvent, SignalState};

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const FRACS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];

const NAM_SAMPLE_RATE: u32 = 48_000;

const NAM_MODEL_PATHS: [Option<&str>; 4] = [
    Some("nam/comp-50.nam"), // Classic
    Some("nam/petrucci.nam"), // Eternal
    Some("nam/plexi.nam"),   // VoiceC
    None,                    // VoiceD -- bypass
];

// --
// Parameter matrix: every tunable value in the engine gets a default, a
// valid range, an interpolation curve, and a weight against each live
// signal. param_factor() is the single place any of that gets resolved --
// call sites never touch a raw const again.
// --

#[derive(Clone, Copy)]
enum Curve { Linear, Exp }

// pitch, left_vel, right_vel: natural future additions here once SignalState
// grows matching fields -- nothing to weight against yet, so left out.
#[derive(Clone, Copy)]
struct SignalWeights {
    width:  f32,
    filter: f32,
    fuzz:   f32,
    thump:  f32,
}

impl SignalWeights {
    const NONE: SignalWeights = SignalWeights { width: 0.0, filter: 0.0, fuzz: 0.0, thump: 0.0 };
}

#[derive(Clone, Copy)]
struct ParamSpec {
    default: f32,
    range:   (f32, f32),
    curve:   Curve,
    weights: SignalWeights,
}

// Blends a param's default toward its range endpoints, weighted by how much
// each live signal should influence it. All-zero weights => always `default`,
// which is exactly today's plain-const behaviour.
fn param_factor (spec: &ParamSpec, signal: &SignalState) -> f32 {
    let (lo, hi) = spec.range;
    let interp = |t: f32| match spec.curve {
        Curve::Linear => lo + (hi - lo) * t,
        Curve::Exp    => linexp(0.0, 1.0, lo, hi, t),
    };

    let mut result = spec.default;
    result += spec.weights.width  * (interp(signal.width)  - spec.default);
    result += spec.weights.filter * (interp(signal.filter) - spec.default);
    result += spec.weights.fuzz   * (interp(signal.fuzz)   - spec.default);
    result += spec.weights.thump  * (interp(signal.thump)  - spec.default);

    result.clamp(lo.min(hi), lo.max(hi))
}

struct VoiceParams {
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
    glide_time:       ParamSpec,
    octave_shift:     ParamSpec,
    thump_decay_sec:  ParamSpec,
    thump_pitch_mult: ParamSpec,
    fuzz_drive:       ParamSpec,
}

// `filter_q`, `attack`, `release`, `glide_time`, `noise_lpf_hz` and `amp` are
// baked into fundsp AudioUnits at graph-construction time (lowpass_q's Q,
// adsr_live's times, follow's response time, lowpass_hz's cutoff, and the
// `*` amp multiply all take a fixed value once, not a live per-sample
// input) -- so they're only ever evaluated with SignalState::new() (rest
// state). That's provably identical to live evaluation as long as their
// weights stay SignalWeights::NONE. Making them truly live would mean
// forking adsr_live's closure or switching to fundsp's 3-input lowpass().
const VOICE_PARAMS: VoiceParams = VoiceParams {
    attack:           ParamSpec { default: 0.01,   range: (0.001, 1.0),     curve: Curve::Linear, weights: SignalWeights::NONE },
    release:          ParamSpec { default: 0.25,   range: (0.01, 2.0),      curve: Curve::Linear, weights: SignalWeights::NONE },
    amp:              ParamSpec { default: 0.3,    range: (0.0, 1.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    filter_q:         ParamSpec { default: 0.6,    range: (0.1, 4.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    // range must start at exactly 100.0 -- with weight=1.0 `default` fully cancels (see param_factor)
    filter_cutoff_hz: ParamSpec { default: 100.0,  range: (100.0, 14000.0), curve: Curve::Exp,    weights: SignalWeights { filter: 1.0, ..SignalWeights::NONE } },
    ratio_a:          ParamSpec { default: 1.0,    range: (0.5, 2.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    ratio_b:          ParamSpec { default: 1.007,  range: (0.5, 2.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    ratio_c:          ParamSpec { default: 2.003,  range: (0.5, 4.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    index_b:          ParamSpec { default: 2.2,    range: (0.0, 8.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    index_c:          ParamSpec { default: 3.5,    range: (0.0, 8.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    // range must start at exactly 0.0 -- weight=1.0 reproduces today's `width * DETUNE_CENTS_MAX`
    detune_cents_max: ParamSpec { default: 25.0,   range: (0.0, 25.0),      curve: Curve::Linear, weights: SignalWeights { width: 1.0, ..SignalWeights::NONE } },
    sub_level:        ParamSpec { default: 0.35,   range: (0.0, 1.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    noise_level:      ParamSpec { default: 0.05,   range: (0.0, 0.5),       curve: Curve::Linear, weights: SignalWeights::NONE },
    noise_lpf_hz:     ParamSpec { default: 4000.0, range: (200.0, 12000.0), curve: Curve::Linear, weights: SignalWeights::NONE },
    bypass_sub_ratio: ParamSpec { default: 0.5,    range: (0.25, 1.0),      curve: Curve::Linear, weights: SignalWeights::NONE },
    bypass_sub_level: ParamSpec { default: 0.35,   range: (0.0, 1.0),       curve: Curve::Linear, weights: SignalWeights::NONE },
    glide_time:       ParamSpec { default: 0.08,   range: (0.0, 0.5),       curve: Curve::Linear, weights: SignalWeights::NONE },
    octave_shift:     ParamSpec { default: 0.5,    range: (0.25, 2.0),      curve: Curve::Linear, weights: SignalWeights::NONE },
    thump_decay_sec:  ParamSpec { default: 0.18,   range: (0.02, 1.0),      curve: Curve::Linear, weights: SignalWeights::NONE },
    // range must start at exactly 0.0 -- weight=1.0 reproduces today's `thump * THUMP_PITCH_MULT`
    thump_pitch_mult: ParamSpec { default: 1.5,    range: (0.0, 1.5),       curve: Curve::Linear, weights: SignalWeights { thump: 1.0, ..SignalWeights::NONE } },
    // range must start at exactly 0.0 -- weight=1.0 reproduces today's `fuzz * FUZZ_DRIVE`
    fuzz_drive:       ParamSpec { default: 8.0,    range: (0.0, 8.0),       curve: Curve::Linear, weights: SignalWeights { fuzz: 1.0, ..SignalWeights::NONE } },
};

pub struct RsOutput {
    freq:          Shared,
    gate:          Shared,
    bend:          Shared,
    width:         Shared,
    filter:        Shared,
    fuzz:          Shared,
    thump_amt:     Shared,
    thump_trigger: Shared,
    nam_selected:  Shared,
    stream:        cpal::Stream,
}

impl RsOutput {
    pub fn new () -> io::Result<RsOutput> {
        println!("║ Starting native Rust audio backend... ");

        let freq          = shared(110.0);
        let gate          = shared(GATE_OFF);
        let bend          = shared(0.0);
        let width         = shared(0.0);
        let filter        = shared(0.0);
        let fuzz          = shared(0.0);
        let thump_amt     = shared(0.0);
        let thump_trigger = shared(0.0);

        let mut voice_engine = VoiceEngine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(),
            thump_amt.clone(), thump_trigger.clone(),
        );
        let mut post_nam = build_post_nam(&filter, &fuzz);

        println!("║ Loading NAM models... ");
        let nam_selected = shared(0.0);
        let nam = NamStage { models: load_nam_models()?, selected: nam_selected.clone() };
        println!("║ NAM models loaded.");

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

        println!("║ Native Rust audio backend OK.");

        Ok(RsOutput { freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, nam_selected, stream })
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

fn load_nam_models () -> io::Result<[Option<Model>; 4]> {
    let mut models: [Option<Model>; 4] = [None, None, None, None];

    for (i, path) in NAM_MODEL_PATHS.iter().enumerate() {
        let Some(path) = path else { continue };

        let nam_model = NamModel::from_file(path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load NAM model '{path}': {e}")))?;
        let model = Model::from_nam(&nam_model)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build NAM model '{path}': {e}")))?;

        models[i] = Some(model);
    }

    Ok(models)
}

struct NamStage {
    models:   [Option<Model>; 4],
    selected: Shared,
}

impl NamStage {
    fn process_buffer (&mut self, block: &mut [f32]) {
        if let Some(model) = self.models.get_mut(self.selected.value() as usize).and_then(Option::as_mut) {
            model.process_buffer(block);
        }
        // VoiceD/bypass (or an out-of-range index): leave `block` untouched.
    }
}
struct FmVoice {
    op_a: An<Sine<f64>>,
    op_b: An<Sine<f64>>,
    op_c: An<Sine<f64>>,
    frac: f32,
}

impl FmVoice {
    fn new (frac: f32) -> FmVoice {
        FmVoice { op_a: sine(), op_b: sine(), op_c: sine(), frac }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        self.op_a.set_sample_rate(sr);
        self.op_b.set_sample_rate(sr);
        self.op_c.set_sample_rate(sr);
    }

    fn tick (&mut self, base_freq: f32, signal: &SignalState) -> f32 {
        let detune_cents_max = param_factor(&VOICE_PARAMS.detune_cents_max, signal);
        let detune = 2f32.powf(self.frac * detune_cents_max / 1200.0);
        let voice_freq = base_freq * detune;

        let ratio_c = param_factor(&VOICE_PARAMS.ratio_c, signal);
        let freq_c = voice_freq * ratio_c;
        let out_c = self.op_c.filter_mono(freq_c);

        let ratio_b = param_factor(&VOICE_PARAMS.ratio_b, signal);
        let index_c = param_factor(&VOICE_PARAMS.index_c, signal);
        let freq_b = voice_freq * ratio_b;
        let out_b = self.op_b.filter_mono(freq_b + out_c * (index_c * freq_c));

        let ratio_a = param_factor(&VOICE_PARAMS.ratio_a, signal);
        let index_b = param_factor(&VOICE_PARAMS.index_b, signal);
        let freq_a = voice_freq * ratio_a;
        let out_a = self.op_a.filter_mono(freq_a + out_b * (index_b * freq_b));

        out_a
    }
}

struct VoiceEngine {
    freq:          Shared,
    gate:          Shared,
    bend:          Shared,
    width:         Shared,
    thump_amt:     Shared,
    thump_trigger: Shared,

    voices: Vec<FmVoice>,
    sub:    An<Sine<f64>>,
    bypass_sub: An<Sine<f64>>,
    noise:  Box<dyn AudioUnit>,
    glide:  An<Follow<f64>>,
    envelope: Box<dyn AudioUnit>,

    thump_last_trigger:    f32,
    thump_elapsed_samples: f32,
    sample_rate:           f32,
}

impl VoiceEngine {
    fn new (
        freq: Shared, gate: Shared, bend: Shared, width: Shared,
        thump_amt: Shared, thump_trigger: Shared,
    ) -> VoiceEngine {
        let rest = SignalState::new();
        VoiceEngine {
            freq, gate, bend, width, thump_amt, thump_trigger,
            voices: FRACS.iter().map(|&frac| FmVoice::new(frac)).collect(),
            sub:    sine(),
            bypass_sub: sine(),
            noise:  Box::new(white() >> lowpass_hz(param_factor(&VOICE_PARAMS.noise_lpf_hz, &rest), 1.0)),
            glide:  follow(param_factor(&VOICE_PARAMS.glide_time, &rest)),
            envelope: Box::new(adsr_live(
                param_factor(&VOICE_PARAMS.attack, &rest), 0.0, 1.0,
                param_factor(&VOICE_PARAMS.release, &rest),
            )),
            thump_last_trigger:    0.0,
            thump_elapsed_samples: 0.0,
            sample_rate:           DEFAULT_SR as f32,
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        for voice in self.voices.iter_mut() { voice.set_sample_rate(sr); }
        self.sub.set_sample_rate(sr);
        self.bypass_sub.set_sample_rate(sr);
        self.noise.set_sample_rate(sr);
        self.glide.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        self.sample_rate = sr as f32;
    }

    // `dry` => the full -voice signal for effect chain
    // `bypass` => effect bypass (sub-osc)
    fn tick (&mut self) -> (f32, f32) {
        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            ..SignalState::new()
        };

        let glided       = self.glide.filter_mono(self.freq.value());
        let bend_mult    = 2f32.powf(signal.bend);
        let octave_shift = param_factor(&VOICE_PARAMS.octave_shift, &signal);
        let base_freq    = glided * bend_mult * self.tick_thump(&signal) * octave_shift;

        let voice_count = self.voices.len() as f32;
        let fm_sum: f32 = self.voices.iter_mut()
            .map(|voice| voice.tick(base_freq, &signal))
            .sum::<f32>() / voice_count;

        let sub_level   = param_factor(&VOICE_PARAMS.sub_level, &signal);
        let noise_level = param_factor(&VOICE_PARAMS.noise_level, &signal);
        let sub   = self.sub.filter_mono(base_freq * 0.5) * sub_level;
        let noise = self.noise.get_mono() * noise_level;

        let dry = fm_sum + sub + noise;
        let env = self.envelope.filter_mono(self.gate.value());

        let bypass_ratio = param_factor(&VOICE_PARAMS.bypass_sub_ratio, &signal);
        let bypass_level = param_factor(&VOICE_PARAMS.bypass_sub_level, &signal);
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

        let decay_sec  = param_factor(&VOICE_PARAMS.thump_decay_sec, signal);
        // already includes the live thump amount (weights.thump = 1.0), i.e. == thump * THUMP_PITCH_MULT
        let pitch_bump = param_factor(&VOICE_PARAMS.thump_pitch_mult, signal);
        let decay = (-5.0 * t / decay_sec).exp();
        1.0 + decay * pitch_bump
    }
}

// FX after the NAM stage
fn build_post_nam (filter: &Shared, fuzz: &Shared) -> Box<dyn AudioUnit> {
    let cutoff_hz = var(filter) >> map(|i: &Frame<f32, U1>| {
        let signal = SignalState { filter: i[0], ..SignalState::new() };
        param_factor(&VOICE_PARAMS.filter_cutoff_hz, &signal)
    });

    let filter_q = param_factor(&VOICE_PARAMS.filter_q, &SignalState::new());
    let filtered = (pass() | cutoff_hz) >> lowpass_q(filter_q);

    let with_fuzz = (filtered | var(fuzz)) >> map(|i: &Frame<f32, U2>| {
        let (x, f) = (i[0], i[1]);
        let signal = SignalState { fuzz: f, ..SignalState::new() };
        // already includes the live fuzz amount (weights.fuzz = 1.0), i.e. == f * FUZZ_DRIVE
        let drive = param_factor(&VOICE_PARAMS.fuzz_drive, &signal);
        let wet = (x * (1.0 + drive)).tanh();
        x * (1.0 - f) + wet * f
    });

    let amp = param_factor(&VOICE_PARAMS.amp, &SignalState::new());
    Box::new(with_fuzz * amp)
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

                nam.process_buffer(block);

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

impl DeltaConsumer for RsOutput {
    fn panic (&mut self) {
        self.gate.set_value(GATE_OFF);
    }

    fn handle_signal (&mut self, signal: &SignalState) {
        self.bend.set_value(signal.bend);
        self.width.set_value(signal.width);
        self.filter.set_value(signal.filter);
        self.fuzz.set_value(signal.fuzz);
        self.thump_amt.set_value(signal.thump);
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
            DeltaEvent::VoiceChange(voice) => self.nam_selected.set_value(*voice as u8 as f32),
            _ => {},
        }
    }
}
