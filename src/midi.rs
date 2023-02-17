
use std::time::Duration;
use std::thread::sleep;

use midir::{MidiOutput, MidiOutputConnection};


const NOTE_ON_MSG:  u8 = 0x90;
const NOTE_OFF_MSG: u8 = 0x80;


pub struct MidiState {
    output:     MidiOutput,
    connection: MidiOutputConnection,
    pub port_name:  String,
}

impl MidiState {

    // TODO: -> Result<MidiState, Error> ?
    pub fn new(device_name: &str) -> MidiState {
        let output   = MidiOutput::new(device_name).unwrap();
        let midi_ports = output.ports();
        let out_port = midi_ports[0].clone();
        //let out_port   = output.ports()[0].clone();
        let port_name  = output.port_name(&out_port).unwrap_or("Unknown".to_string());
        let connection = output.connect(&out_port, "midir-test").unwrap();

        MidiState {
            output,
            connection,
            port_name,
        }
    }

    pub fn test_sequence(&self) {
        println!("MIDI test...");

        let play_note = |note: u8| {
            self.note_on(note, 0x64);
            sleep(Duration::from_millis(150));
            self.note_off(note, 0x64);
            sleep(Duration::from_millis(150));
        };

        play_note(66);
        play_note(65);
        play_note(64);
        play_note(63);

        println!("...done");
    }

    pub fn note_on(&self, note: u8, vel: u8) {
        println!("MIDI::ON_ {}@{}", note, vel);
        let _ = self.connection.send(&[NOTE_ON_MSG, note, vel]);
    }

    pub fn note_off(&self, note: u8, vel: u8) {
        println!("MIDI::OFF {}@{}", note, vel);
        let _ = self.connection.send(&[NOTE_OFF_MSG, note, vel]);
    }

    pub fn close (self) {
        self.connection.close();
    }
}

