//
// Audio Engine
//

use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::zgicabra::{DeltaEvent, SignalState};

mod nam;
mod stutter;
mod growl;
mod swarm;
mod basic;
mod gen_node;
mod fx_node;
mod reese;
mod fm;
mod filter;
mod reverb;
mod crusher;
mod compressor;
mod voice;
mod cc_input;
pub mod snapshot;

use nam::NAM_BLOCK_CAP;
use reverb::ReverbFx;
use compressor::Compressor;
use voice::Voice;
use growl::GrowlVoice;
use swarm::SwarmVoice;
use reese::ReeseVoice;
use basic::BasicVoice;
use cc_input::CcInput;
pub use growl::GrowlView;
pub use reese::ReeseView;
pub use basic::BasicView;
pub use swarm::SwarmView;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const NAM_SAMPLE_RATE: u32 = 48_000;
const AUDIO_BUFFER_FRAMES: cpal::FrameCount = 1024;

const ENVELOPE_ATTACK:  f32 = 0.003;
const ENVELOPE_RELEASE: f32 = 0.1;

const AMP_MODEL: &str = "lowgain";

const VOICE_NAMES: [&str; 4] = ["Reese", "Growl", "Basic", "Swarm"];

// Debug: captures snippet of cpal output stream to check non-zero output
const CAPTURE_SECONDS: f32 = 0.1;
const AUDIO_ERROR_LOG_CAP: usize = 50;

pub type AudioErrors = Arc<Mutex<Vec<String>>>;


// Taps the raw cpal output stream (mono, left channel) so main.rs's --test
// self-test can confirm audio is actually producing signal.
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

// Direct handle onto the note gate for main.rs's --test self-test, bypassing
// DeltaEvent/hydra entirely. Fights the real controller's freq/gate cells if
// used at the same time as a live note.
#[derive(Clone)]
pub struct TestTone {
    freq: Shared,
    gate: Shared,
}

impl TestTone {
    pub fn start (&self, note: u8) {
        self.freq.set_value(midi_hz(note as f32));
        self.gate.set_value(GATE_ON);
    }

    pub fn stop (&self) {
        self.gate.set_value(GATE_OFF);
    }
}

// Every Shared cell external code (ui.rs) needs to read, bundled once
// so AudioOutput and AudioHandles don't each declare their own copy of the
// same ~20-field list (see mod.rs's old handles()/AudioOutput duplication).
// Per-voice fields are read-only *View types (see growl.rs's GrowlView doc)
// -- writes are audio-thread/MIDI-CC-only now, see voice.rs's module doc.
#[derive(Clone)]
pub struct Handles {
    pub voice_selected: Shared,
    pub voice_a: ReeseView,
    pub voice_b: GrowlView,
    pub voice_c: BasicView,
    pub voice_d: SwarmView,

    // Set (audio thread, build_stream) whenever a live CC edits that voice's
    // params; polled and cleared (main thread, persist_dirty_voices) once
    // per engine-loop tick -- see snapshot.rs's module doc.
    pub voice_dirty: [Arc<AtomicBool>; 4],

    pub main_sub_lvl:  Shared,
    pub dry_sub_lvl:   Shared,
    pub thump_peak:    Shared,
    pub thump_decay:   Shared,

    pub amp_bypass:    Shared,
    pub amp_boost:     Shared,
    pub amp_blend:     Shared,
    pub amp_crossover: Shared,

    pub reverb_bypass: Shared,
    pub reverb_dry:    Shared,
    pub reverb_decay:  Shared,
    pub reverb_damp:   Shared,
    pub reverb_size:   Shared,

    pub limiter_bypass: Shared,
    pub limiter_thresh: Shared,

    pub master_vol: Shared,
}

// Every external-facing handle onto a running AudioOutput, bundled so
// main.rs threads one Option through instead of per-feature. Derefs to
// `Handles` so `audio.voice_a`/`audio.master_vol`/etc. keep working as plain
// field access.
#[derive(Clone)]
pub struct AudioHandles {
    pub test_tone: TestTone,
    pub handles: Handles,
    pub capture: AudioCapture,
    pub errors:  AudioErrors,
}

impl std::ops::Deref for AudioHandles {
    type Target = Handles;
    fn deref (&self) -> &Handles { &self.handles }
}

impl AudioHandles {
    // Call once per main-loop tick (see main.rs::run_engine_loop). Cheap
    // when nothing changed -- each check is a single atomic swap; only a
    // voice a live CC actually touched since the last call hits disk.
    pub fn persist_dirty_voices (&self) {
        let dirty = &self.handles.voice_dirty;

        if dirty[0].swap(false, Ordering::Relaxed) {
            if let Err(e) = snapshot::save_state(VOICE_NAMES[0], &self.handles.voice_a.fields()) {
                eprintln!("║ Failed to persist {} state: {e}", VOICE_NAMES[0]);
            }
        }
        if dirty[1].swap(false, Ordering::Relaxed) {
            if let Err(e) = snapshot::save_state(VOICE_NAMES[1], &self.handles.voice_b.fields()) {
                eprintln!("║ Failed to persist {} state: {e}", VOICE_NAMES[1]);
            }
        }
        if dirty[2].swap(false, Ordering::Relaxed) {
            if let Err(e) = snapshot::save_state(VOICE_NAMES[2], &self.handles.voice_c.fields()) {
                eprintln!("║ Failed to persist {} state: {e}", VOICE_NAMES[2]);
            }
        }
        if dirty[3].swap(false, Ordering::Relaxed) {
            if let Err(e) = snapshot::save_state(VOICE_NAMES[3], &self.handles.voice_d.fields()) {
                eprintln!("║ Failed to persist {} state: {e}", VOICE_NAMES[3]);
            }
        }
    }
}

pub struct AudioOutput {
    level:             Shared,
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

    handles: Handles,

    capture: AudioCapture,
    errors:  AudioErrors,
    stream:  cpal::Stream,
}

impl AudioOutput {
    pub fn handles (&self) -> AudioHandles {
        AudioHandles {
            test_tone: TestTone { freq: self.freq.clone(), gate: self.gate.clone() },
            handles: self.handles.clone(),
            capture: self.capture.clone(),
            errors:  self.errors.clone(),
        }
    }

    pub fn new () -> io::Result<AudioOutput> {
        println!("║ Starting native audio backend... ");

        let level         = shared(1.0);
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

        println!("║ Loading NAM models ... ");
        let (nam_models, nam_names) = nam::load_nam_models()?;
        let nam_names = Arc::new(nam_names);
        println!("║ NAM models loaded ({} found).", nam_names.len().saturating_sub(1));

        // Defaults to Reese (index 0) so a fresh run has an audible voice.
        let voice_selected = shared(0.0);

        let main_sub_lvl  = shared(0.35);
        let dry_sub_lvl   = shared(0.35);
        let thump_peak    = shared(1.5);
        let thump_decay   = shared(0.10);

        let amp_bypass    = shared(0.0);
        let amp_boost     = shared(1.0);
        let amp_blend     = shared(0.0);
        // 0Hz = crossover no-op, full signal to the model (see NamStage::xover_alpha).
        let amp_crossover = shared(0.0);

        let reverb_bypass = shared(0.0);
        let reverb_dry    = shared(0.12);
        let reverb_decay  = shared(0.6);
        let reverb_damp   = shared(0.5);
        let reverb_size   = shared(10.0);

        let limiter_bypass = shared(0.0);
        let limiter_thresh = shared(-6.0);

        let master_vol = shared(1.0);

        let capture = AudioCapture::new((NAM_SAMPLE_RATE as f32 * CAPTURE_SECONDS) as usize);

        let amp_model_l = nam::load_named_model(AMP_MODEL)?;
        let amp_model_r = nam::load_named_model(AMP_MODEL)?;

        let mut engine = Engine::new(
            level.clone(),
            freq.clone(),
            gate.clone(),
            bend.clone(),
            width.clone(),
            filter.clone(),
            fuzz.clone(),
            velocity.clone(),
            acceleration.clone(),

            thump_amt.clone(),
            thump_trigger.clone(),
            thump_peak.clone(),
            thump_decay.clone(),

            voice_selected.clone(),

            main_sub_lvl.clone(),
            dry_sub_lvl.clone(),

            nam_models,
            nam_names,
            amp_model_l,
            amp_model_r,

            amp_bypass.clone(),
            amp_boost.clone(),
            amp_blend.clone(),
            amp_crossover.clone(),

            reverb_bypass.clone(),
            reverb_dry.clone(),
            reverb_decay.value(),
            reverb_damp.value(),
            reverb_size.value(),

            limiter_bypass.clone(),
            limiter_thresh.clone(),

            master_vol.clone(),

        );

        // Voice *View types are built here, right after the real Voices
        // exist (inside `engine`, same module so private fields are
        // visible) but before `engine` moves into build_stream's closure.
        let voice_dirty = engine.voice_dirty.clone();

        let handles = Handles {
            voice_selected: voice_selected.clone(),
            voice_a: engine.voice_a.view(),
            voice_b: engine.voice_b.view(),
            voice_c: engine.voice_c.view(),
            voice_d: engine.voice_d.view(),
            voice_dirty,

            main_sub_lvl:  main_sub_lvl.clone(),
            dry_sub_lvl:   dry_sub_lvl.clone(),
            thump_peak:    thump_peak.clone(),
            thump_decay:   thump_decay.clone(),

            amp_bypass:    amp_bypass.clone(),
            amp_boost:     amp_boost.clone(),
            amp_blend:     amp_blend.clone(),
            amp_crossover: amp_crossover.clone(),

            reverb_bypass: reverb_bypass.clone(),
            reverb_dry:    reverb_dry.clone(),
            reverb_decay:  reverb_decay.clone(),
            reverb_damp:   reverb_damp.clone(),
            reverb_size:   reverb_size.clone(),

            limiter_bypass: limiter_bypass.clone(),
            limiter_thresh: limiter_thresh.clone(),

            master_vol: master_vol.clone(),
        };

        // Load persisted voice state (see snapshot.rs's module doc) --
        // safe here, before stream.play() starts the audio thread. Missing
        // files (fresh checkout) are silent no-ops, keeping each voice's
        // compiled-in defaults.
        if let Some(name) = snapshot::load_selected() {
            if let Some(idx) = VOICE_NAMES.iter().position(|n| *n == name) {
                voice_selected.set_value(idx as f32);
            }
        }
        load_voice_state(VOICE_NAMES[0], |f| handles.voice_a.apply(f));
        load_voice_state(VOICE_NAMES[1], |f| handles.voice_b.apply(f));
        load_voice_state(VOICE_NAMES[2], |f| handles.voice_c.apply(f));
        load_voice_state(VOICE_NAMES[3], |f| handles.voice_d.apply(f));

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        println!("║ Output device: {device}");
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;
        println!("║ Output config: {supported:?}");

        let sample_format = supported.sample_format();
        let buffer_range = *supported.buffer_size();
        let mut config: cpal::StreamConfig = supported.into();
        config.buffer_size = match buffer_range {
            cpal::SupportedBufferSize::Range { min, max } => cpal::BufferSize::Fixed(AUDIO_BUFFER_FRAMES.clamp(min, max)),
            cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
        };
        println!("║ Output buffer size: {:?}", config.buffer_size);

        engine.set_sample_rate(config.sample_rate as f64);

        let errors = Arc::new(Mutex::new(Vec::new()));
        let err_fn = {
            let errors = errors.clone();
            move |e| {
                let mut log = errors.lock().unwrap();
                if log.len() >= AUDIO_ERROR_LOG_CAP { log.remove(0); }
                log.push(format!("{e}"));
            }
        };

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, engine, capture.clone(), err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, engine, capture.clone(), err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, engine, capture.clone(), err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native audio backend OK.");

        Ok(AudioOutput {
            level, freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            handles,
            capture, errors, stream,
        })
    }
}

// Loads `config/{voice_name}.state` and hands the fields to `apply` --
// factored out of AudioOutput::new purely to avoid repeating the "NotFound
// is fine, anything else is worth a warning" branch four times.
fn load_voice_state (voice_name: &str, apply: impl FnOnce(&[(String, f32)])) {
    match snapshot::load_state(voice_name) {
        Ok(fields) => apply(&fields),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        Err(e) => eprintln!("║ Failed to load {voice_name} state: {e}"),
    }
}

// Picks an output config at exactly `target_rate` to match NAM A2 models
fn pick_output_config (device: &cpal::Device, target_rate: u32) -> io::Result<cpal::SupportedStreamConfig> {
    let ranges = device.supported_output_configs()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("no output configs available: {e}")))?;

    // A device can advertise the same rate under several sample formats
    // (e.g. the PipeWire ALSA plugin lists U8/I16/U16/F32 ranges for one
    // "default" device); pick the best-quality format we actually support
    // in build_stream rather than whichever the enumeration happens to
    // yield first.
    let format_rank = |f: cpal::SampleFormat| match f {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I16 => 1,
        cpal::SampleFormat::U16 => 2,
        _ => 3,
    };

    let best = ranges
        .filter(|r| r.min_sample_rate() <= target_rate && r.max_sample_rate() >= target_rate)
        .min_by_key(|r| format_rank(r.sample_format()));

    if let Some(range) = best {
        return Ok(range.with_sample_rate(target_rate));
    }

    println!("║ ⚠ Output device has no config supporting {target_rate}Hz (the rate every NAM model in nam/ was captured at) -- NAM output will be pitched/timed wrong. Falling back to the device default.");

    device.default_output_config()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("no usable output config: {e}")))
}

// Owns the full per-note graph: all voices wired in parallel (each silences
// itself when not selected -- see voice.rs) summed with main_sub, through
// the fixed amp/reverb/limiter stages, combined with dry_sub at the very end
// (dry_sub bypasses amp/reverb/limiter entirely).
struct Engine {
    level: Shared,
    freq: Shared,
    gate: Shared,
    bend: Shared,
    width: Shared,
    filter: Shared,
    fuzz: Shared,
    thump_amt: Shared,
    velocity: Shared,
    acceleration: Shared,

    main_sub_tri: An<WaveSynth<U1>>,
    dry_sub:      An<Sine<f64>>,
    envelope:     Box<dyn AudioUnit>,

    voice_a: ReeseVoice,
    voice_b: GrowlVoice,
    voice_c: BasicVoice,
    voice_d: SwarmVoice,
    voice_selected: Shared,

    voice_dirty: [Arc<AtomicBool>; 4],

    main_sub_lvl:  Shared,
    dry_sub_lvl:   Shared,

    amp_l: nam::NamStage,
    amp_r: nam::NamStage,
    amp_bypass:    Shared,
    amp_boost:     Shared,
    amp_blend:     Shared,
    amp_crossover: Shared,

    reverb: ReverbFx,
    reverb_bypass: Shared,
    reverb_dry:    Shared,

    limiter: Compressor,
    limiter_bypass: Shared,
    limiter_thresh: Shared,

    master_vol: Shared,

    cc_input: CcInput,
}

impl Engine {
    fn new (
        level: Shared,
        freq: Shared,
        gate: Shared,
        bend: Shared,
        width: Shared,
        filter: Shared,
        fuzz: Shared,
        velocity: Shared,
        acceleration: Shared,

        thump_amt: Shared,
        thump_trigger: Shared,
        thump_peak: Shared,
        thump_decay: Shared,

        voice_selected: Shared,

        main_sub_lvl: Shared,
        dry_sub_lvl: Shared,

        nam_models: Vec<Option<nam::NamModelSlot>>,
        nam_names: Arc<Vec<String>>,
        amp_model_l: nam::NamModelSlot,
        amp_model_r: nam::NamModelSlot,

        amp_bypass: Shared,
        amp_boost: Shared,
        amp_blend: Shared,
        amp_crossover: Shared,

        reverb_bypass: Shared,
        reverb_dry: Shared,
        reverb_decay: f32,
        reverb_damp: f32,
        reverb_size: f32,

        limiter_bypass: Shared,
        limiter_thresh: Shared,

        master_vol: Shared,

    ) -> Engine {
        // Each NamStage holds exactly one fixed model -- no Bypass slot, no cycling.
        let amp_l = nam::NamStage::new(vec![Some(amp_model_l)], shared(0.0));
        let amp_r = nam::NamStage::new(vec![Some(amp_model_r)], shared(0.0));

        Engine {
            freq, gate,
            level, bend, width, filter, fuzz, thump_amt, velocity, acceleration,
            main_sub_tri: triangle(),
            dry_sub:      sine(),
            envelope: Box::new(adsr_live(ENVELOPE_ATTACK, 0.0, 1.0, ENVELOPE_RELEASE)),
            voice_selected,

            voice_a: ReeseVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_b: GrowlVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_c: BasicVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_d: SwarmVoice::new(nam_models, nam_names, thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_dirty: std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),

            main_sub_lvl,
            dry_sub_lvl,

            amp_l,
            amp_r,
            amp_bypass,
            amp_boost,
            amp_blend,
            amp_crossover,

            reverb: ReverbFx::new(reverb_size, reverb_decay, reverb_damp), reverb_bypass, reverb_dry,

            limiter: Compressor::new(), limiter_bypass, limiter_thresh,

            master_vol,

            cc_input: CcInput::connect(),
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub_tri.set_sample_rate(sr);
        self.dry_sub.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        self.voice_a.set_sample_rate(sr);
        self.voice_b.set_sample_rate(sr);
        self.voice_c.set_sample_rate(sr);
        self.voice_d.set_sample_rate(sr);
        self.amp_l.set_sample_rate(sr);
        self.amp_r.set_sample_rate(sr);
        self.reverb.set_sample_rate(sr);
        self.limiter.set_sample_rate(sr);
    }

    // Everything before the amp stage, gated by the envelope. Returns
    // (dry_l, dry_r, dry_sub) -- split out of a single tick() so build_stream
    // can batch dry_l/dry_r across a block and run the amp stage once per
    // block instead of once per sample (see run_nam / NamStage::process_block).
    // Snapshot the live performance signals and push a read-only copy into
    // every voice. Called once per block from build_stream (knob-rate: the
    // main thread only rewrites these cells per frame, so within a block they
    // never change) -- each voice keeps its own SignalState copy and reads the
    // fields it cares about in render().
    fn update_voice_signals (&mut self) {
        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            ..SignalState::new()
        };
        self.voice_a.set_signal(&signal);
        self.voice_b.set_signal(&signal);
        self.voice_c.set_signal(&signal);
        self.voice_d.set_signal(&signal);
    }

    fn tick_pre_nam (&mut self) -> (f32, f32, f32) {
        let bend_mult = 2f32.powf(self.bend.value());
        let base_freq = self.freq.value() * bend_mult;

        let sel = self.voice_selected.value();
        let voice_a_out = self.voice_a.tick(&Frame::from([base_freq, sel]));
        let voice_b_out = self.voice_b.tick(&Frame::from([base_freq, sel]));
        let voice_c_out = self.voice_c.tick(&Frame::from([base_freq, sel]));
        let voice_d_out = self.voice_d.tick(&Frame::from([base_freq, sel]));
        let voice_l = voice_a_out[0] + voice_b_out[0] + voice_c_out[0] + voice_d_out[0];
        let voice_r = voice_a_out[1] + voice_b_out[1] + voice_c_out[1] + voice_d_out[1];

        let main_sub = self.main_sub_tri.filter_mono(base_freq) * self.main_sub_lvl.value();

        let env = self.envelope.filter_mono(self.gate.value());

        // dry_sub: one octave below base_freq, bypasses amp/reverb/limiter entirely.
        let dry_sub = self.dry_sub.filter_mono(base_freq * 0.5) * self.dry_sub_lvl.value() * env;

        let dry_l = (voice_l + main_sub) * env;
        let dry_r = (voice_r + main_sub) * env;

        (dry_l, dry_r, dry_sub)
    }

    // Knob-rate values, read once per block rather than per sample.
    fn nam_block_params (&self) -> (f32, f32, f32, f32) {
        let level = if self.amp_bypass.value() >= 1.0 { 0.0 } else { 1.0 };
        (level, self.amp_blend.value(), self.amp_boost.value(), self.amp_crossover.value())
    }

    fn run_nam (&mut self, block_l: &mut [f32], block_r: &mut [f32]) {
        let (level, blend, boost, crossover_hz) = self.nam_block_params();
        self.amp_l.process_block(block_l, level, blend, boost, crossover_hz);
        self.amp_r.process_block(block_r, level, blend, boost, crossover_hz);
    }

    // Reverb, limiter, then final mix with dry_sub (which bypassed amp entirely).
    fn tick_post_nam (&mut self, dry_l: f32, dry_r: f32, dry_sub: f32) -> (f32, f32) {
        // Soft-clip rather than a hard wall so transient peaks saturate instead of clipping.
        let mut l = dry_l.tanh();
        let mut r = dry_r.tanh();

        // Skip the call entirely rather than driving level=0 -- ReverbFx mono-sums
        // its input, so this is the only way to preserve stereo width while bypassed.
        if self.reverb_bypass.value() < 1.0 {
            let out = self.reverb.tick(&Frame::from([l, r, 1.0, self.reverb_dry.value(), 0.0, 0.0, 0.0]));
            l = out[0];
            r = out[1];
        }

        if self.limiter_bypass.value() < 1.0 {
            let (ll, rr) = self.limiter.tick(l, r, self.limiter_thresh.value());
            l = ll;
            r = rr;
        }

        // Master volume is modulated by zgicabra level
        let vol = self.master_vol.value() * self.level.value();

        (
            ((l + dry_sub) * vol).clamp(-1.0, 1.0),
            ((r + dry_sub) * vol).clamp(-1.0, 1.0),
        )
    }
}

fn build_stream<T> (
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut engine: Engine,
    capture: AudioCapture,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;

    // Pre-amp dry signal scratch (L, R, dry_sub) -- sized once, never
    // reallocated on the audio thread.
    let mut dryl_scratch:   Vec<f32> = vec![0.0; NAM_BLOCK_CAP];
    let mut dryr_scratch:   Vec<f32> = vec![0.0; NAM_BLOCK_CAP];
    let mut drysub_scratch: Vec<f32> = vec![0.0; NAM_BLOCK_CAP];

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels;
            let mut done = 0;

            while done < frames {
                let n = std::cmp::min(frames - done, NAM_BLOCK_CAP);
                let dryl_block   = &mut dryl_scratch[..n];
                let dryr_block   = &mut dryr_scratch[..n];
                let drysub_block = &mut drysub_scratch[..n];

                // Only the selected voice's on_block_start runs -- GrowlVoice's
                // is a full NAM WaveNet block inference, wasted CPU when Growl
                // isn't even the active voice. Switching voices while a note is
                // audible can produce a brief startup transient on Growl's
                // model (see NamStage::process_block's warm-state comment);
                // switching between notes/songs is silent.
                let selected = engine.voice_selected.value() as usize;
                if selected == ReeseVoice::INDEX { engine.voice_a.on_block_start(n); }
                if selected == GrowlVoice::INDEX { engine.voice_b.on_block_start(n); }
                if selected == BasicVoice::INDEX { engine.voice_c.on_block_start(n); }
                if selected == SwarmVoice::INDEX { engine.voice_d.on_block_start(n); }

                engine.update_voice_signals();

                // Drain the CC ring buffer and retarget each message to
                // whichever voice is currently selected -- switching voices
                // mid-performance retargets subsequent CC messages, it
                // doesn't replay queued ones onto the old voice.
                while let Some((cc, value)) = engine.cc_input.pop() {
                    crate::dbg!("audio::build_stream - applying CC {cc}={value} to voice index {selected}");
                    if selected == ReeseVoice::INDEX { engine.voice_a.apply_cc(cc, value); }
                    if selected == GrowlVoice::INDEX { engine.voice_b.apply_cc(cc, value); }
                    if selected == BasicVoice::INDEX { engine.voice_c.apply_cc(cc, value); }
                    if selected == SwarmVoice::INDEX { engine.voice_d.apply_cc(cc, value); }
                    if let Some(flag) = engine.voice_dirty.get(selected) { flag.store(true, Ordering::Relaxed); }
                }

                for i in 0..n {
                    let (dry_l, dry_r, dry_sub) = engine.tick_pre_nam();
                    dryl_block[i]   = dry_l;
                    dryr_block[i]   = dry_r;
                    drysub_block[i] = dry_sub;
                }

                engine.run_nam(dryl_block, dryr_block);

                for i in 0..n {
                    let (left, right) = engine.tick_post_nam(dryl_block[i], dryr_block[i], drysub_block[i]);
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

impl AudioOutput {
    pub fn panic (&mut self) {
        self.gate.set_value(GATE_OFF);
    }

    pub fn handle_signal (&mut self, signal: &SignalState) {
        self.level.set_value(signal.level);
        self.bend.set_value(signal.bend);
        self.width.set_value(signal.width);
        self.filter.set_value(signal.filter);
        self.fuzz.set_value(signal.fuzz);
        self.thump_amt.set_value(signal.thump);
        self.velocity.set_value(signal.velocity);
        self.acceleration.set_value(signal.acceleration);
    }

    pub fn handle_event (&mut self, delta: &DeltaEvent) {
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
            // Index must match zgicabra::Voice's enum order.
            DeltaEvent::VoiceChange(voice) => {
                let idx = *voice as u8 as usize;
                self.handles.voice_selected.set_value(idx as f32);
                if let Some(name) = VOICE_NAMES.get(idx) {
                    if let Err(e) = snapshot::save_selected(name) {
                        eprintln!("║ Failed to persist selected voice: {e}");
                    }
                }
            },
            DeltaEvent::Panic()    => self.gate.set_value(GATE_OFF),
            _ => {},
        }
    }
}
