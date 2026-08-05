
//
// RS
//
// Native Rust audio backend: a `fundsp` signal graph rendered straight to the
// system's default output device via `cpal` -- no subprocess, no bridge
// language. Named to mirror `sc.rs`/`osc.rs` and the `--rs` flag that selects
// it.
//
// Reproduces sc/main.scd's proof-of-concept patch term-for-term: one
// persistent saw voice through a resonant lowpass, gated by an ASR envelope.
// Control values (frequency, gate, filter cutoff) live in `fundsp::Shared`
// atomic cells, which is fundsp's own idiom for driving a running audio graph
// from another thread with no locks/allocation in the audio callback --
// filling the same role `ScOutput`'s stdin messages play for its separate
// sclang process, just in-process.
//

use std::io;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::prelude64::*;

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

pub struct RsOutput {
    freq:   Shared,
    gate:   Shared,
    cutoff: Shared,
    stream: cpal::Stream, // kept alive to keep audio playing; dropping RsOutput stops it
}

impl RsOutput {
    pub fn new () -> io::Result<RsOutput> {
        println!("║ Starting native Rust audio backend... ");

        let freq   = shared(110.0);
        let gate   = shared(GATE_OFF);
        let cutoff = shared(4000.0);

        let mut voice = build_voice(&freq, &gate, &cutoff);

        // -- NAM insertion seam ---------------------------------------------
        // A future NAM (Neural Amp Modeler) stage would compose here, e.g.
        // `voice = Box::new(voice >> nam_node)` once nam-rs's buffer-based
        // inference is wrapped in a small custom AudioNode. Nothing below
        // this point -- cpal setup, the DeltaConsumer impl -- needs to change
        // for that; this is the only seam that would.
        // ---------------------------------------------------------------------

        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no default audio output device"))?;
        let supported = device.default_output_config()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("no usable output config: {e}")))?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        voice.set_sample_rate(config.sample_rate as f64);

        let err_fn = |e| eprintln!("║ 🟥 Audio stream error: {e}");

        let build_result = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, voice, err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, voice, err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, voice, err_fn),
            other => return Err(io::Error::new(io::ErrorKind::Other, format!("unsupported sample format: {other:?}"))),
        };

        let stream = build_result
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build audio stream: {e}")))?;

        stream.play()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to start audio stream: {e}")))?;

        println!("║ Native Rust audio backend OK.");

        Ok(RsOutput { freq, gate, cutoff, stream })
    }
}

// Oscillator -> resonant lowpass -> envelope -> amp, matching sc/main.scd's
// Saw.ar -> RLPF.ar -> (* env * amp) chain. Kept as its own function so the
// NAM seam in `new()` above has a single, clearly-typed thing to compose with
// later.
fn build_voice (freq: &Shared, gate: &Shared, cutoff: &Shared) -> Box<dyn AudioUnit> {
    let osc = var(freq) >> saw();
    let env = var(gate) >> adsr_live(ATTACK, 0.0, 1.0, RELEASE);
    let filtered = (osc * env | var(cutoff)) >> lowpass_q(FILTER_Q);
    Box::new(filtered * AMP)
}

fn build_stream<T> (
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut voice: Box<dyn AudioUnit>,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;

    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let (l, r) = voice.get_stereo();
                for (i, sample) in frame.iter_mut().enumerate() {
                    *sample = T::from_sample(if i % 2 == 0 { l } else { r });
                }
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
            _ => {},
        }
    }
}
