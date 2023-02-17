#![allow(dead_code, unused_imports, unused_variables)]

use std::io::{Read, stdout};
use std::thread::sleep;
use std::time::Duration;

use midir::{MidiOutput, MidiOutputConnection};


mod sixense;
mod hydra;
mod zgicabra;
mod history;
mod ui;

use hydra::HydraState;
use zgicabra::Zgicabra;
use history::History;


pub const HISTORY_WINDOW: usize = 50;

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

    let output     = MidiOutput::new(MIDI_DEVICE_NAME).unwrap();
    let out_port   = output.ports()[0].clone();
    let port_name  = output.port_name(&out_port).unwrap_or("Unknown".to_string());
    let connection = output.connect(&out_port, "midir-test").unwrap();

    println!("✅");


    let mut hydra_state = HydraState::new();
    let mut zgicabra    = Zgicabra::new();

    let mut history: History<Zgicabra> = History::new(HISTORY_WINDOW);


    //
    // Hydra Setup
    //

    hydra::start(&mut hydra_state);

    history.push(zgicabra.clone()); // Fill first frame

    print!("{}", termion::clear::All);

    loop {
        hydra::update(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state).unwrap();
        ui::draw_all(&zgicabra, &history).unwrap();

        history.push(zgicabra);

        sleep(REFRESH_MS);

        if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
            break;
        }
    }

    print!("Closing connection... ");
    connection.close();
    println!("ok");

    hydra::stop(&mut hydra_state);
}

