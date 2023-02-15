
use std::io::{Read, stdout};
use std::thread::sleep;
use std::time::Duration;

use termion;
use termion::screen::IntoAlternateScreen;

use midir;
use midir::{MidiOutput, MidiOutputConnection};


mod sixense;
mod hydra;
mod zgicabra;
mod history;
mod ui;

use hydra::{blank_frame};
use history::{History};


//
// Global Data Structures
//

pub struct ZgiState {
    frames: [History<sixense::ControllerFrame>; 2],
    conn_out: MidiOutputConnection,
    port_name: String,
    screen: termion::screen::AlternateScreen<std::io::Stdout>,
}


//
// Helpers
//

const REFRESH_MS: Duration = Duration::from_millis(10);
const NOTE_ON_MSG:  u8 = 0x90;
const NOTE_OFF_MSG: u8 = 0x80;

fn note_on (conn_out: &mut MidiOutputConnection, note: u8, vel: u8) {
    println!("Doot! {} ON", note);
    let _ = conn_out.send(&[NOTE_ON_MSG, note, vel]);
}

fn note_off (conn_out: &mut MidiOutputConnection, note: u8, vel: u8) {
    println!("_____ {} OFF", note);
    let _ = conn_out.send(&[NOTE_OFF_MSG, note, vel]);
}

fn midi_test(conn_out: &mut MidiOutputConnection) {
    println!("MIDI test...");
    const VELOCITY: u8 = 0x64;

    let mut play_note = |note: u8, duration: u64| {
        note_on(conn_out, note, VELOCITY);
        sleep(Duration::from_millis(duration * 150));
        note_off(conn_out, note, VELOCITY);
    };

    play_note(66, 4);
    play_note(65, 3);
    play_note(64, 2);
    play_note(63, 1);

    println!("...done");
}


//
// Main
//

fn main() {

    ui::header();

    // History test
    let mut buffer = History::new(5);

    for i in 0..10 {
        buffer.push(i);
        println!("Buffer: {:?}", buffer);
        println!("Last: {:?}", buffer.last());
    }


    //
    // MIDI Setup
    //

    print!("Establishing MIDI connection... ");

    let midi_out   = MidiOutput::new("Zgicabra").unwrap();
    let midi_ports = midi_out.ports();
    let out_port   = midi_ports[0].clone();
    let port_name  = midi_out.port_name(&out_port).unwrap_or("Unknown".to_string());

    println!("ok");


    //
    // Establish overall state
    //

    let mut temp = blank_frame();

    let mut state = ZgiState {
        screen: stdout().into_alternate_screen().unwrap(),
        frames: [History::new(10), History::new(10)],
        conn_out: midi_out.connect(&out_port, "midir-test").unwrap(),
        port_name: port_name,
    };

    midi_test(&mut state.conn_out);


    //
    // Hydra Setup
    //

    println!("Establishing Hydra connection... ");

    sixense::init();

    println!("Waiting for Hydra...");

    // Loop until we get a frame
    while temp.sequence_number == 0 {
        sixense::read_frame(0, &mut temp);
    }

    println!("First frame received");

    print!("{}", termion::clear::All);


    loop {

        // Read both hands, index by self-disclosed handedness (permits hand swapping)
        sixense::read_frame(0, &mut temp);
        state.frames[temp.which_hand as usize - 1].push(temp);

        sixense::read_frame(1, &mut temp);
        state.frames[temp.which_hand as usize - 1].push(temp);

        ui::draw_all(&mut state);

        sleep(REFRESH_MS);

        if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
            break;
        }
    }

    println!("\nClosing connection");

    state.conn_out.close();

    sixense::exit();

}

