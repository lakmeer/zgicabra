#![allow(dead_code, unused_imports, unused_variables)]

use std::thread::sleep;
use std::time::Duration;
use std::env;

mod tools;
mod hydra;
mod zgicabra;
mod ui;
mod osc;
mod sc;
mod rs;
mod output;

use osc::OscOutput;
use sc::ScOutput;
use rs::RsOutput;
use output::DeltaConsumer;

use hydra::HydraState;
use zgicabra::{Zgicabra, DeltaEvent};

pub const HISTORY_WINDOW: usize = 100;

const REFRESH_MS: Duration = Duration::from_millis(10);
const DEVICE_NAME: &str = "Zgicabra";


//
// TODOs
//
// - Better debug output
// - Represent stick click on UI
// - CLI args:
//   - no-ui mode
//   - self-test mode
// - Double stickclick -> Panic
// - Proxy OSC heartbeat status to UI
//
// INVESTIGATE
// - Argent Compressor: https://www.youtube.com/watch?v=dqv3jC7GX6Y
// - Odin 2
// - Shreddage 3 Argent
// - Tonepusher Argent Metal preset pack (Serum) https://www.tonepusher.com/product-page/argent-metal
// - Small LCD display
//
// BUGS
// - Fix pitchbend accuracy
// - B4 crash
//


//
// Main
//

fn main() {

    println!("█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║▌│ zgicabra ▌▌│║▌║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│\n");

    let args = tools::parse_args();

    let mut hydra_state                    = HydraState::new();
    let mut zgicabra                       = Zgicabra::new();
    let mut history:       Vec<Zgicabra>   = Vec::with_capacity(HISTORY_WINDOW);
    let mut delta_events:  Vec<DeltaEvent> = Vec::new();
    let mut delta_history: Vec<DeltaEvent> = Vec::new();


    // Setup

    let mut output: Box<dyn DeltaConsumer> = match args.consumer {
        tools::Consumer::Sc  => Box::new(ScOutput::new(args.no_ui).unwrap_or_else(|e| panic!("║ 🟥 Failed to init SuperCollider backend: {e}"))),
        tools::Consumer::Osc => Box::new(OscOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init OSC connection: {e}"))),
        tools::Consumer::Rs  => Box::new(RsOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init native Rust audio backend: {e}"))),
    };
    output.panic(); // Kill any overrunning notes

    hydra::start(&mut hydra_state);
    history.push(zgicabra.clone()); // Fill first frame to allow initial derivatives

    // NOTE: Not required?
    //sleep(Duration::from_millis(1000));

    if !args.no_ui {
        print!("{}{}", termion::cursor::Hide, termion::clear::All);
    }

    println!("║ Running...");

    loop {
        // Collect and process new frame
        hydra::update(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state, &mut delta_events);

        // Draw UI
        if !args.no_ui {
            ui::draw_all(&zgicabra, &history);
            ui::draw_events(&delta_events, &delta_history);
            ui::draw_note_state(&zgicabra);
            ui::draw_graph(&history);
        }

        // Send continuous OSC commands
        output.handle_signal(&zgicabra.signal);

        // Copy current frame's deltas to history.
        for delta in delta_events.drain(..) {
            if args.no_ui { println!("- {:?}", delta); }
            output.handle_event(&delta);
            delta_history.push(delta);
        }
        delta_events.clear();

        if history.len() >= HISTORY_WINDOW {
            history.remove(0);
        }
        history.push(zgicabra.clone());

        sleep(REFRESH_MS);

        if hydra::should_quit(&mut hydra_state) {
            break;
        }
    }

    print!("║ Closing hydra connection... ");

    hydra::stop(&mut hydra_state);

    println!("ok");

}

