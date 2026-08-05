
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

const ATTACK:  f32 = 0.01;
const RELEASE: f32 = 0.25;
const AMP:     f32 = 0.3;

const FILTER_Q: f32 = 0.6;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const RATIO_A: f32 = 1.0;
const RATIO_B: f32 = 1.007;
const RATIO_C: f32 = 2.003;
const INDEX_B: f32 = 2.2;
const INDEX_C: f32 = 3.5;

const FRACS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];
const DETUNE_CENTS_MAX: f32 = 25.0;

const SUB_LEVEL:    f32 = 0.35;
const NOISE_LEVEL:  f32 = 0.05;
const NOISE_LPF_HZ: f32 = 4000.0;

const BYPASS_SUB_RATIO: f32 = 0.5;
const BYPASS_SUB_LEVEL: f32 = 0.35;

const GLIDE_TIME: f32 = 0.08;

const OCTAVE_SHIFT: f32 = 0.5;

const THUMP_DECAY_SEC:  f32 = 0.18;
const THUMP_PITCH_MULT: f32 = 1.5;

const FUZZ_DRIVE: f32 = 8.0;

const NAM_SAMPLE_RATE: u32 = 48_000;

const NAM_MODEL_PATHS: [Option<&str>; 4] = [
    Some("nam/comp-50.nam"), // Classic
    Some("nam/petrucci.nam"), // Eternal
    Some("nam/plexi.nam"),   // VoiceC
    None,                    // VoiceD -- bypass
];

pub struct RsOutput {
    freq:          Shared,
    gate:          Shared,
    bend:          Shared,
    width:         Shared,
    cutoff:        Shared,
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
        let cutoff        = shared(4000.0);
        let fuzz          = shared(0.0);
        let thump_amt     = shared(0.0);
        let thump_trigger = shared(0.0);

        let mut voice_engine = VoiceEngine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(),
            thump_amt.clone(), thump_trigger.clone(),
        );
        let mut post_nam = build_post_nam(&cutoff, &fuzz);

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

        Ok(RsOutput { freq, gate, bend, width, cutoff, fuzz, thump_amt, thump_trigger, nam_selected, stream })
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

    fn tick (&mut self, base_freq: f32, width: f32) -> f32 {
        let detune = 2f32.powf(self.frac * width * DETUNE_CENTS_MAX / 1200.0);
        let voice_freq = base_freq * detune;

        let freq_c = voice_freq * RATIO_C;
        let out_c = self.op_c.filter_mono(freq_c);

        let freq_b = voice_freq * RATIO_B;
        let out_b = self.op_b.filter_mono(freq_b + out_c * (INDEX_C * freq_c));

        let freq_a = voice_freq * RATIO_A;
        let out_a = self.op_a.filter_mono(freq_a + out_b * (INDEX_B * freq_b));

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
        VoiceEngine {
            freq, gate, bend, width, thump_amt, thump_trigger,
            voices: FRACS.iter().map(|&frac| FmVoice::new(frac)).collect(),
            sub:    sine(),
            bypass_sub: sine(),
            noise:  Box::new(white() >> lowpass_hz(NOISE_LPF_HZ, 1.0)),
            glide:  follow(GLIDE_TIME),
            envelope: Box::new(adsr_live(ATTACK, 0.0, 1.0, RELEASE)),
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
        let glided    = self.glide.filter_mono(self.freq.value());
        let bend_mult = 2f32.powf(self.bend.value());
        let base_freq = glided * bend_mult * self.tick_thump() * OCTAVE_SHIFT;

        let width = self.width.value();
        let voice_count = self.voices.len() as f32;
        let fm_sum: f32 = self.voices.iter_mut()
            .map(|voice| voice.tick(base_freq, width))
            .sum::<f32>() / voice_count;

        let sub   = self.sub.filter_mono(base_freq * 0.5) * SUB_LEVEL;
        let noise = self.noise.get_mono() * NOISE_LEVEL;

        let dry = fm_sum + sub + noise;
        let env = self.envelope.filter_mono(self.gate.value());

        let bypass = self.bypass_sub.filter_mono(base_freq * BYPASS_SUB_RATIO) * BYPASS_SUB_LEVEL * env;

        (dry * env, bypass)
    }

    fn tick_thump (&mut self) -> f32 {
        let trigger = self.thump_trigger.value();
        if trigger != self.thump_last_trigger {
            self.thump_last_trigger = trigger;
            self.thump_elapsed_samples = 0.0;
        }

        let t = self.thump_elapsed_samples / self.sample_rate;
        self.thump_elapsed_samples += 1.0;

        let decay = (-5.0 * t / THUMP_DECAY_SEC).exp();
        1.0 + decay * self.thump_amt.value() * THUMP_PITCH_MULT
    }
}

// FX after the NAM stage
fn build_post_nam (cutoff: &Shared, fuzz: &Shared) -> Box<dyn AudioUnit> {
    let filtered = (pass() | var(cutoff)) >> lowpass_q(FILTER_Q);

    let with_fuzz = (filtered | var(fuzz)) >> map(|i: &Frame<f32, U2>| {
        let (x, f) = (i[0], i[1]);
        let wet = (x * (1.0 + f * FUZZ_DRIVE)).tanh();
        x * (1.0 - f) + wet * f
    });

    Box::new(with_fuzz * AMP)
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
        self.fuzz.set_value(signal.fuzz);
        self.thump_amt.set_value(signal.thump);
        self.cutoff.set_value(linexp(0.0, 1.0, 100.0, 14000.0, signal.filter));
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
