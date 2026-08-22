//
// Audio Engine
//

use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::zgicabra::{DeltaEvent, SignalState};

mod engine;
mod nam;
mod nam_node;
mod nam_graph;
mod stutter;
mod growl;
mod swarm;
mod basic;
mod reese;
mod fm;
mod filter;
mod reverb;
mod compressor;
mod voice;
mod cc_input;
mod signal;

use signal::SharedSignal;

use voice::{Voice, ViewFields};
pub mod crusher;
pub mod snapshot;

use engine::Engine;
pub use engine::{ReeseView, GrowlView, BasicView, SwarmView};
use nam::NAM_BLOCK_CAP;


// Debug: captures snippet of cpal output stream to check non-zero output
const CAPTURE_SECONDS: f32 = 0.1;
const AUDIO_ERROR_LOG_CAP: usize = 50;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const NAM_SAMPLE_RATE: u32 = 48_000;
const AUDIO_BUFFER_FRAMES: cpal::FrameCount = 1024;

const ENVELOPE_ATTACK:  f32 = 0.003;
const ENVELOPE_RELEASE: f32 = 0.1;

const AMP_MODEL: &str = "lowgain";

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

#[derive(Clone)]
pub struct Handles {
    pub voice_selected: Shared,
    pub voice_a: ReeseView,
    pub voice_b: GrowlView,
    pub voice_c: BasicView,
    pub voice_d: SwarmView,
    pub voice_names: [&'static str; 4],

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

impl Handles {
    // The currently-selected voice's own name (see Voice::name, sourced via
    // Engine::voice_names) -- the UI reads this instead of Zgicabra tracking
    // a separate copy of "which voice is this".
    pub fn voice_name (&self) -> &'static str {
        self.voice_names[self.voice_selected.value() as usize]
    }
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
        let views: [(&str, &dyn ViewFields); 4] = [
            (self.handles.voice_names[0], &self.handles.voice_a),
            (self.handles.voice_names[1], &self.handles.voice_b),
            (self.handles.voice_names[2], &self.handles.voice_c),
            (self.handles.voice_names[3], &self.handles.voice_d),
        ];

        for (i, (name, view)) in views.iter().enumerate() {
            if dirty[i].swap(false, Ordering::Relaxed) {
                if let Err(e) = snapshot::save_state(name, &view.fields()) {
                    eprintln!("║ Failed to persist {name} state: {e}");
                }
            }
        }
    }
}

pub struct AudioOutput {
    freq:              Shared,
    gate:              Shared,
    thump_trigger:     Shared,

    signal: SharedSignal,

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

        let freq          = shared(110.0);
        let gate          = shared(GATE_OFF);
        let thump_trigger = shared(0.0);

        let signal = SharedSignal::new();
        signal.level.set_value(1.0);

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
            signal.clone(),
            freq.clone(),
            gate.clone(),

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

        // Cloned out here, before `engine` moves into build_stream's closure.
        let voice_dirty = engine.voice_dirty.clone();
        let (voice_a, voice_b, voice_c, voice_d) = engine.voice_views.clone();
        let voice_names = engine.voice_names();

        let handles = Handles {
            voice_selected: voice_selected.clone(),
            voice_a,
            voice_b,
            voice_c,
            voice_d,
            voice_names,
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
            if let Some(idx) = voice_names.iter().position(|n| *n == name) {
                voice_selected.set_value(idx as f32);
            }
        }
        let views: [(&str, &dyn ViewFields); 4] = [
            (voice_names[0], &handles.voice_a),
            (voice_names[1], &handles.voice_b),
            (voice_names[2], &handles.voice_c),
            (voice_names[3], &handles.voice_d),
        ];
        for (name, view) in views {
            load_voice_state(name, |f| view.apply(f));
        }

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
            freq, gate, thump_trigger,
            signal,
            handles,
            capture, errors, stream,
        })
    }
}

fn load_voice_state (voice_name: &str, apply: impl FnOnce(&[(String, f32)])) {
    match snapshot::load_state(voice_name) {
        Ok(fields) => apply(&fields),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        Err(e) => eprintln!("║ Failed to load {voice_name} state: {e}"),
    }
}

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
                for voice in engine.voices.iter_mut() {
                    if voice.index() == selected { voice.on_block_start(n); }
                }

                // Drain the CC ring buffer and retarget each message to
                // whichever voice is currently selected -- switching voices
                // mid-performance retargets subsequent CC messages, it
                // doesn't replay queued ones onto the old voice.
                while let Some((cc, value)) = engine.cc_input.pop() {
                    crate::dbg!("audio::build_stream - applying CC {cc}={value} to voice index {selected}");
                    for voice in engine.voices.iter_mut() {
                        if voice.index() == selected { voice.apply_cc(cc, value); }
                    }
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
        self.signal.set(signal);
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
            // delta is -1 or 1 -- Zgicabra doesn't track which voice is
            // selected any more, only the engine does (voice_selected).
            DeltaEvent::VoiceChange(delta) => {
                let count = self.handles.voice_names.len() as i8;
                let current = self.handles.voice_selected.value() as i8;
                let idx = (current + delta).rem_euclid(count) as usize;
                self.handles.voice_selected.set_value(idx as f32);
                if let Some(name) = self.handles.voice_names.get(idx) {
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
