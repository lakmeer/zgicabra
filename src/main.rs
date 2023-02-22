#![allow(dead_code, unused_imports, unused_variables)]

use std::io::{Read, stdout};
use std::thread::sleep;
use std::time::Duration;

use midir::{MidiOutput, MidiOutputConnection};


mod sixense;
mod hydra;
mod zgicabra;
mod midi;
mod ui;
mod history;

use hydra::HydraState;
use zgicabra::Zgicabra;
use history::History;
use midi::MidiEvent;


pub const HISTORY_WINDOW: usize = 50;

const REFRESH_MS: Duration = Duration::from_millis(10);
const MIDI_DEVICE_NAME: &str = "Zgicabra";


//
// Main
//

fn main() {

    print!("{}{}{}", termion::clear::All, termion::cursor::Hide, termion::cursor::Goto(1,1));
    println!("█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│\n");


    //
    // Setup Phase
    //

    print!("Establishing MIDI connection... ");

    let output         = MidiOutput::new(MIDI_DEVICE_NAME).unwrap();
    let out_port       = output.ports()[0].clone();
    let port_name      = output.port_name(&out_port).unwrap_or("Unknown".to_string());
    let mut connection = output.connect(&out_port, "midir-test").unwrap();

    println!("✅");

    let mut hydra_state = HydraState::new();
    let mut zgicabra    = Zgicabra::new();
    let mut history: History<Zgicabra> = History::new(HISTORY_WINDOW);
    let mut midi_events: Vec<MidiEvent> = Vec::new();


    //
    // Hydra Setup
    //

    hydra::start(&mut hydra_state);

    sleep(Duration::from_millis(1000));

    history.push(zgicabra.clone()); // Fill first frame to allow initial derivatives

    print!("{}", termion::clear::All);

    loop {
        hydra::update(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state).unwrap();

        midi::update(&zgicabra, &mut midi_events);
        midi::dispatch(&midi_events, &mut connection);

        ui::draw_all(&zgicabra, &history);
        ui::draw_events(&zgicabra.deltas, &midi_events);

        zgicabra.deltas.clear();
        midi::clear(&mut midi_events);

        history.push(zgicabra.clone());

        sleep(REFRESH_MS);

        if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
            break;
        }
    }

    hydra::stop(&mut hydra_state);

    print!("Closing connection... ");

    midi::close(connection);

    println!("ok");

}

