
//
// Audio Engine
//

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

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
const NAM_DIR: &str = "nam";


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
    glide_time:       ParamSpec,
    octave_shift:     ParamSpec,
    thump_decay_sec:  ParamSpec,
    thump_pitch_mult: ParamSpec,
}

impl VoiceParams {
    // `filter_q`, `attack`, `release`, `glide_time`, `noise_lpf_hz` and `amp` are
    // baked into fundsp AudioUnits at graph-construction time (lowpass_q's Q,
    // adsr_live's times, follow's response time, lowpass_hz's cutoff, and the
    // `*` amp multiply all take a fixed value once, not a live per-sample
    // input) -- so editing them here only takes effect on the next
    // RsOutput::new() (process restart), not live. Making them truly live
    // would mean forking adsr_live's closure or switching to fundsp's
    // 3-input lowpass().
    pub fn new () -> VoiceParams {
        VoiceParams {
            attack:           ParamSpec::new("attack",           0.01,   (0.001, 1.0),     Curve::Linear, SignalWeights::NONE),
            release:          ParamSpec::new("release",          0.25,   (0.01, 2.0),      Curve::Linear, SignalWeights::NONE),
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
            glide_time:       ParamSpec::new("glide_time",       0.08,   (0.0, 0.5),       Curve::Linear, SignalWeights::NONE),
            octave_shift:     ParamSpec::new("octave_shift",     0.5,    (0.25, 2.0),      Curve::Linear, SignalWeights::NONE),
            thump_decay_sec:  ParamSpec::new("thump_decay_sec",  0.18,   (0.02, 1.0),      Curve::Linear, SignalWeights::NONE),
            // range must start at exactly 0.0 -- weight=1.0 reproduces today's `thump * THUMP_PITCH_MULT`
            thump_pitch_mult: ParamSpec::new("thump_pitch_mult", 1.5,    (0.0, 1.5),       Curve::Linear, SignalWeights { thump: 1.0, ..SignalWeights::NONE }),
        }
    }

    // All params in a stable display order, for building the UI grid.
    pub fn entries (&self) -> [&ParamSpec; 20] {
        [
            &self.attack, &self.release, &self.amp, &self.filter_q, &self.filter_cutoff_hz,
            &self.ratio_a, &self.ratio_b, &self.ratio_c, &self.index_b, &self.index_c,
            &self.detune_cents_max, &self.sub_level, &self.noise_level, &self.noise_lpf_hz,
            &self.bypass_sub_ratio, &self.bypass_sub_level, &self.glide_time, &self.octave_shift,
            &self.thump_decay_sec, &self.thump_pitch_mult,
        ]
    }
}

// A handle for cycling through the discovered NAM models (index 0 is always
// "Bypass" -- no model, dry passthrough) and reading the current selection's
// name. Independent of Zgicabra's Voice enum: something else (currently
// gui.rs's Model cycler buttons) drives `selected` directly.
#[derive(Clone)]
pub struct NamModelCycler {
    selected: Shared,
    names:    Arc<Vec<String>>,
}

impl NamModelCycler {
    pub fn selected_name (&self) -> &str {
        let i = self.selected.value() as usize;
        self.names.get(i).map(String::as_str).unwrap_or("?")
    }

    pub fn cycle (&self, delta: i32) {
        let count = self.names.len() as i32;
        if count == 0 { return; }
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }
}

pub struct RsOutput {
    freq:            Shared,
    gate:            Shared,
    bend:            Shared,
    width:           Shared,
    filter:          Shared,
    fuzz:            Shared,
    thump_amt:       Shared,
    thump_trigger:   Shared,
    nam_selected:    Shared,
    nam_model_names: Arc<Vec<String>>,
    voice_params:    Arc<VoiceParams>,
    stream:          cpal::Stream,
}

impl RsOutput {
    // Handle to the live parameter matrix, for a UI to read/write. Cheap to
    // clone (each field is an Arc'd atomic cell); the audio thread reads the
    // same cells lock-free every sample.
    pub fn voice_params (&self) -> Arc<VoiceParams> {
        self.voice_params.clone()
    }

    // Handle to the NAM model cycler, for a UI to drive/display. Cheap to
    // clone (an Arc'd atomic cell plus an Arc'd name list).
    pub fn nam_models (&self) -> NamModelCycler {
        NamModelCycler { selected: self.nam_selected.clone(), names: self.nam_model_names.clone() }
    }

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
        let voice_params  = Arc::new(VoiceParams::new());

        let mut voice_engine = VoiceEngine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(),
            thump_amt.clone(), thump_trigger.clone(), voice_params.clone(),
        );
        let mut post_nam = build_post_nam(&filter, voice_params.clone());

        println!("║ Loading NAM models... ");
        let (nam_model_list, nam_model_name_list) = load_nam_models()?;
        let nam_model_names = Arc::new(nam_model_name_list);
        let nam_selected = shared(0.0);
        let nam = NamStage {
            models:      nam_model_list,
            selected:    nam_selected.clone(),
            fuzz:        fuzz.clone(),
            dry_scratch: [0.0; NAM_BLOCK_CAP],
        };
        println!("║ NAM models loaded: {}", nam_model_names.join(", "));

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

        Ok(RsOutput { freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, nam_selected, nam_model_names, voice_params, stream })
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

// Discovers every *.nam file in NAM_DIR (sorted for a stable, predictable
// cycle order) and loads each one. Index 0 is always a "Bypass" slot (no
// model, dry passthrough) so the cycler always has a way back to clean.
fn load_nam_models () -> io::Result<(Vec<Option<Model>>, Vec<String>)> {
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(NAM_DIR)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to read NAM model directory '{NAM_DIR}': {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("nam")))
        .collect();
    paths.sort();

    let mut models: Vec<Option<Model>> = vec![None];
    let mut names:  Vec<String>        = vec!["Bypass".to_string()];

    for path in paths {
        let path_str = path.to_string_lossy().into_owned();

        let nam_model = NamModel::from_file(&path_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load NAM model '{path_str}': {e}")))?;
        let model = Model::from_nam(&nam_model)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build NAM model '{path_str}': {e}")))?;

        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or(&path_str).to_string();

        models.push(Some(model));
        names.push(name);
    }

    Ok((models, names))
}

struct NamStage {
    models:      Vec<Option<Model>>,
    selected:    Shared,
    fuzz:        Shared,
    dry_scratch: [f32; NAM_BLOCK_CAP],
}

impl NamStage {
    // NAM plays the role of the fuzz/distortion stage: signal_state.fuzz is
    // the dry/wet blend against the selected model's output, rather than a
    // separate waveshaper after the NAM stage.
    fn process_buffer (&mut self, block: &mut [f32]) {
        let Some(model) = self.models.get_mut(self.selected.value() as usize).and_then(Option::as_mut) else {
            return; // VoiceD/bypass (or an out-of-range index): leave `block` untouched.
        };

        let fuzz = self.fuzz.value().clamp(0.0, 1.0);
        if fuzz <= 0.0 { return; } // fully dry: skip the model entirely

        self.dry_scratch[..block.len()].copy_from_slice(block);
        model.process_buffer(block);

        for (i, wet) in block.iter_mut().enumerate() {
            *wet = self.dry_scratch[i] * (1.0 - fuzz) + *wet * fuzz;
        }
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

    fn tick (&mut self, base_freq: f32, signal: &SignalState, params: &VoiceParams) -> f32 {
        let detune_cents_max = param_factor(&params.detune_cents_max, signal);
        let detune = 2f32.powf(self.frac * detune_cents_max / 1200.0);
        let voice_freq = base_freq * detune;

        let ratio_c = param_factor(&params.ratio_c, signal);
        let freq_c = voice_freq * ratio_c;
        let out_c = self.op_c.filter_mono(freq_c);

        let ratio_b = param_factor(&params.ratio_b, signal);
        let index_c = param_factor(&params.index_c, signal);
        let freq_b = voice_freq * ratio_b;
        let out_b = self.op_b.filter_mono(freq_b + out_c * (index_c * freq_c));

        let ratio_a = param_factor(&params.ratio_a, signal);
        let index_b = param_factor(&params.index_b, signal);
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
    params:        Arc<VoiceParams>,

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
        thump_amt: Shared, thump_trigger: Shared, params: Arc<VoiceParams>,
    ) -> VoiceEngine {
        let rest = SignalState::new();
        VoiceEngine {
            freq, gate, bend, width, thump_amt, thump_trigger,
            voices: FRACS.iter().map(|&frac| FmVoice::new(frac)).collect(),
            sub:    sine(),
            bypass_sub: sine(),
            noise:  Box::new(white() >> lowpass_hz(param_factor(&params.noise_lpf_hz, &rest), 1.0)),
            glide:  follow(param_factor(&params.glide_time, &rest)),
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

        let params       = self.params.clone();
        let glided       = self.glide.filter_mono(self.freq.value());
        let bend_mult    = 2f32.powf(signal.bend);
        let octave_shift = param_factor(&params.octave_shift, &signal);
        let base_freq    = glided * bend_mult * self.tick_thump(&signal) * octave_shift;

        let voice_count = self.voices.len() as f32;
        let fm_sum: f32 = self.voices.iter_mut()
            .map(|voice| voice.tick(base_freq, &signal, &params))
            .sum::<f32>() / voice_count;

        let sub_level   = param_factor(&params.sub_level, &signal);
        let noise_level = param_factor(&params.noise_level, &signal);
        let sub   = self.sub.filter_mono(base_freq * 0.5) * sub_level;
        let noise = self.noise.get_mono() * noise_level;

        let dry = fm_sum + sub + noise;
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

    let filter_q = param_factor(&params.filter_q, &SignalState::new());
    let filtered = (pass() | cutoff_hz) >> lowpass_q(filter_q);

    let amp = param_factor(&params.amp, &SignalState::new());
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
            // VoiceChange is intentionally unhandled here: NAM model
            // selection is decoupled from it now -- see NamModelCycler.
            _ => {},
        }
    }
}
