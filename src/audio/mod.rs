//
// Audio Engine
//

use std::io;
use std::collections::VecDeque;
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
mod voice;
mod cc_input;
mod signal;
mod sample;
mod analysis;
mod wavetable;

use signal::SharedSignal;

use voice::{Voice, ViewFields};
pub mod crusher;
pub mod comb;
pub mod snapshot;

use engine::Engine;
pub use engine::{ReeseView, GrowlView, BasicView, SwarmView};


// Debug: captures snippet of cpal output stream to check non-zero output
const CAPTURE_SECONDS: f32 = 0.1;
const AUDIO_ERROR_LOG_CAP: usize = 50;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

pub const NAM_SAMPLE_RATE: u32 = 48_000;
const AUDIO_BUFFER_FRAMES: cpal::FrameCount = 1024;

const ENVELOPE_ATTACK:  f32 = 0.003;
const ENVELOPE_RELEASE: f32 = 0.1;

pub type AudioErrors = Arc<Mutex<Vec<String>>>;


// Taps the raw cpal output stream (mono, left channel) into a fixed-size
// ring buffer, continuously. Used by main.rs's --test self-test to confirm
// audio is actually producing signal, and by the debug panel's live
// spectrum readout (see ui/panel_debug.rs).
#[derive(Clone)]
pub struct AudioCapture {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    cap:    usize,
}

impl AudioCapture {
    fn new (cap: usize) -> AudioCapture {
        AudioCapture { buffer: Arc::new(Mutex::new(VecDeque::with_capacity(cap))), cap }
    }

    // Clears the buffer so the next `cap` samples pushed are a fresh
    // window -- the self-test uses this to capture strictly after the test
    // tone starts.
    pub fn start (&self) {
        self.buffer.lock().unwrap().clear();
    }

    pub fn is_full (&self) -> bool {
        self.buffer.lock().unwrap().len() >= self.cap
    }

    pub fn samples (&self) -> Vec<f32> {
        self.buffer.lock().unwrap().iter().copied().collect()
    }

    fn push (&self, sample: f32) {
        let mut buf = self.buffer.lock().unwrap();
        buf.push_back(sample);
        if buf.len() > self.cap {
            buf.pop_front();
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

    pub reverb_bypass: Shared,
    pub reverb_mix:    Shared,
    pub reverb_decay:  Shared,
    pub reverb_damp:   Shared,
    pub reverb_size:   Shared,

    pub limiter_bypass: Shared,
    pub limiter_thresh: Shared,

    pub master_vol: Shared,

    pub out_level_peak_l: Shared,
    pub out_level_peak_r: Shared,
    pub out_level_rms_l:  Shared,
    pub out_level_rms_r:  Shared,

    pub engine_selected_knob: Shared,
}

impl Handles {
    // Engine's own knob list (see engine::ENGINE_KNOB_RANGES) -- the ordered
    // (name, cell, min, max) list the debug panel's knob display iterates,
    // same shape as a *View's knobs() (see ui/panel_debug.rs).
    pub fn engine_knobs (&self) -> Vec<(&'static str, Shared, f32, f32)> {
        engine::ENGINE_KNOB_RANGES.iter().map(|&(name, min, max)| {
            let cell = match name {
                "master_vol"     => self.master_vol.clone(),
                "limiter"        => self.limiter_thresh.clone(),
                "reverb_mix"     => self.reverb_mix.clone(),
                "main_sub_lvl"   => self.main_sub_lvl.clone(),
                "dry_sub_lvl"    => self.dry_sub_lvl.clone(),
                _ => unreachable!("ENGINE_KNOB_RANGES entry with no matching Handles cell"),
            };
            (name, cell, min, max)
        }).collect()
    }
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
    cc_connected: bool,

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
        let thump_decay   = shared(0.07);

        let reverb_bypass = shared(0.0);
        let reverb_mix    = shared(0.12);
        let reverb_decay  = shared(0.6);
        let reverb_damp   = shared(0.5);
        let reverb_size   = shared(10.0);

        let limiter_bypass = shared(0.0);
        let limiter_thresh = shared(-6.0);

        let master_vol = shared(1.0);

        let out_level_peak_l = shared(0.0);
        let out_level_peak_r = shared(0.0);
        let out_level_rms_l  = shared(0.0);
        let out_level_rms_r  = shared(0.0);

        let capture = AudioCapture::new((NAM_SAMPLE_RATE as f32 * CAPTURE_SECONDS) as usize);

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

            reverb_bypass.clone(),
            reverb_mix.clone(),
            reverb_decay.value(),
            reverb_damp.value(),
            reverb_size.value(),

            limiter_bypass.clone(),
            limiter_thresh.clone(),

            master_vol.clone(),

            out_level_peak_l.clone(),
            out_level_peak_r.clone(),
            out_level_rms_l.clone(),
            out_level_rms_r.clone(),

        );

        // Cloned out here, before `engine` moves into build_stream's closure.
        let voice_dirty = engine.voice_dirty.clone();
        let (voice_a, voice_b, voice_c, voice_d) = engine.voice_views.clone();
        let voice_names = engine.voice_names();
        let engine_selected_knob = engine.selected_knob.clone();
        let cc_connected = engine.cc_input.connected();

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

            reverb_bypass: reverb_bypass.clone(),
            reverb_mix:    reverb_mix.clone(),
            reverb_decay:  reverb_decay.clone(),
            reverb_damp:   reverb_damp.clone(),
            reverb_size:   reverb_size.clone(),

            limiter_bypass: limiter_bypass.clone(),
            limiter_thresh: limiter_thresh.clone(),

            master_vol: master_vol.clone(),

            out_level_peak_l: out_level_peak_l.clone(),
            out_level_peak_r: out_level_peak_r.clone(),
            out_level_rms_l:  out_level_rms_l.clone(),
            out_level_rms_r:  out_level_rms_r.clone(),

            engine_selected_knob,
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
            cc_connected,
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

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels;

            // Only the selected voice's on_block_start runs -- GrowlVoice's
            // is a full NAM WaveNet block inference, wasted CPU when Growl
            // isn't even the active voice. Switching voices while a note is
            // audible can produce a brief startup transient on Growl's
            // model (see NamStage::process_block's warm-state comment);
            // switching between notes/songs is silent.
            let selected = engine.voice_selected.value() as usize;
            for voice in engine.voices.iter_mut() {
                if voice.index() == selected { voice.on_block_start(frames); }
            }

            // Drain the CC ring buffer -- the fixed 8-knob AKAI layout (see
            // voice.rs's module doc): CC1-4 are the global filter/width/
            // fuzz/thump signals (gated the same way SharedSignal::set()
            // gates wand writes -- only applied if actually different, so
            // whichever of wand/CC moved most recently wins); CC5/6 move
            // the selected voice's own selected_knob/value; CC7/8 do the
            // same for Engine's own knob list. Retargeted to whichever
            // voice is currently selected -- switching voices mid-
            // performance retargets subsequent CC5/6 messages, it doesn't
            // replay queued ones onto the old voice.
            while let Some((cc, value)) = engine.cc_input.pop() {
                crate::dbg!("audio::build_stream - CC {cc}={value} (selected voice index {selected})");
                match cc {
                    1 => { let s = &engine.signal; if value != s.filter.value() { s.filter.set_value(value); } },
                    2 => { let s = &engine.signal; if value != s.width.value()  { s.width.set_value(value); } },
                    3 => { let s = &engine.signal; if value != s.fuzz.value()   { s.fuzz.set_value(value); } },
                    4 => { let s = &engine.signal; if value != s.thump.value()  { s.thump.set_value(value); } },
                    5 => {
                        for voice in engine.voices.iter_mut() {
                            if voice.index() == selected {
                                let idx = std::cmp::min((value * voice.knob_count() as f32).floor() as usize,
                                    voice.knob_count().saturating_sub(1));
                                voice.set_selected_knob(idx);
                            }
                        }
                    },
                    6 => {
                        for voice in engine.voices.iter_mut() {
                            if voice.index() == selected {
                                let k = voice.selected_knob();
                                voice.set_knob_value(k, value);
                            }
                        }
                        if let Some(flag) = engine.voice_dirty.get(selected) { flag.store(true, Ordering::Relaxed); }
                    },
                    7 => {
                        let idx = std::cmp::min((value * engine.knob_count() as f32).floor() as usize,
                            engine.knob_count().saturating_sub(1));
                        engine.set_selected_knob(idx);
                    },
                    8 => {
                        let k = engine.selected_knob();
                        engine.set_knob_value(k, value);
                    },
                    _ => {},
                }
            }

            for i in 0..frames {
                let (left, right) = engine.tick();
                capture.push(left);
                let frame_start = i * channels;
                for ch in 0..channels {
                    data[frame_start + ch] = T::from_sample(if ch % 2 == 0 { left } else { right });
                }
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
        self.signal.set(signal, self.cc_connected);
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
