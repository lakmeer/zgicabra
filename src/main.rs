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
mod audio;
mod output;
mod gui;

use osc::OscOutput;
use audio::AudioOutput;
use output::DeltaConsumer;

use hydra::{HydraState, MockControls};
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

    let mut audio: Option<audio::AudioHandles> = None;

    let mut output: Box<dyn DeltaConsumer + Send> = match args.consumer {
        tools::Consumer::Osc   => Box::new(OscOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init OSC connection: {e}"))),
        tools::Consumer::Audio => {
            let audio_output = AudioOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init native audio backend: {e}"));
            audio = Some(audio_output.handles());
            Box::new(audio_output)
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
        let engine_mock_controls = mock_controls.clone();

        if args.test {
            match audio.clone() {
                Some(handles) => {
                    let test_quit = quit.clone();
                    std::thread::spawn(move || run_self_test(handles, test_quit));
                },
                None => eprintln!("║ 🟥 --test needs the audio backend (drop --osc)"),
            }
        }

        let engine_thread = std::thread::spawn(move || {
            run_engine_loop(args, hydra_state, output, engine_bridge, engine_quit, engine_mock_controls);
        });

        gui::run(audio, mock_controls, bridge, quit);
        engine_thread.join().expect("engine thread panicked");
    } else {
        run_engine_loop(args, hydra_state, output, bridge, Arc::new(AtomicBool::new(false)), mock_controls);
    }
}

// Self-test seam (--gui --test): launches the real gui-mode code path, holds
// a test note through the same audition-sequence mechanism the GUI's
// "Play Sequence" button uses, then taps ~0.1s of the raw cpal output buffer (see
// AudioCapture in audio/mod.rs) and checks it's non-zero. Diagnoses "no
// audio output" independent of the OS/device layer -- if this reports
// non-zero, the engine is producing signal and the bug is downstream (cpal
// device selection, OS routing, etc); if it reports all-zero, the bug is in
// the graph itself (note/gate wiring, envelope, a stuck bypass level, etc).
fn run_self_test (audio: audio::AudioHandles, quit: Arc<AtomicBool>) {
    println!("║ [selftest] waiting for audio stream to settle...");
    sleep(Duration::from_millis(300));

    println!("║ [selftest] holding test note (A4, 440Hz)...");
    audio.audition_seq.start(69); // A4 -- clearly audible on any speaker/headphone
    sleep(Duration::from_millis(50)); // let the envelope attack

    audio.capture.start();
    println!("║ [selftest] capturing 0.1s of cpal output...");
    while !audio.capture.is_full() {
        sleep(Duration::from_millis(5));
    }

    let samples = audio.capture.samples();

    let max_abs  = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    let nonzero  = samples.iter().any(|&s| s.abs() > 1e-6);

    println!("║ [selftest] captured {} samples, max |sample| = {max_abs:.6}", samples.len());
    if nonzero {
        println!("║ [selftest] ✅ PASS: cpal output buffer has signal.");
    } else {
        println!("║ [selftest] 🟥 FAIL: cpal output buffer is silent (all zero).");
    }

    // Rules out "too short/too quiet to notice" -- if the capture above
    // passed but nothing is audible for these 3 real seconds either, the
    // break is downstream of this process entirely (OS/device routing).
    println!("║ [selftest] 🔊 LISTEN NOW: holding an audible A4 tone for 3 seconds...");
    sleep(Duration::from_secs(3));
    audio.audition_seq.stop();

    sleep(Duration::from_millis(200));
    quit.store(true, Ordering::Relaxed);
}

// The hydra/zgicabra/audio loop that used to be all of main(). Runs on the
// main thread normally; runs on a background thread when --gui is set, since
// gui::run() then needs the main thread for itself (see main() above).
// `quit` is polled every frame in addition to hydra::should_quit() so
// closing the gui window (which sets it) stops this loop too.
fn run_engine_loop (args: tools::Args, mut hydra_state: HydraState, mut output: Box<dyn DeltaConsumer + Send>, bridge: ZgicabraBridge, quit: Arc<AtomicBool>, mock_controls: Option<MockControls>) {
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
        delta_events.extend(hydra::take_midi_notes(&mut hydra_state));

        if let Some(mc) = &mock_controls {
            if mc.midi_connected {
                bridge.filter.set(mc.midi_filter.load());
                bridge.width.set(mc.midi_width.load());
                bridge.fuzz.set(mc.midi_fuzz.load());
                bridge.thump.set(mc.midi_thump.load());
                bridge.bend.set(mc.midi_bend.load());
            }
        }

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

