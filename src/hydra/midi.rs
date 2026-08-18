
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::tools::AtomicF32;
use crate::zgicabra::DeltaEvent;

#[derive(Clone)]
pub struct MidiState {
    pub bend:      Arc<AtomicF32>,
    pub connected: bool,
}

impl MidiState {
    fn inert() -> MidiState {
        MidiState {
            bend:      Arc::new(AtomicF32::new(0.0)),
            connected: false,
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod real {
    use super::*;

    use midir::{MidiInput, MidiInputConnection, Ignore};

    use crate::zgicabra::Voice;

    pub struct Connection(Option<MidiInputConnection<()>>);

    fn is_virtual_port (name: &str) -> bool {
        name.contains("Midi Through") || name.contains("VirMIDI") || name.contains("Virtual Raw MIDI")
    }

    // Connects to the first available MIDI input port, if any, storing
    // incoming CC1/pitch-bend values into the given atomics and pushing
    // Note On/Off as DeltaEvents onto `notes` (drained each tick by
    // hydra::take_midi_notes). Monophonic, last-note-priority like a single
    // wand trigger: a second Note On while one is held emits NoteChange
    // rather than a second NoteStart; Note Off only ends the note if it
    // matches the currently-held one. Returns None if no MIDI port is
    // available -- caller just proceeds without MIDI input.
    fn connect_midi (bend: Arc<AtomicF32>, notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> Option<MidiInputConnection<()>> {
        let mut midi_in = MidiInput::new("zgicabra").ok()?;
        midi_in.ignore(Ignore::None);

        let ports = midi_in.ports();
        // Skip ALSA's software-only ports in favor of a real device -- see
        // cc_input.rs's connect() for the same fix.
        let port = ports.iter()
            .find(|p| !is_virtual_port(&midi_in.port_name(p).unwrap_or_default()))
            .or_else(|| ports.first())?;
        let name = midi_in.port_name(port).unwrap_or_default();

        println!("Hydra::start - MIDI controller found: {name}");

        let mut held_note: Option<u8> = None;

        midi_in.connect(port, "zgicabra-midi-in", move |_stamp, message, _| {
            crate::dbg!("hydra::midi - raw message {:?}", message);
            match message {
                [status, lsb, msb] if status & 0xF0 == 0xE0 => {
                    let raw = ((*msb as u16) << 7) | *lsb as u16;
                    bend.store((raw as f32 - 8192.0) / 8192.0);
                },
                [status, note, velocity] if status & 0xF0 == 0x90 && *velocity > 0 => {
                    let event = match held_note {
                        Some(prev) => DeltaEvent::NoteChange(prev, *note),
                        None       => DeltaEvent::NoteStart(*note),
                    };
                    held_note = Some(*note);
                    crate::dbg!("hydra::midi - note on {note} vel {velocity} -> {:?}", event);
                    notes.lock().unwrap().push_back(event);
                },
                [status, note, _] if status & 0xF0 == 0x80 || (status & 0xF0 == 0x90) => {
                    crate::dbg!("hydra::midi - note off {note} (held_note={:?})", held_note);
                    if held_note == Some(*note) {
                        held_note = None;
                        notes.lock().unwrap().push_back(DeltaEvent::NoteEnd(*note));
                    }
                },
                // Program Change: absolute voice select (PC 0-3, one per
                // voice slot) instead of the rocking button/keyboard's
                // relative cycle().
                [status, program] if status & 0xF0 == 0xC0 => {
                    notes.lock().unwrap().push_back(DeltaEvent::VoiceChange(Voice::from_index(*program)));
                },
                _ => {},
            }
        }, ()).ok()
    }

    pub fn connect (notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> (MidiState, Connection) {
        let bend   = Arc::new(AtomicF32::new(0.0));

        let conn = connect_midi(bend.clone(), notes);
        let connected = conn.is_some();
        crate::dbg!("hydra::midi - connect() connected={connected}");
        if !connected {
            println!("Hydra::start - no MIDI controller found, proceeding without MIDI input.");
        }

        (MidiState { bend, connected }, Connection(conn))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod stub {
    use super::*;

    pub struct Connection;

    pub fn connect (_notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> (MidiState, Connection) {
        (MidiState::inert(), Connection)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use real::{connect, Connection};
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use stub::{connect, Connection};
