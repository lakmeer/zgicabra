
//
// MIDI controller supplement for the mock backend (see mock.rs) -- lets an
// external MIDI controller feed CC/pitch-bend/note input on the macOS dev
// machine, which has no real Hydra. The Linux performance machine's MusNix
// audio setup has no ALSA, so midir can't build there (see Cargo.toml's
// target-scoped dependency); non-macOS builds get the inert `stub` module
// below instead of `real`, so callers never need their own #[cfg].
//

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::tools::AtomicF32;
use crate::zgicabra::DeltaEvent;

// Latest CC 1-4 / pitch-bend values from the MIDI listener, read fresh each
// tick (a live "current value", not drain-on-read) and pushed onto
// ZgicabraBridge's SignalOverride each frame (see main.rs). rot_left/right
// are CC7/8, fed into wand_frame's rot_quat twist slot instead since they
// drive zgicabra's rotation->bend math rather than bypass it like bend does.
// Stays at its default (0.0 / false) if `connected` is false.
#[derive(Clone)]
pub struct MidiState {
    pub filter:    Arc<AtomicF32>,
    pub width:     Arc<AtomicF32>,
    pub fuzz:      Arc<AtomicF32>,
    pub thump:     Arc<AtomicF32>,
    pub bend:      Arc<AtomicF32>,
    pub rot_left:  Arc<AtomicF32>,
    pub rot_right: Arc<AtomicF32>,
    pub connected: bool,
}

impl MidiState {
    fn inert() -> MidiState {
        MidiState {
            filter:    Arc::new(AtomicF32::new(0.0)),
            width:     Arc::new(AtomicF32::new(0.0)),
            fuzz:      Arc::new(AtomicF32::new(0.0)),
            thump:     Arc::new(AtomicF32::new(0.0)),
            bend:      Arc::new(AtomicF32::new(0.0)),
            rot_left:  Arc::new(AtomicF32::new(0.0)),
            rot_right: Arc::new(AtomicF32::new(0.0)),
            connected: false,
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod real {
    use super::*;
    use std::f32::consts::PI;

    use midir::{MidiInput, MidiInputConnection, Ignore};

    use crate::zgicabra::Voice;

    const CC_FILTER:    u8 = 1;
    const CC_WIDTH:     u8 = 2;
    const CC_FUZZ:      u8 = 3;
    const CC_THUMP:     u8 = 4;
    const CC_ROT_LEFT:  u8 = 7;
    const CC_ROT_RIGHT: u8 = 8;
    // CC7/8's full sweep maxes out at a quarter turn (90 degrees) in either
    // direction, not a full -1..1 twist -- keeps the knob from being wildly
    // oversensitive vs. an actual wand twist.
    const TWIST_ANGLE_RANGE: f32 = PI / 2.0;

    // Holds the live connection alive; disconnects on drop. Opaque to
    // callers outside this module.
    pub struct Connection(Option<MidiInputConnection<()>>);

    // rot_quat[2] is a quaternion component (sin(angle/2) for rotation about
    // the twist axis), not the angle itself -- storing a raw fraction-of-a-
    // turn would be nonlinear-feeling since sin() isn't linear. gui.rs's
    // xy_pad inverts this via asin to get the angle back for display.
    fn twist_component (level: f32) -> f32 {
        let angle = (1.0 - level * 2.0) * TWIST_ANGLE_RANGE;
        (angle * 0.5).sin()
    }

    // Connects to the first available MIDI input port, if any, storing
    // incoming CC 1-4/7-8 / pitch-bend values into the given atomics and
    // pushing Note On/Off as DeltaEvents onto `notes` (drained each tick by
    // hydra::take_midi_notes). Monophonic, last-note-priority like a single
    // wand trigger: a second Note On while one is held emits NoteChange
    // rather than a second NoteStart; Note Off only ends the note if it
    // matches the currently-held one. Returns None if no MIDI port is
    // available -- caller just proceeds without MIDI input.
    fn connect_midi (filter: Arc<AtomicF32>, width: Arc<AtomicF32>, fuzz: Arc<AtomicF32>, thump: Arc<AtomicF32>, bend: Arc<AtomicF32>, rot_left: Arc<AtomicF32>, rot_right: Arc<AtomicF32>, notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> Option<MidiInputConnection<()>> {
        let mut midi_in = MidiInput::new("zgicabra").ok()?;
        midi_in.ignore(Ignore::None);

        let ports = midi_in.ports();
        let port = ports.first()?;
        let name = midi_in.port_name(port).unwrap_or_default();

        println!("Hydra::start - MIDI controller found: {name}");

        let mut held_note: Option<u8> = None;

        midi_in.connect(port, "zgicabra-midi-in", move |_stamp, message, _| {
            match message {
                [status, cc, value] if status & 0xF0 == 0xB0 => {
                    let level = *value as f32 / 127.0;
                    match *cc {
                        CC_FILTER => filter.store(level),
                        CC_WIDTH  => width.store(level),
                        CC_FUZZ   => fuzz.store(level),
                        CC_THUMP  => thump.store(level),
                        CC_ROT_LEFT  => rot_left.store(twist_component(level)),
                        CC_ROT_RIGHT => rot_right.store(twist_component(level)),
                        _ => {},
                    }
                },
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
                    notes.lock().unwrap().push_back(event);
                },
                [status, note, _] if status & 0xF0 == 0x80 || (status & 0xF0 == 0x90) => {
                    if held_note == Some(*note) {
                        held_note = None;
                        notes.lock().unwrap().push_back(DeltaEvent::NoteEnd(*note));
                    }
                },
                // Program Change: absolute voice select (PC 0-3, one per
                // voice slot -- see VOICE_NAMES in gui.rs) instead of the
                // rocking button/keyboard's relative cycle().
                [status, program] if status & 0xF0 == 0xC0 => {
                    notes.lock().unwrap().push_back(DeltaEvent::VoiceChange(Voice::from_index(*program)));
                },
                _ => {},
            }
        }, ()).ok()
    }

    pub fn connect (notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> (MidiState, Connection) {
        let filter    = Arc::new(AtomicF32::new(0.0));
        let width     = Arc::new(AtomicF32::new(0.0));
        let fuzz      = Arc::new(AtomicF32::new(0.0));
        let thump     = Arc::new(AtomicF32::new(0.0));
        let bend      = Arc::new(AtomicF32::new(0.0));
        let rot_left  = Arc::new(AtomicF32::new(0.0));
        let rot_right = Arc::new(AtomicF32::new(0.0));

        let conn = connect_midi(filter.clone(), width.clone(), fuzz.clone(), thump.clone(), bend.clone(), rot_left.clone(), rot_right.clone(), notes);
        let connected = conn.is_some();
        if !connected {
            println!("Hydra::start - no MIDI controller found, proceeding without MIDI input.");
        }

        (MidiState { filter, width, fuzz, thump, bend, rot_left, rot_right, connected }, Connection(conn))
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod stub {
    use super::*;

    pub struct Connection;

    pub fn connect (_notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> (MidiState, Connection) {
        (MidiState::inert(), Connection)
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub use real::{connect, Connection};
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub use stub::{connect, Connection};
