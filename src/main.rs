#![allow(dead_code, unused_imports, unused_variables)]

use std::io::{Read, stdout};
use std::thread::sleep;
use std::time::Duration;

use termion::screen::IntoAlternateScreen;

use midir::{MidiOutput, MidiOutputConnection};


mod sixense;
mod hydra;
mod zgicabra;
mod history;
mod ui;

use hydra::HydraState;
use zgicabra::Zgicabra;


const REFRESH_MS: Duration = Duration::from_millis(10);
const MIDI_DEVICE_NAME: &str = "Zgicabra";


//
// Main
//

fn main() {

    ui::header();


    //
    // Setup Phase
    //

    print!("Establishing MIDI connection... ");

    let output   = MidiOutput::new(MIDI_DEVICE_NAME).unwrap();
    let midi_ports = output.ports();
    let out_port = midi_ports[0].clone();
    //let out_port   = output.ports()[0].clone();
    let port_name  = output.port_name(&out_port).unwrap_or("Unknown".to_string());
    let connection = output.connect(&out_port, "midir-test").unwrap();

    println!("ok");


    let mut hydra_state = HydraState::new();

    let mut zgicabra    = Zgicabra::new();


    //
    // Hydra Setup
    //

    hydra::start(&mut hydra_state);

    let mut screen = stdout().into_alternate_screen().unwrap();

    loop {

        hydra::update(&mut hydra_state);

        zgicabra::update(&mut zgicabra, &hydra_state).unwrap();

        ui::draw_all(&mut screen, &port_name, &zgicabra, &hydra_state).unwrap();

        sleep(REFRESH_MS);

        if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
            break;
        }

    }

    println!("\nClosing connection");

    connection.close();

    hydra::stop(&mut hydra_state);
}

