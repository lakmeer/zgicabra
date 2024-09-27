
use std::time::Duration;
use std::thread::sleep;

use midir::{MidiOutput, MidiOutputConnection};

use crate::zgicabra::{Zgicabra,DeltaEvent};
use crate::midi_event::{MidiEvent};


// Custom MIDI CCs

const CC_CUTOFF:       u8 = 0x20;
const CC_FUZZ:         u8 = 0x21;
const CC_THUMP:        u8 = 0x22;
const CC_VELOCITY:     u8 = 0x23;
const CC_ACCELERATION: u8 = 0x24;
const CC_JERK:         u8 = 0x25;
const CC_BIGNESS:      u8 = 0x26;
const CC_WIDTH:        u8 = 0x27;

type Conn = MidiOutputConnection;



//
// Module Functions
//

pub fn update (zgicabra: &Zgicabra, delta_events: &Vec<DeltaEvent>, midi_events: &mut Vec<MidiEvent>) {

    // 'Always' events
    midi_events.push(MidiEvent::pitch_bend((zgicabra.note.bend * 8192.0 + 8192.0) as i16));
    midi_events.push(MidiEvent::control_change(CC_VELOCITY, (zgicabra.signal.velocity * 127.0) as u8));
    midi_events.push(MidiEvent::control_change(CC_CUTOFF, (zgicabra.signal.filter * 127.0) as u8));
    //midi_events.push(MidiEvent::control_change(CC_FUZZ, (lvl * 127.0) as u8));
    //midi_events.push(MidiEvent::control_change(CC_WIDTH, (lvl * 127.0) as u8));

    // Events Deltas
    for delta in delta_events.iter() {
        match delta {
            DeltaEvent::Panic() => {
                midi_events.push(MidiEvent::panic());
            },

            DeltaEvent::NoteStart(note) => {
                midi_events.push(MidiEvent::note_on(*note, 127));
            },

            DeltaEvent::NoteEnd(note) => {
                midi_events.push(MidiEvent::note_off(*note));
            },

            DeltaEvent::NoteChange(from, to) => {
                midi_events.push(MidiEvent::note_off(*from));
                midi_events.push(MidiEvent::note_on(*to, 127));
            },

            _ => {}
        }
    }
}

pub fn dispatch (midi_events: &Vec<MidiEvent>, conn: &mut Conn) {
    for event in midi_events {
        let result = conn.send(&[event.msg, event.msb, event.lsb]);
    }
}

pub fn clear (midi_events: &mut Vec<MidiEvent>, limit: usize) {
    let len = midi_events.len();

    if len > limit {
        *midi_events = midi_events.split_off(len - limit);
    }
}

pub fn close (conn: Conn) {
    // Taking ownership so we can destroy it
    conn.close();
}


