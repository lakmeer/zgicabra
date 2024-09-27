
use std::fmt;


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


//
// MidiEvent
//
// Dispatch MIDI events with particular parameters
//

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
            lsb: (value >> 7)   as u8,
            msb: (value & 0x7F) as u8
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

    pub fn panic () -> MidiEvent {
        MidiEvent {
            msg: MSG_CONTROL_CHANGE,
            msb: CC_MIDI_PANIC,
            lsb: 0
        }
    }
}

impl fmt::Debug for MidiEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.msg {
            MSG_NOTE_ON        => write!(f, "Note On: {}@{}", self.msb, self.lsb),
            MSG_NOTE_OFF       => write!(f, "Note Off: {}", self.msb),
            MSG_CONTROL_CHANGE => write!(f, "Control Change: {} {}", self.msb, self.lsb),
            MSG_PROGRAM_CHANGE => write!(f, "Program Change: {}", self.msb),
            MSG_PITCH_BEND     => write!(f, "Pitch Bend: {} ({},{})", self.msb << 7 | self.lsb, self.msb, self.lsb),
            _                  => write!(f, "Unknown: {} {} {}", self.msg, self.msb, self.lsb)
        }
    }
}

