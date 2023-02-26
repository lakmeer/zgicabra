
use std::time::Duration;
use std::thread::sleep;
use std::fmt;

use midir::{MidiOutput, MidiOutputConnection};

use crate::zgicabra::{Zgicabra,DeltaEvent};


// MIDI Message Types
const MSG_NOTE_ON:        u8 = 0x90;
const MSG_NOTE_OFF:       u8 = 0x80;
const MSG_CONTROL_CHANGE: u8 = 0xB0;
const MSG_PROGRAM_CHANGE: u8 = 0xC0;
const MSG_PITCH_BEND:     u8 = 0xE0;

// Default MIDI CCs
const CC_MOD_WHEEL:       u8 = 0x01;
const CC_PORTAMENTO_RATE: u8 = 0x05;
const CC_MIDI_PANIC:      u8 = 0x7B;
const CC_SILENCE:         u8 = 0x78;

// Custom MIDI CCs
const CC_CUTOFF:          u8 = 0x20;
const CC_WIDTH:           u8 = 0x20;
const CC_FUZZ:            u8 = 0x21;
const CC_THUMP:           u8 = 0x22;
const CC_VELOCITY:        u8 = 0x23;
const CC_ACCELERATION:    u8 = 0x24;
const CC_JERK:            u8 = 0x25;
const CC_BIGNESS:         u8 = 0x26;


type Conn = MidiOutputConnection;

pub struct MidiEvent {
    pub msg: u8,
    pub msb: u8,
    pub lsb: u8
}

impl MidiEvent {
    pub fn note_on  (note: u8, vel: u8) -> MidiEvent {
        MidiEvent {
            msg: MSG_NOTE_ON,
            msb: note,
            lsb: vel
        }
    }

    pub fn note_off (note: u8) -> MidiEvent {
        MidiEvent {
            msg: MSG_NOTE_OFF,
            msb: note,
            lsb: 0
        }
    }

    pub fn pitch_bend (value: i16) -> MidiEvent {
        MidiEvent {
            msg: MSG_PITCH_BEND,
            msb: (value >> 7)   as u8,
            lsb: (value & 0x7F) as u8
        }
    }

    pub fn control_change (cc: u8, value: u8) -> MidiEvent {
        MidiEvent {
            msg: MSG_CONTROL_CHANGE,
            msb: cc,
            lsb: value
        }
    }

    pub fn program_change (program: u8) -> MidiEvent {
        MidiEvent {
            msg: MSG_PROGRAM_CHANGE,
            msb: program,
            lsb: 0
        }
    }
}

impl fmt::Debug for MidiEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.msg {
            MSG_NOTE_ON        => write!(f, "Note On: {} {}", self.msb, self.lsb),
            MSG_NOTE_OFF       => write!(f, "Note Off: {}", self.msb),
            MSG_CONTROL_CHANGE => write!(f, "Control Change: {} {}", self.msb, self.lsb),
            MSG_PROGRAM_CHANGE => write!(f, "Program Change: {}", self.msb),
            MSG_PITCH_BEND     => write!(f, "Pitch Bend: {}", self.msb << 7 | self.lsb),
            _                  => write!(f, "Unknown: {} {} {}", self.msg, self.msb, self.lsb)
        }
    }
}


//
// Module Functions
//

pub fn update (zgicabra: &Zgicabra, midi_events: &mut Vec<MidiEvent>) {

    midi_events.push(MidiEvent::pitch_bend((zgicabra.bend * 8192.0) as i16));

    midi_events.push(MidiEvent::control_change(CC_CUTOFF, (zgicabra.left.pitch * 127.0) as u8));
    midi_events.push(MidiEvent::control_change(CC_WIDTH, (zgicabra.right.pitch * 127.0) as u8));

    for delta in zgicabra.deltas.iter() {
        match delta {
            DeltaEvent::TriggerStart(hand) => {
                midi_events.push(MidiEvent::note_on(60, 127));
            },
            DeltaEvent::TriggerEnd(hand) => {
                midi_events.push(MidiEvent::note_off(60));
            },
            DeltaEvent::ButtonDown(hand, btn) => {
                midi_events.push(MidiEvent::control_change(CC_MOD_WHEEL, 127));
            },
            DeltaEvent::ButtonUp(hand, btn) => {
                midi_events.push(MidiEvent::control_change(CC_MOD_WHEEL, 127));
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

pub fn clear (midi_events: &mut Vec<MidiEvent>) {
    midi_events.clear();
}

pub fn close (conn: Conn) {
    // Taking ownership so we can destroy it
    conn.close();
}


