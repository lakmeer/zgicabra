
use std::io::{Write, Read, stdout};

use termion;
use termion::screen::IntoAlternateScreen;

use midir;

mod sixense;
mod utils;

use sixense::HydraFrame;
use utils::format_frame;



const BANNER_TEXT:&str = "█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";




//
// Helpers
//

fn new_blank_frame() -> HydraFrame {
    HydraFrame {
        pos: [0.0, 0.0, 0.0],
        rot_mat: [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        joystick_x: 0.0,
        joystick_y: 0.0,
        trigger: 0.0,
        buttons: 0,
        sequence_number: 0,
        rot_quat: [0.0, 0.0, 0.0, 0.0],
        firmware_revision: 0,
        hardware_revision: 0,
        packet_type: 0,
        magnetic_frequency: 0,
        enabled: 0,
        controller_index: 0,
        is_docked: 0,
        which_hand: 0,
        hemi_tracking_enabled: 0,
    }
}




use std::thread::sleep;
use std::time::Duration;

use midir::{MidiOutput, MidiOutputConnection};

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
    print!("{}{}{}{}", termion::clear::All, termion::cursor::Hide, termion::cursor::Goto(1,1), BANNER_TEXT);
    print!("{}", termion::cursor::Goto(1,3));


    //
    // MIDI Setup
    //

    print!("Establishing MIDI connection... ");

    let midi_out = MidiOutput::new("Zgicabra").unwrap();
    let midi_ports = midi_out.ports();
    let out_port = midi_ports[0].clone();
    let port_name = midi_out.port_name(&out_port).unwrap_or("Unknown".to_string());
    let mut conn_out = midi_out.connect(&out_port, "midir-test").unwrap();

    println!("ok");

    midi_test(&mut conn_out);

    // Array of 2 frames
    let mut temp = new_blank_frame();
    let mut frames = [new_blank_frame(), new_blank_frame()];


    //
    // Hydra Setup
    //

    unsafe {
        println!("Establishing Hydra connection... ");

        sixense::init();

        println!("Waiting for Hydra...");

        // Loop until we get a frame
        while temp.sequence_number == 0 {
            sixense::sixenseGetNewestData(0, &mut temp);
        }

        println!("First frame received");

        let mut screen = stdout().into_alternate_screen().unwrap();

        loop {

            // Read both hands, index by self-disclosed handedness (permits hand swapping)
            sixense::read_frame(0, &mut temp);
            frames[temp.which_hand as usize - 1] = temp;

            sixense::read_frame(1, &mut temp);
            frames[temp.which_hand as usize - 1] = temp;

            write!(screen, "{}{}", termion::cursor::Goto(1,1), BANNER_TEXT).unwrap();
            write!(screen, "{}| MIDI Port: '{}'", termion::cursor::Goto(1,3), port_name).unwrap();

            write!(screen, "{}L> {}", termion::cursor::Goto(1,5), format_frame(frames[0])).unwrap();
            write!(screen, "{}R> {}", termion::cursor::Goto(1,6), format_frame(frames[1])).unwrap();

            screen.flush().unwrap();

            sleep(Duration::from_millis(10));

            if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
                break;
            }
        }


        sleep(Duration::from_millis(150));
        println!("\nClosing connection");
        conn_out.close();

        sixense::exit();
    }
}

