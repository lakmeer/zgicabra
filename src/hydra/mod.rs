
//
// Hydra
//
// Public API for wand/controller input. Three interchangeable backends sit
// behind a common `Backend` trait:
//
//  - `sdk` (linux x86_64 only) talks to real Hydra hardware via libsixense.
//  - `hid` (macOS x86_64 only) talks to real Hydra hardware directly over
//    USB HID, bypassing the dead/crash-prone Sixense SDK (see hid.rs).
//  - `mock` generates synthetic wand motion and simulates triggers from the
//    keyboard. Used on any target without a real backend, and as a runtime
//    fallback if no hardware responds within a real backend's detect window.
//

use std::time::{Instant,Duration};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use libc::{c_float, c_int, c_uint, c_uchar, c_ushort};

use crate::zgicabra::DeltaEvent;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod sdk;
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
mod hid;
mod midi;
mod mock;

pub use mock::MockControls;

pub const LEFT_HAND:  c_uchar = 1;
pub const RIGHT_HAND: c_uchar = 2;

// Puts stdin in cbreak mode: keystrokes are available immediately, no local
// echo. Unlike termion's `into_raw_mode()`, leaves stdout's `\n` -> `\r\n`
// translation alone so plain `print!`/`println!` still works. Used by any
// backend that needs a non-blocking should_quit check.
struct CbreakGuard {
    original: libc::termios,
}

impl CbreakGuard {
    fn enable() -> CbreakGuard {
        unsafe {
            let mut term: libc::termios = std::mem::zeroed();
            libc::tcgetattr(libc::STDIN_FILENO, &mut term);
            let original = term;

            term.c_lflag &= !(libc::ICANON | libc::ECHO);
            term.c_cc[libc::VMIN]  = 1;
            term.c_cc[libc::VTIME] = 0;

            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term);

            CbreakGuard { original }
        }
    }
}

impl Drop for CbreakGuard {
    fn drop (&mut self) {
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original); }
    }
}

pub const BUTTON_JOYCLICK : c_uint = 0b100000000;
pub const BUTTON_BUMPER   : c_uint = 0b010000000;
pub const BUTTON_HOME     : c_uint = 0b000000001;
pub const BUTTON_1        : c_uint = 0b000100000;
pub const BUTTON_2        : c_uint = 0b001000000;
pub const BUTTON_3        : c_uint = 0b000001000;
pub const BUTTON_4        : c_uint = 0b000010000;


// One frame of wand data, shaped according to the Sixense API. All backends
// fill one of these in per wand per frame.

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ControllerFrame {
    pub pos: [c_float; 3],
    pub rot_mat: [[c_float; 3]; 3],
    pub joystick_x: c_float,
    pub joystick_y: c_float,
    pub trigger: c_float,
    pub buttons: c_uint,
    pub sequence_number: c_uchar,
    pub rot_quat: [c_float; 4],
    pub firmware_revision: c_ushort,
    pub hardware_revision: c_ushort,
    pub packet_type: c_uchar,
    pub magnetic_frequency: c_uchar,
    pub enabled: c_int,
    pub controller_index: c_int,
    pub is_docked: c_uchar,
    pub which_hand: c_uchar,
    pub hemi_tracking_enabled: c_uchar,
}

impl ControllerFrame {
    pub fn new() -> ControllerFrame {
        ControllerFrame {
            sequence_number:       0,
            which_hand:            0,
            pos:                   [0.0, 0.0, 0.0],
            rot_mat:               [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
            rot_quat:              [0.0, 0.0, 0.0, 0.0],
            joystick_x:            0.0,
            joystick_y:            0.0,
            trigger:               0.0,
            buttons:               0,
            packet_type:           0,
            controller_index:      0,
            enabled:               0,
            is_docked:             0,
            magnetic_frequency:    0,
            firmware_revision:     0,
            hardware_revision:     0,
            hemi_tracking_enabled: 0,
        }
    }
}

impl Default for ControllerFrame {
    fn default() -> ControllerFrame {
        ControllerFrame::new()
    }
}


// What any Hydra input source must provide. Voice/tune-cycle and
// mock-controls are dev/mock-only conveniences, so they get no-op defaults
// rather than forcing every backend to restate them. should_quit defaults to
// "any keypress"; only the mock backend (which reserves keys for wand input)
// overrides it.
trait Backend: Send {
    fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]);

    fn should_quit (&mut self) -> bool {
        use std::io::Read;
        std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some()
    }

    fn take_voice_cycle (&mut self) -> i8 { 0 }
    fn take_tune_cycle (&mut self) -> i8 { 0 }
    fn mock_controls (&self) -> Option<MockControls> { None }
}

// Records and manipulates incoming hydra data.
pub struct HydraState {
    pub timestamp:   Instant,
    pub timedelta:   Duration,
    pub controllers: [ ControllerFrame; 2 ],
    backend: Option<Box<dyn Backend>>,

    // MIDI note/CC/pitch-bend listener -- connected unconditionally,
    // independent of which Backend is active, so an external MIDI
    // controller can feed notes/global signals alongside real Hydra
    // hardware rather than only as a mock-backend supplement. The mock
    // backend also feeds its own audition-sequence notes onto the same
    // `midi_notes` queue (see mock.rs's step_sequence), so it's kept here
    // rather than owned by any one backend.
    pub midi:   midi::MidiState,
    midi_notes: Arc<Mutex<VecDeque<DeltaEvent>>>,
    _midi_connection: midi::Connection, // held to keep the callback alive; disconnects on drop
}

impl HydraState {
    pub fn new() -> HydraState {
        let midi_notes: Arc<Mutex<VecDeque<DeltaEvent>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (midi, midi_connection) = midi::connect(midi_notes.clone());

        HydraState {
            timestamp:   Instant::now(),
            timedelta:   Duration::from_millis(0),
            controllers: [ ControllerFrame::new(), ControllerFrame::new() ],
            backend:     None,
            midi,
            midi_notes,
            _midi_connection: midi_connection,
        }
    }
}

impl Default for HydraState {
    fn default() -> HydraState {
        HydraState::new()
    }
}


pub fn start (state: &mut HydraState) {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if let Some(backend) = sdk::SdkBackend::try_start() {
        state.backend = Some(Box::new(backend));
        return;
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    if let Some(backend) = hid::HidBackend::try_start() {
        state.backend = Some(Box::new(backend));
        return;
    }

    println!("Hydra::start - falling back to mock hydra backend.");
    state.backend = Some(Box::new(mock::MockBackend::new(state.midi.clone(), state.midi_notes.clone())));
}

pub fn stop (state: &mut HydraState) {
    state.backend = None; // real backends clean up in Drop
}

pub fn update (state: &mut HydraState) {
    match &mut state.backend {
        Some(backend) => backend.update(&mut state.controllers),
        None => panic!("hydra::update called before hydra::start"),
    }

    state.timedelta = Instant::now().duration_since(state.timestamp);
    state.timestamp = Instant::now();
}

// Net voice-cycle direction since the last call ('a'/'s', mock only --
// real hardware uses the physical Rocking button, handled in zgicabra::update).
pub fn take_voice_cycle (state: &mut HydraState) -> i8 {
    state.backend.as_mut().map_or(0, |backend| backend.take_voice_cycle())
}

// Net tune direction since the last call ('-'/'=', mock only -- real
// hardware uses the physical Tune button).
pub fn take_tune_cycle (state: &mut HydraState) -> i8 {
    state.backend.as_mut().map_or(0, |backend| backend.take_tune_cycle())
}

// Shared handle onto the mock backend's inputs (audition sequence, keyboard
// state). None on real backends.
pub fn mock_controls (state: &HydraState) -> Option<MockControls> {
    state.backend.as_ref().and_then(|backend| backend.mock_controls())
}

// Note On/Off/PC DeltaEvents accumulated since the last call -- MIDI-sourced
// (any backend) and/or the mock backend's audition-sequence player.
pub fn take_midi_notes (state: &mut HydraState) -> Vec<DeltaEvent> {
    state.midi_notes.lock().unwrap().drain(..).collect()
}

// True if the user has asked to quit -- any keypress on real backends; the
// mock backend reserves 'z'/'.' for triggers, so it listens for 'q' instead.
pub fn should_quit (state: &mut HydraState) -> bool {
    state.backend.as_mut().map_or(false, |backend| backend.should_quit())
}
