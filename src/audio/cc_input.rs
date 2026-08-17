
//
// Second, independent MIDI connection dedicated to per-voice live tuning --
// separate from hydra/midi.rs's note/CC/pitch-bend simulation path (that one
// keeps feeding the global performance signals, unmodified). This one only
// looks at CC messages (status & 0xF0 == 0xB0) and hands them to the audio
// thread through a wait-free SPSC ring buffer (rtrb) rather than the
// Arc<Mutex<..>> hydra/midi.rs uses -- a Mutex inside a real-time audio
// callback risks priority inversion/glitches, so this path avoids locking
// entirely on both ends.
//
// See voice.rs for the CC number registry consumed by Voice::apply_cc.
//

use rtrb::{RingBuffer, Consumer};

const CC_RING_CAPACITY: usize = 256;

// Owns the ring buffer's consumer half and (on macOS/Linux) the live midir
// connection -- dropping this disconnects it. `pop` is called once per
// cpal callback block from mod.rs's build_stream, right next to the
// existing per-block on_block_start dispatch.
pub struct CcInput {
    consumer: Consumer<(u8, f32)>,
    _backend: Backend,
}

impl CcInput {
    pub fn connect () -> CcInput {
        let (producer, consumer) = RingBuffer::<(u8, f32)>::new(CC_RING_CAPACITY);
        let backend = backend::connect(producer);
        CcInput { consumer, _backend: backend }
    }

    // Non-blocking: returns the next queued (cc, value) if any, or None if
    // the queue is empty (or no controller is connected at all).
    pub fn pop (&mut self) -> Option<(u8, f32)> {
        self.consumer.pop().ok()
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod backend {
    use super::*;
    use rtrb::Producer;
    use midir::{MidiInput, MidiInputConnection, Ignore};

    // Holds the live connection alive; disconnects on drop. None if no
    // port was found at connect time -- caller just never receives CC
    // events, same no-op-safe fallback as hydra/midi.rs.
    pub struct Real(Option<MidiInputConnection<()>>);

    // True for ALSA's software-only ports (the "Midi Through" loopback, and
    // "VirMIDI" clients from the snd-virmidi kernel module) -- never a real
    // attached controller.
    fn is_virtual_port (name: &str) -> bool {
        name.contains("Midi Through") || name.contains("VirMIDI") || name.contains("Virtual Raw MIDI")
    }

    pub fn connect (mut producer: Producer<(u8, f32)>) -> Real {
        let connection = (|| {
            let mut midi_in = MidiInput::new("zgicabra-cc").ok()?;
            midi_in.ignore(Ignore::None);

            let ports = midi_in.ports();
            // ports().first() would happily grab one of ALSA's virtual ports
            // (the "Midi Through" loopback, or a "VirMIDI" client from the
            // snd-virmidi kernel module) over an actual attached controller,
            // depending on enumeration order -- skip those in favor of a
            // real device; only fall back to a virtual port if nothing else
            // is present.
            let port = ports.iter()
                .find(|p| !is_virtual_port(&midi_in.port_name(p).unwrap_or_default()))
                .or_else(|| ports.first())?;
            let name = midi_in.port_name(port).unwrap_or_default();
            println!("║ CC MIDI controller found: {name}");

            midi_in.connect(port, "zgicabra-cc-in", move |_stamp, message, _| {
                crate::dbg!("cc_input - raw message {:?}", message);
                if let [status, cc, value] = *message {
                    if status & 0xF0 == 0xB0 {
                        let level = value as f32 / 127.0;
                        crate::dbg!("cc_input - CC {cc} = {level}, pushing to ring buffer");
                        // Drop-on-full is correct here -- these are advisory
                        // current-knob-positions, not events needing
                        // guaranteed delivery.
                        if producer.push((cc, level)).is_err() {
                            crate::dbg!("cc_input - ring buffer full, dropped CC {cc}");
                        }
                    }
                }
            }, ()).ok()
        })();

        if connection.is_none() {
            println!("║ CC MIDI: no controller found, proceeding without CC input.");
        }

        Real(connection)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod backend {
    use super::*;
    use rtrb::Producer;

    pub struct Real;

    pub fn connect (_producer: Producer<(u8, f32)>) -> Real {
        println!("║ CC MIDI: unsupported platform, proceeding without CC input.");
        Real
    }
}

type Backend = backend::Real;
