//
// Audio Engine
//

use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

use crate::output::DeltaConsumer;
use crate::zgicabra::{DeltaEvent, SignalState};

mod nam;
mod stutter;
mod growl;
mod gorgle;
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
pub mod snapshot;

use nam::NAM_BLOCK_CAP;
use reverb::ReverbFx;
use compressor::Compressor;
use voice::Voice;
use growl::GrowlVoice;
use gorgle::GorgleVoice;
use reese::ReeseVoice;
use basic::BasicVoice;
pub use growl::{GrowlHandle, GrowlParams};
pub use gorgle::{GorgleHandle, GorgleParams};
pub use reese::{ReeseHandle, ReeseParams};
pub use basic::{BasicHandle, BasicParams};
pub use voice::VoiceParams;

const GATE_ON:  f32 = 1.0;
const GATE_OFF: f32 = -1.0;

const NAM_SAMPLE_RATE: u32 = 48_000;

// Fixed envelope times -- not GUI editable, baked into adsr_live at
// construction time.
const ENVELOPE_ATTACK:  f32 = 0.003;
const ENVELOPE_RELEASE: f32 = 0.1;

// Debug: captures snippet of cpal output stream to check non-zero output
const CAPTURE_SECONDS: f32 = 0.1;

// The fixed "amp" stage always runs this one model
const AMP_MODEL: &str = "lowgain";

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

// Direct handle onto the note gate, for main.rs's --test self-test to hold a
// single test tone -- deliberately bypasses DeltaEvent/DeltaConsumer/hydra
// entirely so a failure here isolates to the audio graph itself, independent
// of the note/CC dispatch pipeline (see hydra::mock's audition sequence
// player and MIDI listener for the DeltaEvent-driven equivalent). Note:
// running this at the same time as a real controller note will fight over
// the same `freq`/`gate` cells; it's a manual test tone, not a second voice.
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

// Every GUI-facing handle onto a running AudioOutput, bundled so main.rs/
// gui.rs thread one Option through instead of one per feature.
#[derive(Clone)]
pub struct AudioHandles {
    pub test_tone: TestTone,

    pub voice_selected: Shared,
    pub voice_a: GrowlHandle,
    pub voice_b: GorgleHandle,
    pub voice_c: ReeseHandle,
    pub voice_d: BasicHandle,

    pub main_sub_lvl:  Shared,
    pub main_sub_wave: Shared,
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

    voice_selected: Shared,
    voice_a: GrowlHandle,
    voice_b: GorgleHandle,
    voice_c: ReeseHandle,
    voice_d: BasicHandle,

    main_sub_lvl:  Shared,
    main_sub_wave: Shared,
    dry_sub_lvl:   Shared,
    thump_peak:    Shared,
    thump_decay:   Shared,

    amp_bypass:    Shared,
    amp_boost:     Shared,
    amp_blend:     Shared,
    amp_crossover: Shared,

    reverb_bypass: Shared,
    reverb_dry:    Shared,
    reverb_decay:  Shared,
    reverb_damp:   Shared,
    reverb_size:   Shared,

    limiter_bypass: Shared,
    limiter_thresh: Shared,

    capture: AudioCapture,
    stream:  cpal::Stream,
}

impl AudioOutput {
    // Every handle a UI needs to drive/display this engine, bundled. Cheap
    // to build (every field is an Arc'd atomic cell or Arc'd name list).
    pub fn handles (&self) -> AudioHandles {
        AudioHandles {
            test_tone: TestTone { freq: self.freq.clone(), gate: self.gate.clone() },
            voice_selected: self.voice_selected.clone(),
            voice_a: self.voice_a.clone(),
            voice_b: self.voice_b.clone(),
            voice_c: self.voice_c.clone(),
            voice_d: self.voice_d.clone(),
            main_sub_lvl:  self.main_sub_lvl.clone(),
            main_sub_wave: self.main_sub_wave.clone(),
            dry_sub_lvl:   self.dry_sub_lvl.clone(),
            thump_peak:    self.thump_peak.clone(),
            thump_decay:   self.thump_decay.clone(),
            amp_bypass:    self.amp_bypass.clone(),
            amp_boost:     self.amp_boost.clone(),
            amp_blend:     self.amp_blend.clone(),
            amp_crossover: self.amp_crossover.clone(),
            reverb_bypass: self.reverb_bypass.clone(),
            reverb_dry:    self.reverb_dry.clone(),
            reverb_decay:  self.reverb_decay.clone(),
            reverb_damp:   self.reverb_damp.clone(),
            reverb_size:   self.reverb_size.clone(),
            limiter_bypass: self.limiter_bypass.clone(),
            limiter_thresh: self.limiter_thresh.clone(),
            capture: self.capture.clone(),
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

        println!("║ Loading NAM models for Growl... ");
        let (growl_nam_models, growl_nam_names) = nam::load_nam_models()?;
        let growl_nam_names = Arc::new(growl_nam_names);
        println!("║ Growl NAM models loaded ({} found).", growl_nam_names.len().saturating_sub(1));

        // Defaults to Growl (index 0) so a fresh run has an audible voice.
        let voice_selected = shared(0.0);
        let voice_a = GrowlHandle::new(&GrowlParams::default(), growl_nam_names);
        let voice_b = GorgleHandle::new(&GorgleParams::default());
        let voice_c = ReeseHandle::new(&ReeseParams::default());
        let voice_d = BasicHandle::new(&BasicParams::default());

        let main_sub_lvl  = shared(0.35);
        let main_sub_wave = shared(0.5);
        let dry_sub_lvl   = shared(0.35);
        let thump_peak    = shared(1.5);
        let thump_decay   = shared(0.18);

        let amp_bypass    = shared(0.0);
        let amp_boost     = shared(1.0);
        let amp_blend     = shared(0.0);
        // Default 0Hz: crossover no-op, full signal to the model, same
        // behavior as before this split existed (see NamStage::xover_alpha).
        let amp_crossover = shared(0.0);

        let reverb_bypass = shared(0.0);
        let reverb_dry    = shared(0.12);
        let reverb_decay  = shared(0.6);
        let reverb_damp   = shared(0.5);
        let reverb_size   = shared(10.0);

        let limiter_bypass = shared(0.0);
        let limiter_thresh = shared(-6.0);

        let capture = AudioCapture::new((NAM_SAMPLE_RATE as f32 * CAPTURE_SECONDS) as usize);

        println!("║ Loading NAM amp model ({AMP_MODEL})... ");
        let amp_model_l = nam::load_named_model(AMP_MODEL)?;
        let amp_model_r = nam::load_named_model(AMP_MODEL)?;
        println!("║ NAM amp model loaded.");

        let mut engine = Engine::new(
            freq.clone(), gate.clone(), bend.clone(), width.clone(), filter.clone(), fuzz.clone(),
            thump_amt.clone(), thump_trigger.clone(), velocity.clone(), acceleration.clone(),
            voice_selected.clone(), voice_a.clone(), growl_nam_models, voice_b.clone(), voice_c.clone(), voice_d.clone(),
            main_sub_lvl.clone(), main_sub_wave.clone(), dry_sub_lvl.clone(),
            thump_peak.clone(), thump_decay.clone(),
            amp_model_l, amp_model_r, amp_bypass.clone(), amp_boost.clone(), amp_blend.clone(), amp_crossover.clone(),
            reverb_bypass.clone(), reverb_dry.clone(), reverb_decay.value(), reverb_damp.value(), reverb_size.value(),
            limiter_bypass.clone(), limiter_thresh.clone(),
        );

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        println!("║ Output device: {device}");
        let supported = pick_output_config(&device, NAM_SAMPLE_RATE)?;
        println!("║ Output config: {supported:?}");

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        engine.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

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
            freq, gate, bend, width, filter, fuzz, thump_amt, thump_trigger, velocity, acceleration,
            voice_selected, voice_a, voice_b, voice_c, voice_d,
            main_sub_lvl, main_sub_wave, dry_sub_lvl, thump_peak, thump_decay,
            amp_bypass, amp_boost, amp_blend, amp_crossover,
            reverb_bypass, reverb_dry, reverb_decay, reverb_damp, reverb_size,
            limiter_bypass, limiter_thresh,
            capture, stream,
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

// Owns the full per-note graph: both voices wired in parallel (each
// silences itself when not selected -- see voice.rs) summed with main_sub,
// through the fixed amp/reverb/limiter stages, combined with dry_sub at the
// very end (dry_sub bypasses amp/reverb/limiter entirely, same as before).
struct Engine {
    freq: Shared, gate: Shared, bend: Shared, width: Shared, filter: Shared, fuzz: Shared,
    thump_amt: Shared, velocity: Shared, acceleration: Shared,

    main_sub_tri: An<WaveSynth<U1>>,
    main_sub_saw: An<WaveSynth<U1>>,
    dry_sub:      An<Sine<f64>>,
    envelope:     Box<dyn AudioUnit>,

    voice_a: GrowlVoice,
    voice_b: GorgleVoice,
    voice_c: ReeseVoice,
    voice_d: BasicVoice,
    voice_selected: Shared,

    main_sub_lvl:  Shared,
    main_sub_wave: Shared,
    dry_sub_lvl:   Shared,

    // Two fully independent NamStage instances (own weights, own WaveNet
    // dilation state) -- "amp" is stereo per spec, and sharing one model
    // instance across both channels would mix L/R history into one
    // recurrent state, which is wrong, not just cheaper.
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
}

impl Engine {
    fn new (
        freq: Shared, gate: Shared, bend: Shared, width: Shared, filter: Shared, fuzz: Shared,
        thump_amt: Shared, thump_trigger: Shared, velocity: Shared, acceleration: Shared,
        voice_selected: Shared, voice_a: GrowlHandle, growl_nam_models: Vec<Option<nam::NamModelSlot>>,
        voice_b: GorgleHandle, voice_c: ReeseHandle, voice_d: BasicHandle,
        main_sub_lvl: Shared, main_sub_wave: Shared, dry_sub_lvl: Shared,
        thump_peak: Shared, thump_decay: Shared,
        amp_model_l: nam::NamModelSlot, amp_model_r: nam::NamModelSlot,
        amp_bypass: Shared, amp_boost: Shared, amp_blend: Shared, amp_crossover: Shared,
        reverb_bypass: Shared, reverb_dry: Shared,
        reverb_decay: f32, reverb_damp: f32, reverb_size: f32,
        limiter_bypass: Shared, limiter_thresh: Shared,
    ) -> Engine {
        // Model selector is fixed at 0 forever -- these NamStages each hold
        // exactly one model, no Bypass slot, no cycling (see AudioOutput::new).
        let amp_l = nam::NamStage::new(vec![Some(amp_model_l)], shared(0.0));
        let amp_r = nam::NamStage::new(vec![Some(amp_model_r)], shared(0.0));

        Engine {
            freq, gate, bend, width, filter, fuzz, thump_amt, velocity, acceleration,
            main_sub_tri: triangle(),
            main_sub_saw: saw(),
            dry_sub:      sine(),
            envelope: Box::new(adsr_live(ENVELOPE_ATTACK, 0.0, 1.0, ENVELOPE_RELEASE)),

            voice_a: GrowlVoice::new(voice_a, growl_nam_models, thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_b: GorgleVoice::new(voice_b, thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_c: ReeseVoice::new(voice_c, thump_trigger.clone(), thump_peak.clone(), thump_decay.clone()),
            voice_d: BasicVoice::new(voice_d, thump_trigger, thump_peak, thump_decay),
            voice_selected,

            main_sub_lvl, main_sub_wave, dry_sub_lvl,

            amp_l, amp_r, amp_bypass, amp_boost, amp_blend, amp_crossover,

            reverb: ReverbFx::new(reverb_size, reverb_decay, reverb_damp), reverb_bypass, reverb_dry,

            limiter: Compressor::new(), limiter_bypass, limiter_thresh,
        }
    }

    fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub_tri.set_sample_rate(sr);
        self.main_sub_saw.set_sample_rate(sr);
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

    // Everything up to (not including) the amp stage: both voices (parallel,
    // self-gating) summed with main_sub, gated by the envelope. Returns
    // (dry_l, dry_r, dry_sub) -- dry_sub bypasses amp/reverb/limiter entirely
    // and is re-added at the very end by tick_post_nam. Split out of a single
    // tick() so build_stream can batch every sample's dry_l/dry_r into a
    // block and run the amp stage once per block instead of once per sample
    // -- see run_nam and NamStage::process_block in nam.rs for why.
    fn tick_pre_nam (&mut self) -> (f32, f32, f32) {
        let signal = SignalState {
            bend: self.bend.value(), width: self.width.value(), thump: self.thump_amt.value(),
            filter: self.filter.value(), fuzz: self.fuzz.value(),
            velocity: self.velocity.value(), acceleration: self.acceleration.value(),
            ..SignalState::new()
        };

        let bend_mult = 2f32.powf(signal.bend);
        let base_freq = self.freq.value() * bend_mult;

        let sel = self.voice_selected.value();
        self.voice_a.set_signal(signal.bend, signal.filter, signal.fuzz, signal.width, signal.thump);
        self.voice_b.set_signal(signal.bend, signal.filter, signal.fuzz, signal.width, signal.thump);
        self.voice_c.set_signal(signal.bend, signal.filter, signal.fuzz, signal.width, signal.thump);
        self.voice_d.set_signal(signal.bend, signal.filter, signal.fuzz, signal.width, signal.thump);
        let voice_a_out = self.voice_a.tick(&Frame::from([base_freq, sel]));
        let voice_b_out = self.voice_b.tick(&Frame::from([base_freq, sel]));
        let voice_c_out = self.voice_c.tick(&Frame::from([base_freq, sel]));
        let voice_d_out = self.voice_d.tick(&Frame::from([base_freq, sel]));
        let voice_l = voice_a_out[0] + voice_b_out[0] + voice_c_out[0] + voice_d_out[0];
        let voice_r = voice_a_out[1] + voice_b_out[1] + voice_c_out[1] + voice_d_out[1];

        let main_sub_wave = (self.main_sub_wave.value() * signal.filter).clamp(0.0, 1.0);
        let tri = self.main_sub_tri.filter_mono(base_freq);
        let saw = self.main_sub_saw.filter_mono(base_freq);
        let main_sub = (tri * (1.0 - main_sub_wave) + saw * main_sub_wave) * self.main_sub_lvl.value();

        let env = self.envelope.filter_mono(self.gate.value());

        // dry_sub: hardcoded one octave below base_freq, bypasses amp/reverb/
        // limiter entirely -- same as before this refactor.
        let dry_sub = self.dry_sub.filter_mono(base_freq * 0.5) * self.dry_sub_lvl.value() * env;

        let dry_l = (voice_l + main_sub) * env;
        let dry_r = (voice_r + main_sub) * env;

        (dry_l, dry_r, dry_sub)
    }

    // amp_bypass/blend/boost read once per block (not per sample), right
    // before the batched amp call -- see run_nam. These are slow knob-rate
    // values, so block-rate resolution costs nothing audible.
    fn nam_block_params (&self) -> (f32, f32, f32, f32) {
        let level = if self.amp_bypass.value() >= 1.0 { 0.0 } else { 1.0 };
        (level, self.amp_blend.value(), self.amp_boost.value(), self.amp_crossover.value())
    }

    // Runs both amp instances over a whole block in place -- see
    // NamStage::process_block for why this must be a block call, not a
    // per-sample one.
    fn run_nam (&mut self, block_l: &mut [f32], block_r: &mut [f32]) {
        let (level, blend, boost, crossover_hz) = self.nam_block_params();
        self.amp_l.process_block(block_l, level, blend, boost, crossover_hz);
        self.amp_r.process_block(block_r, level, blend, boost, crossover_hz);
    }

    // Everything after the amp stage: reverb, limiter, final mix with
    // dry_sub (which bypassed amp entirely). dry_l/dry_r are this sample's
    // already-batched amp output (see run_nam).
    fn tick_post_nam (&mut self, dry_l: f32, dry_r: f32, dry_sub: f32) -> (f32, f32) {
        // Soft-clip instead of a hard wall so rare transient peaks
        // saturate instead of digitally clipping.
        let mut l = dry_l.tanh();
        let mut r = dry_r.tanh();

        // Bypass skips the call entirely (rather than driving ReverbFx's
        // own level=0 path) so a bypassed reverb preserves whatever stereo
        // width the voice produced -- ReverbFx itself mono-sums its input
        // before the tail (untouched from before this refactor, when
        // everything upstream really was mono), so stereo width is only
        // preserved through this stage while it's off.
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

        (
            (l + dry_sub).clamp(-1.0, 1.0),
            (r + dry_sub).clamp(-1.0, 1.0),
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

    // Scratch for the pre-amp dry signal (L, R, and the dry_sub that
    // bypasses amp entirely) -- sized once here, never reallocated on the
    // audio thread. Chunking by NAM_BLOCK_CAP is just a fixed-size-scratch
    // safety net; real cpal callback sizes are always far smaller.
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

                engine.voice_a.on_block_start(n);
                engine.voice_b.on_block_start(n);
                engine.voice_c.on_block_start(n);
                engine.voice_d.on_block_start(n);

                for i in 0..n {
                    let (dry_l, dry_r, dry_sub) = engine.tick_pre_nam();
                    dryl_block[i]   = dry_l;
                    dryr_block[i]   = dry_r;
                    drysub_block[i] = dry_sub;
                }

                // Batched, not per-sample -- see NamStage::process_block.
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
            // Rocking button / keyboard 'a'/'s' relative cycle, or MIDI
            // Program Change absolute select (see hydra/mock.rs) -- either
            // way, just apply the resulting Voice's index (matches
            // VOICE_NAMES order in gui.rs).
            DeltaEvent::VoiceChange(voice) => self.voice_selected.set_value(*voice as u8 as f32),
            DeltaEvent::Panic()    => self.gate.set_value(GATE_OFF),
            _ => {},
        }
    }
}
