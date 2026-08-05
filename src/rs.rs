
//
// RS
//
// Native Rust audio backend: a `fundsp` signal graph rendered straight to the
// system's default output device via `cpal` -- no subprocess, no bridge
// language. Named to mirror `sc.rs`/`osc.rs` and the `--rs` flag that selects
// it.
//
// Reproduces sc/main.scd's proof-of-concept patch's ingredients -- one
// persistent saw voice through a resonant lowpass, gated by an ASR envelope
// -- but with a NAM (Neural Amp Modeler) stage spliced in between the
// envelope and the filter (build_pre_nam -> NamStage -> build_post_nam), amp
// distortion before tone-shaping, same order a real pedal/amp-then-tonestack
// chain would use. Control values (frequency, gate, filter cutoff) live in
// `fundsp::Shared` atomic cells, which is fundsp's own idiom for driving a
// running audio graph from another thread with no locks/allocation in the
// audio callback -- filling the same role `ScOutput`'s stdin messages play
// for its separate sclang process, just in-process.
//

use std::io;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;
use nam_rs::{Model, NamModel};

use crate::output::DeltaConsumer;
use crate::tools::linexp;
use crate::zgicabra::{DeltaEvent, SignalState};

// Matches sc/main.scd's SynthDef(\zgicabraSaw): Env.asr(0.01, 1, 0.3), amp=0.3.
const ATTACK:   f32 = 0.01;
const RELEASE:  f32 = 0.3;
const AMP:      f32 = 0.3;

// RLPF's resonance arg (0.3) isn't a directly portable number -- fundsp's
// lowpass_q takes a genuine Q rather than SC's reciprocal-of-Q convention --
// so this is a by-ear equivalent, same treatment exp3.md gives every filter
// swap that isn't a literal port.
const FILTER_Q: f32 = 0.7;

// adsr_live's gate convention (see fundsp's own adsr.rs): control > 0 starts
// the attack, control <= 0 starts the release. Any non-positive value works;
// -1.0 matches the convention fundsp's own live_adsr.rs example uses.
const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

// Every model shipped in nam/ is an A2 (SlimmableContainer) capture at this
// rate (confirmed by inspecting each file's own `sample_rate` field).
// nam-rs does not resample -- feeding it audio at any other rate produces
// silently wrong output (its own crate docs' words), so the output stream is
// requested at this exact rate rather than trusting the device's default.
const NAM_SAMPLE_RATE: u32 = 48_000;

// Indexed by Voice's own discriminant (Classic=0, Eternal=1, VoiceC=2,
// VoiceD=3) -- see zgicabra::Voice and this module's handle_event. VoiceD's
// `None` is deliberate: bypass, so the pure synth signal stays available to
// A/B against every model.
const NAM_MODEL_PATHS: [Option<&str>; 4] = [
    Some("nam/comp-50.nam"), // Classic
    Some("nam/petrucci.nam"), // Eternal
    Some("nam/plexi.nam"),   // VoiceC
    None,                    // VoiceD -- bypass
];

pub struct RsOutput {
    freq:         Shared,
    gate:         Shared,
    cutoff:       Shared,
    nam_selected: Shared, // Voice discriminant as f32; read by NamStage on the audio thread
    stream:       cpal::Stream, // kept alive to keep audio playing; dropping RsOutput stops it
}

impl RsOutput {
    pub fn new () -> io::Result<RsOutput> {
        println!("║ Starting native Rust audio backend... ");

        let freq   = shared(110.0);
        let gate   = shared(GATE_OFF);
        let cutoff = shared(4000.0);

        let mut pre_nam  = build_pre_nam(&freq, &gate);
        let mut post_nam = build_post_nam(&cutoff);

        // NAM stage: all four Voice slots loaded up front (VoiceD stays
        // bypass), so switching mid-stream on the audio thread is just an
        // index write -- no allocation/IO/locks there. See handle_event's
        // VoiceChange arm, which is what drives nam_selected.
        println!("║ Loading NAM models... ");
        let nam_selected = shared(0.0); // Voice::Classic, matching Zgicabra::new()'s default
        let nam = NamStage { models: load_nam_models()?, selected: nam_selected.clone() };
        println!("║ NAM models loaded.");

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        pre_nam.set_sample_rate(config.sample_rate as f64);
        post_nam.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, pre_nam, post_nam, nam, err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, pre_nam, post_nam, nam, err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, pre_nam, post_nam, nam, err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native Rust audio backend OK.");

        Ok(RsOutput { freq, gate, cutoff, nam_selected, stream })
    }
}

// Picks an output config at exactly `target_rate` if the device supports it
// (see NAM_SAMPLE_RATE's comment for why this matters); falls back to the
// device default -- audibly wrong NAM output, but a working stream -- with a
// loud warning rather than failing outright, since the pure fundsp voice
// (VoiceD/bypass) doesn't care what rate it runs at.
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

// Loads every path in NAM_MODEL_PATHS up front. A missing/corrupt file fails
// the whole backend rather than silently falling back to bypass for that
// slot -- if a model was supposed to be there, better to know immediately.
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

// The NAM stage itself: an array of pre-built models (one per Voice slot,
// VoiceD's `None` standing in for bypass) plus a live index into it.
// Deliberately not a fundsp AudioUnit -- nam_rs::Model doesn't implement
// Clone, which AudioUnit's DynClone bound requires, so it's simpler and just
// as correct to run it as a plain post-processing step in the cpal callback
// after the voice graph, rather than force it into the graph-combinator
// type system.
struct NamStage {
    models:   [Option<Model>; 4],
    selected: Shared,
}

impl NamStage {
    // Runs the selected model's own block kernel over `block` in place (a
    // no-op if VoiceD/bypass is selected). Processing a whole block at once
    // rather than one sample at a time is what lets WaveNet use its
    // "cache-friendly block kernel" (nam-rs's own docs) instead of the much
    // more expensive one-sample path -- calling this per sample was cheap
    // enough to *compile* but not fast enough to keep up with real-time audio,
    // which is what caused the audio thread to fall behind and produce the
    // glitching/blips-then-fade symptom.
    fn process_buffer (&mut self, block: &mut [f32]) {
        if let Some(model) = self.models.get_mut(self.selected.value() as usize).and_then(Option::as_mut) {
            model.process_buffer(block);
        }
        // VoiceD/bypass (or an out-of-range index): leave `block` untouched.
    }
}

// Oscillator * envelope -- the raw excited tone NAM receives, matching
// sc/main.scd's Saw.ar gated by Env.asr(0.01, 1, 0.3) before any filtering.
// Split from the filter/amp stage below so NAM can sit between them (amp
// distortion before tone-shaping, the conventional pedal/amp-then-tonestack
// order) rather than after the whole voice like a bolted-on final effect.
fn build_pre_nam (freq: &Shared, gate: &Shared) -> Box<dyn AudioUnit> {
    let osc = var(freq) >> saw();
    let env = var(gate) >> adsr_live(ATTACK, 0.0, 1.0, RELEASE);
    Box::new(osc * env)
}

// Resonant lowpass -> amp, matching sc/main.scd's RLPF.ar -> (* amp). Takes
// NAM's output as its single input (`pass()` forwards it straight through
// the stack into lowpass_q's audio input) rather than generating its own
// signal, unlike build_pre_nam above.
fn build_post_nam (cutoff: &Shared) -> Box<dyn AudioUnit> {
    let filtered = (pass() | var(cutoff)) >> lowpass_q(FILTER_Q);
    Box::new(filtered * AMP)
}

// Generous upper bound on how many frames any single cpal callback will ever
// ask for (real device callbacks are typically a few hundred). Processing in
// chunks of at most this many samples lets the whole audio callback stay
// allocation-free -- one fixed scratch buffer, reused every call, rather than
// sizing a Vec to the callback's own (variable) length each time.
const NAM_BLOCK_CAP: usize = 4096;

fn build_stream<T> (
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut pre_nam: Box<dyn AudioUnit>,
    mut post_nam: Box<dyn AudioUnit>,
    mut nam: NamStage,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut scratch = [0.0f32; NAM_BLOCK_CAP];

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // The voice is genuinely mono throughout (sc/main.scd's
            // Saw.ar(freq!2) is two identical channels, not independent
            // ones), so NAM and the filter only need to run once per sample
            // -- running them separately per output channel would interleave
            // two copies of the same signal through NAM's stateful model and
            // corrupt its recurrent/dilated state for no benefit.
            let frames = data.len() / channels;
            let mut done = 0;

            while done < frames {
                let n = std::cmp::min(frames - done, NAM_BLOCK_CAP);
                let block = &mut scratch[..n];

                for s in block.iter_mut() { *s = pre_nam.get_mono(); }

                nam.process_buffer(block);

                for (i, &s) in block.iter().enumerate() {
                    // nam-rs's docs are explicit that a model's raw output
                    // isn't loudness-normalized (the reference plugin's DC
                    // blocker/normalization is the host's job, not the
                    // model's) -- clamp as a safety net against digital
                    // clipping/wraparound before it reaches the filter, in
                    // case a model's output pushes past -1..1.
                    let filtered = post_nam.filter_mono(s.clamp(-1.0, 1.0));
                    let sample = T::from_sample(filtered);
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
        // Matches sc/main.scd's ~setFilter: val.linexp(0, 1, 100, 8000).
        self.cutoff.set_value(linexp(0.0, 1.0, 100.0, 8000.0, signal.filter));
    }

    fn handle_event (&mut self, delta: &DeltaEvent) {
        match delta {
            DeltaEvent::NoteStart(note) => {
                self.freq.set_value(midi_hz(*note as f32));
                self.gate.set_value(GATE_ON);
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
