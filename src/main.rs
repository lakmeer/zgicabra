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
mod audio;
mod gui;

use audio::AudioOutput;

use hydra::{HydraState, MockControls};
use zgicabra::{Zgicabra, DeltaEvent, ZgicabraBridge};

pub const HISTORY_WINDOW: usize = 100;

const REFRESH_MS: Duration = Duration::from_millis(10);
const DEVICE_NAME: &str = "Zgicabra";


//
// TODOs
//
// - Represent stick click on UI
// - self-test mode
//
// INVESTIGATE
// - Argent Compressor: https://www.youtube.com/watch?v=dqv3jC7GX6Y
// - Odin 2
// - Shreddage 3 Argent
// - Tonepusher Argent Metal preset pack (Serum) https://www.tonepusher.com/product-page/argent-metal
// - Small LCD display
//
// BUGS
// - Bend downwards
// - Fix pitchbend accuracy
//


//
// Main
//

fn main() {

    println!("█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║▌│ zgicabra ▌▌│║▌║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│\n");

    let args = tools::parse_args();

    let mut hydra_state = HydraState::new();
    let mut output = AudioOutput::new().unwrap_or_else(|e| panic!("║ 🟥 Failed to init audio backend: {e}"));

    output.panic(); // Kill any overrunning notes

    let audio = output.handles();

    hydra::start(&mut hydra_state);

    // None on real hardware -- only the mock backend has keyboard-driven
    // inputs a gui panel can also drive.
    let mock_controls = hydra::mock_controls(&hydra_state);

    // Shared between the engine loop and the gui (wand telemetry, signal
    // overrides); cheap to keep alive even without --gui.
    let bridge = ZgicabraBridge::new();

    if args.gui {
        // SDL2/AppKit requires the window + event loop on the main thread on
        // macOS, so gui owns main() here and the engine loop runs in the background.
        let quit = Arc::new(AtomicBool::new(false));
        let engine_quit = quit.clone();
        let engine_bridge = bridge.clone();
        let engine_mock_controls = mock_controls.clone();

        if args.test {
            let test_quit = quit.clone();
            let test_handles = audio.clone();
            std::thread::spawn(move || run_self_test(test_handles, test_quit));
        }

        let engine_audio = audio.clone();
        let engine_thread = std::thread::spawn(move || {
            run_engine_loop(args, hydra_state, output, engine_bridge, engine_quit, engine_mock_controls, engine_audio);
        });

        gui::run(Some(audio), mock_controls, bridge, quit);
        engine_thread.join().expect("engine thread panicked");
    } else {
        run_engine_loop(args, hydra_state, output, bridge, Arc::new(AtomicBool::new(false)), mock_controls, audio);
    }
}

fn run_self_test (audio: audio::AudioHandles, quit: Arc<AtomicBool>) {
    println!("║ [selftest] waiting for audio stream to settle...");
    sleep(Duration::from_millis(300));

    println!("║ [selftest] holding test note (A4, 440Hz)...");
    audio.test_tone.start(69);
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

    println!("║ [selftest] 🔊 LISTEN NOW: holding an audible A4 tone for 3 seconds...");
    sleep(Duration::from_secs(3));
    audio.test_tone.stop();

    sleep(Duration::from_millis(200));
    quit.store(true, Ordering::Relaxed);
}

// Runs on the main thread normally, or a background thread when --gui needs
// the main thread for itself. `quit` (set by closing the gui window) is
// polled alongside hydra::should_quit() to stop the loop either way.
fn run_engine_loop (
    args: tools::Args,
    mut hydra_state: HydraState,
    mut output: AudioOutput,
    bridge: ZgicabraBridge, 
    quit: Arc<AtomicBool>,
    mock_controls: Option<MockControls>,
    audio: audio::AudioHandles
) {

    let no_ui = args.debug || args.gui;

    let mut zgicabra                       = Zgicabra::new();
    let mut history:       Vec<Zgicabra>   = Vec::with_capacity(HISTORY_WINDOW);
    let mut delta_events:  Vec<DeltaEvent> = Vec::new();
    let mut delta_history: Vec<DeltaEvent> = Vec::new();

    history.push(zgicabra.clone()); // Fill first frame to allow initial derivatives

    if !no_ui {
        print!("{}{}", termion::cursor::Hide, termion::clear::All);
    }

    println!("║ Running...");

    loop {
        hydra::update(&mut hydra_state);

        if no_ui {
            let t = tools::millis_now();
            let l = &hydra_state.controllers[0];
            let r = &hydra_state.controllers[1];

            crate::dbg!(
                "F t={} L seq={} pos=[{:.4},{:.4},{:.4}] quat=[{:.3},{:.3},{:.3},{:.3}] joy=[{:.3},{:.3}] trig={:.3} btn={:#011b} en={} dock={} | R seq={} pos=[{:.4},{:.4},{:.4}] quat=[{:.3},{:.3},{:.3},{:.3}] joy=[{:.3},{:.3}] trig={:.3} btn={:#011b} en={} dock={}",
                t,
                l.sequence_number, l.pos[0], l.pos[1], l.pos[2], l.rot_quat[0], l.rot_quat[1], l.rot_quat[2], l.rot_quat[3], l.joystick_x, l.joystick_y, l.trigger, l.buttons, l.enabled, l.is_docked,
                r.sequence_number, r.pos[0], r.pos[1], r.pos[2], r.rot_quat[0], r.rot_quat[1], r.rot_quat[2], r.rot_quat[3], r.joystick_x, r.joystick_y, r.trigger, r.buttons, r.enabled, r.is_docked,
            );
        }

        let voice_cycle = hydra::take_voice_cycle(&mut hydra_state);
        let tune_cycle  = hydra::take_tune_cycle(&mut hydra_state);
        zgicabra::update(&mut zgicabra, &history.last().unwrap(), &hydra_state, voice_cycle, tune_cycle, &mut delta_events);
        delta_events.extend(hydra::take_midi_notes(&mut hydra_state));

        if let Some(mc) = &mock_controls {
            if mc.midi.connected {
                bridge.filter.set(mc.midi.filter.load());
                bridge.width.set(mc.midi.width.load());
                bridge.fuzz.set(mc.midi.fuzz.load());
                bridge.thump.set(mc.midi.thump.load());
                // Bend also has a live source (wand twist), so only an actual
                // off-center wheel gesture takes it over; forwarding 0 every
                // tick would pin it there forever (SignalOverride has no
                // other way to release `enabled`) -- centering the wheel
                // hands control back to twist, like a spring-return wheel.
                if mc.midi.bend.load() != 0.0 {
                    bridge.bend.set(mc.midi.bend.load());
                } else {
                    bridge.bend.enabled.store(false, Ordering::Relaxed);
                }
            }
            if mc.seq_playing.load(Ordering::Relaxed) {
                bridge.filter.set(mc.seq_filter.load());
            }
        }

        bridge.sync(&mut zgicabra);

        if !no_ui {
            ui::draw_all(&zgicabra, &history, &delta_history, &audio);
        }

        output.handle_signal(&zgicabra.signal);

        for delta in delta_events.drain(..) {
            crate::dbg!("E t={} - {:?}", tools::millis_now(), delta);
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

