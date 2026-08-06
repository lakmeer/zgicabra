
//
// Hydra
//
// Public API for wand/controller input. Two interchangeable backends sit behind
// it:
//
//  - `real` (linux x86_64 and macOS x86_64 only, see build.rs) talks to actual
//    Hydra hardware via libsixense.
//  - `mock` generates synthetic wand motion and simulates the triggers from the
//    keyboard. It's used on any target without a libsixense build (build.rs
//    sets `have_real_hydra`), and as a runtime fallback if no hardware
//    responds within the detection window.
//

use std::time::{Instant,Duration};

use libc::{c_float, c_int, c_uint, c_uchar, c_ushort};

#[cfg(have_real_hydra)]
mod real;
mod mock;

pub use mock::MockControls;

pub const LEFT_HAND:  c_uchar = 1;
pub const RIGHT_HAND: c_uchar = 2;

pub const BUTTON_JOYCLICK : c_uint = 0b100000000;
pub const BUTTON_BUMPER   : c_uint = 0b010000000;
pub const BUTTON_HOME     : c_uint = 0b000000001;
pub const BUTTON_1        : c_uint = 0b000100000;
pub const BUTTON_2        : c_uint = 0b001000000;
pub const BUTTON_3        : c_uint = 0b000001000;
pub const BUTTON_4        : c_uint = 0b000010000;


//
// ControllerFrame
//
// One frame of all data from the hydra, shaped according to the Sixense API.
// Both backends fill one of these in per wand per frame.
//

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


//
// HydraState
//
// Manages a block of memory in which we can record and manipulate incoming hydra data
//

pub struct HydraState {
    pub timestamp:   Instant,
    pub timedelta:   Duration,
    pub controllers: [ ControllerFrame; 2 ],
    backend: Option<Backend>,
}

enum Backend {
    #[cfg(have_real_hydra)]
    Real,
    Mock(mock::MockBackend),
}

impl HydraState {
    pub fn new() -> HydraState {
        HydraState {
            timestamp:   Instant::now(),
            timedelta:   Duration::from_millis(0),
            controllers: [ ControllerFrame::new(), ControllerFrame::new() ],
            backend:     None,
        }
    }
}

impl Default for HydraState {
    fn default() -> HydraState {
        HydraState::new()
    }
}


//
// Functions
//

pub fn start (state: &mut HydraState) {
    #[cfg(have_real_hydra)]
    {
        if real::try_start() {
            state.backend = Some(Backend::Real);
            return;
        }
        println!("Hydra::start - falling back to mock hydra backend.");
    }

    state.backend = Some(Backend::Mock(mock::MockBackend::new()));
}

pub fn stop (state: &mut HydraState) {
    #[cfg(have_real_hydra)]
    if let Some(Backend::Real) = &state.backend {
        real::stop();
    }

    state.backend = None;
}

pub fn update (state: &mut HydraState) {
    match &mut state.backend {
        #[cfg(have_real_hydra)]
        Some(Backend::Real) => real::update(&mut state.controllers),
        Some(Backend::Mock(backend)) => backend.update(&mut state.controllers),
        None => panic!("hydra::update called before hydra::start"),
    }

    state.timedelta = Instant::now().duration_since(state.timestamp);
    state.timestamp = Instant::now();
}

// Net voice-cycle direction accumulated since the last call (mock backend
// only, 'a'/'s' -- see mock::MockBackend::take_voice_cycle). The real backend
// has no keyboard, so it always returns 0; voice changes there come through
// the physical Rocking button instead, handled entirely in zgicabra::update.
pub fn take_voice_cycle (state: &mut HydraState) -> i8 {
    match &mut state.backend {
        Some(Backend::Mock(backend)) => backend.take_voice_cycle(),
        _ => 0,
    }
}

// Net tune direction accumulated since the last call (mock backend only,
// '-'/'=' -- see mock::MockBackend::take_tune_cycle). Same real-backend
// fallback reasoning as take_voice_cycle above: the physical Tune button
// covers this on real hardware, handled entirely in zgicabra::update.
pub fn take_tune_cycle (state: &mut HydraState) -> i8 {
    match &mut state.backend {
        Some(Backend::Mock(backend)) => backend.take_tune_cycle(),
        _ => 0,
    }
}

// A shared handle onto the mock backend's inputs, for a UI to drive directly
// (e.g. gui.rs's mock hydra panel). None on the real backend -- there's no
// keyboard-driven input to hand out.
pub fn mock_controls (state: &HydraState) -> Option<MockControls> {
    match &state.backend {
        Some(Backend::Mock(backend)) => Some(backend.controls()),
        _ => None,
    }
}

// True if the user has asked to quit. On the real backend this is any keypress
// (unchanged from before); the mock backend reserves 'z' and '.' for the
// triggers, so it listens for 'q' instead.
pub fn should_quit (state: &mut HydraState) -> bool {
    match &mut state.backend {
        Some(Backend::Mock(backend)) => backend.should_quit(),
        _ => {
            use std::io::Read;
            std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some()
        },
    }
}
