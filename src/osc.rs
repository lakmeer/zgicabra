
use rosc::encoder;
use rosc::{OscMessage, OscPacket, OscType};
use std::net::{SocketAddrV4, UdpSocket};
use std::str::FromStr;
use std::{env, f32, thread};
use std::io::{Result};

use crate::zgicabra::{DeltaEvent,SignalState};
use crate::hydra::HydraState;
use crate::output::DeltaConsumer;

const HOST_ADDR:&str = "127.0.0.1:0"; // Port 0 = arbitrary free port
const TO_ADDR:&str   = "127.0.0.1:8000"; // Default port for OSC Bitwig plugin (DrivenByMoss)

// DrivenByMoss Extension must be set accordingly:
// Settings -> Controllers -> OSC -> Protocol -> Value Resolution
const MAX_VALUE:i32 = 16384;

fn midi_max (val: f32) -> i32 {
    (val.clamp(0.0, 1.0) * 127 as f32) as i32
}

fn osc_max (val: f32) -> i32 {
    (val.clamp(0.0, 1.0) * MAX_VALUE as f32) as i32
}


pub struct OscOutput {
    socket: UdpSocket,
}

impl OscOutput {
    pub fn new () -> Result<OscOutput> {
        println!("║ Obtaining OSC connection... ");
        let socket = UdpSocket::bind(HOST_ADDR)?;
        socket.connect(TO_ADDR).unwrap();
        println!("║ OSC connection OK.");
        Ok(OscOutput { socket })
    }

    pub fn send (&self, addr: &str, args: &[i32]) {
        for &arg in args {
            assert!((arg >= 0 && arg <= MAX_VALUE), "OSC arg out of range 0-127: {arg} (addr: {addr})");
        }

        let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args: args.iter().map(|&x| OscType::Int(x)).collect(),
        })).unwrap();

        self.socket.send(&msg_buf).unwrap();
    }

    pub fn trigger (&self, addr: &str) {
        self.send(addr, &[]);
    }

    pub fn handle_signal (&self, signal: &SignalState) {
        self.send("/vkb_midi/1/pitchbend", &[midi_max(signal.bend)]);
        self.send("/device/param/1/value", &[osc_max(signal.filter)]);
        self.send("/device/param/2/value", &[osc_max(signal.fuzz)]);
        self.send("/device/param/3/value", &[osc_max(signal.width)]);
        self.send("/device/param/4/value", &[osc_max(signal.thump)]);
    }

    pub fn handle_event (&self, delta: &DeltaEvent) {
        match delta {
            DeltaEvent::NoteStart(note) => {
                self.send("/vkb_midi/1/note", &[*note as i32, 127]);
            },
            DeltaEvent::NoteChange(note, new_note) => {
                self.send("/vkb_midi/1/note", &[*note as i32, 0]);
                self.send("/vkb_midi/1/note", &[*new_note as i32, 127]);
            },
            DeltaEvent::NoteEnd(note) => {
                self.send("/vkb_midi/1/note", &[*note as i32, 0]);
            },
            DeltaEvent::Panic() => {
                self.panic();
            },
            _ => {},
        }
    }


    //
    // Event functions
    //

    pub fn note_on (&self, note: u8){
    }

    pub fn note_off (&self, note: u8){
    }

    pub fn panic (&self) {
        for i in 0..128 {
            self.send("/vkb_midi/1/note", &[i, 0]);
        }
    }

}

impl DeltaConsumer for OscOutput {
    fn panic (&mut self) { OscOutput::panic(self); }
    fn handle_signal (&mut self, signal: &SignalState) { OscOutput::handle_signal(self, signal); }
    fn handle_event (&mut self, delta: &DeltaEvent) { OscOutput::handle_event(self, delta); }
}

