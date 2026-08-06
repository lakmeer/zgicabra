#![allow(dead_code, unused_imports, unused_variables)]

use std::thread::sleep;
use std::time::Duration;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

mod tools;
mod hydra;
mod zgicabra;
mod ui;
mod osc;
mod sc;
mod rs;
mod output;
mod gui;

use osc::OscOutput;
use sc::ScOutput;
use rs::RsOutput;
use output::DeltaConsumer;

use hydra::HydraState;
use zgicabra::{Zgicabra, DeltaEvent, ZgicabraBridge};

pub const HISTORY_WINDOW: usize = 100;

const REFRESH_MS: Duration = Duration::from_millis(10);
const DEVICE_NAME: &str = "Zgicabra";


//
// TODOs
//
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
//


//
// Main
//

fn main() {

    println!("█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║▌│ zgicabra ▌▌│║▌║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│\n");

    let args = tools::parse_args();

    let mut hydra_state = HydraState::new();


    // Setup

    let mut voice_params: Option<Arc<rs::VoiceParams>> = None;
    let mut nam_models: Option<rs::NamModelCycler> = None;

    let mut output: Box<dyn DeltaConsumer + Send> = match args.consumer {
        tools::Consumer::Sc  => Box::new(ScOutput::new(args.no_ui).unwrap_or_else(|e| panic!("║ 🟥 Failed to init SuperCollider backend: {e}"))),
        tools::Consumer::Osc => Box::new(OscOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init OSC connection: {e}"))),
        tools::Consumer::Rs  => {
            let rs_output = RsOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init native Rust audio backend: {e}"));
            voice_params = Some(rs_output.voice_params());
            nam_models = Some(rs_output.nam_models());
            Box::new(rs_output)
        },
    };
    output.panic(); // Kill any overrunning notes

    hydra::start(&mut hydra_state);

    // Only the mock backend (no real Hydra attached) has keyboard-driven
    // inputs a gui panel can also drive; None on real hardware.
    let mock_controls = hydra::mock_controls(&hydra_state);

    // Shared handle between the engine loop and the gui for the Zgicabra
    // section (per-wand rotation telemetry, signal-state overrides). Cheap
    // to keep alive even without --gui.
    let bridge = ZgicabraBridge::new();

    if args.gui {
        // winit/AppKit requires the window + event loop on the main thread
        // on macOS, so the gui owns main() here and the rest of the app
        // (hydra/zgicabra/audio loop, previously all of main()) moves to a
        // background thread instead.
        let quit = Arc::new(AtomicBool::new(false));
        let engine_quit = quit.clone();
        let engine_bridge = bridge.clone();

        let engine_thread = std::thread::spawn(move || {
            run_engine_loop(args, hydra_state, output, engine_bridge, engine_quit);
        });

        gui::run(voice_params, nam_models, mock_controls, bridge, quit);
        engine_thread.join().expect("engine thread panicked");
    } else {
        run_engine_loop(args, hydra_state, output, bridge, Arc::new(AtomicBool::new(false)));
    }
}

// The hydra/zgicabra/audio loop that used to be all of main(). Runs on the
// main thread normally; runs on a background thread when --gui is set, since
// gui::run() then needs the main thread for itself (see main() above).
// `quit` is polled every frame in addition to hydra::should_quit() so
// closing the gui window (which sets it) stops this loop too.
fn run_engine_loop (args: tools::Args, mut hydra_state: HydraState, mut output: Box<dyn DeltaConsumer + Send>, bridge: ZgicabraBridge, quit: Arc<AtomicBool>) {
    let no_ui = args.no_ui || args.gui;

    let mut zgicabra                       = Zgicabra::new();
    let mut history:       Vec<Zgicabra>   = Vec::with_capacity(HISTORY_WINDOW);
    let mut delta_events:  Vec<DeltaEvent> = Vec::new();
    let mut delta_history: Vec<DeltaEvent> = Vec::new();

    history.push(zgicabra.clone()); // Fill first frame to allow initial derivatives

    // NOTE: Not required?
    //sleep(Duration::from_millis(1000));

    if !no_ui {
        print!("{}{}", termion::cursor::Hide, termion::clear::All);
    }

    println!("║ Running...");

    loop {
        // Collect and process new frame
        hydra::update(&mut hydra_state);
        let voice_cycle = hydra::take_voice_cycle(&mut hydra_state);
        let tune_cycle  = hydra::take_tune_cycle(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state, voice_cycle, tune_cycle, &mut delta_events);
        bridge.sync(&mut zgicabra);

        // Draw UI
        if !no_ui {
            ui::draw_all(&zgicabra, &history);
            ui::draw_events(&delta_events, &delta_history);
            ui::draw_note_state(&zgicabra);
            ui::draw_graph(&history);
        }

        // Send continuous OSC commands
        output.handle_signal(&zgicabra.signal);

        // Copy current frame's deltas to history.
        for delta in delta_events.drain(..) {
            if no_ui { println!("- {:?}", delta); }
            output.handle_event(&delta);
            delta_history.push(delta);
        }
        delta_events.clear();

        if history.len() >= HISTORY_WINDOW {
            history.remove(0);
        }
        history.push(zgicabra.clone());

        sleep(REFRESH_MS);

        if hydra::should_quit(&mut hydra_state) || quit.load(Ordering::Relaxed) {
            break;
        }
    }

    print!("║ Closing hydra connection... ");

    hydra::stop(&mut hydra_state);

    println!("ok");
}

