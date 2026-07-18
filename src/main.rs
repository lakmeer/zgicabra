#![allow(dead_code, unused_imports, unused_variables)]

use std::io::{Read, stdout};
use std::thread::sleep;
use std::time::Duration;

mod tools;
mod hydra;
mod zgicabra;
mod ui;
mod osc;

use osc::{OscOutput};

use hydra::HydraState;
use zgicabra::{Zgicabra, DeltaEvent};

pub const HISTORY_WINDOW: usize = 100;

const REFRESH_MS: Duration = Duration::from_millis(10);
const DEVICE_NAME: &str = "Zgicabra";

const OSC_TEST_SINE: bool = false;


/*
 * TODOs
 *
 * - Better debug output
 * - Represent stick click on UI
 *
 * BUGS
 *
 * - Fix pitchbend accuracy
 * - Don't draw anything when either wand is docked
 *
**/


//
// Main
//

fn main() {


    //
    // Setup Phase
    //

    print!("{}{}", termion::clear::All, termion::cursor::Goto(1,1));
    println!("█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║▌│ zgicabra ▌▌│║▌║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│\n");


    let mut hydra_state = HydraState::new();
    let mut zgicabra    = Zgicabra::new();
    let mut history:      Vec<Zgicabra>   = Vec::with_capacity(HISTORY_WINDOW);
    let mut delta_events: Vec<DeltaEvent> = Vec::new();
    let mut delta_history: Vec<DeltaEvent> = Vec::new();


    //
    // Setup
    //

    println!("Obtaining OSC connection... ");

    let output = OscOutput::new().unwrap_or_else(|e| panic!("failed to init OSC connection: {e}"));
    output.panic();

    println!("OSC connection OK.");

    hydra::start(&mut hydra_state);

    sleep(Duration::from_millis(1000));

    history.push(zgicabra.clone()); // Fill first frame to allow initial derivatives

    print!("{}{}", termion::cursor::Hide, termion::clear::All);

    loop {
        hydra::update(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state, &mut delta_events);

        ui::draw_all(&zgicabra, &history);
        ui::draw_events(&delta_events, &delta_history);
        ui::draw_note_state(&zgicabra);
        ui::draw_graph(&history);

        output.handle_signal(&zgicabra.signal);

        // Copy current frame's deltas to history. Don''t clear history, so that we can
        // draw the last frame's deltas on top of the current frame.
        for delta in delta_events.drain(..) {
            output.handle_event(&delta);
            delta_history.push(delta);
        }
        delta_events.clear();

        if history.len() >= HISTORY_WINDOW {
            history.remove(0);
        }
        history.push(zgicabra.clone());

        sleep(REFRESH_MS);

        if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
            break;
        }
    }

    hydra::stop(&mut hydra_state);

    print!("Closing connection... ");

    println!("ok");

}

