
use std::time::{Instant,Duration};
use std::thread::sleep;

use libc::{c_float, c_int, c_uint, c_uchar, c_ushort};

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
// One frame of all data from the hydra formatted according to Sixense API
//

#[link(name="sixense_x64")]
extern {
    pub fn sixenseInit();
    pub fn sixenseExit();
    pub fn sixenseGetNewestData(which: c_int, data: *mut ControllerFrame);
}

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
    pub initialised: bool,
    pub timestamp:   Instant,
    pub timedelta:   Duration,
    pub temp_frame:  ControllerFrame,
    pub controllers: [ ControllerFrame; 2 ],
}

impl HydraState {
    pub fn new() -> HydraState {
        HydraState {
            initialised: false,
            timestamp: Instant::now(),
            timedelta: Duration::from_millis(0),
            temp_frame: ControllerFrame::new(),
            controllers: [ ControllerFrame::new(), ControllerFrame::new() ],
        }
    }
}



//
// Functions
// TODO: Learn what the correct thing is to do with the unsafes here
//

pub fn start (state: &mut HydraState) {
    print!("Hydra::start - init connection... ");
    unsafe { sixenseInit(); }
    state.initialised = true;
    println!("✅");

    print!("Hydra::start - awaiting first frame...");
    while state.temp_frame.which_hand == 0 {
        read_frame(0, &mut state.temp_frame);
        sleep(Duration::from_millis(10));
    }
    println!("✅");
}

pub fn stop (state: &mut HydraState) {
    println!("Hydra::stop - closing down... ");
    unsafe { sixenseExit(); }
    state.initialised = false;
    println!("Hydra::stop - done.");
}

pub fn update (state: &mut HydraState) {

    read_frame(0, &mut state.temp_frame);
    let hand = (state.temp_frame.which_hand - 1) as usize;
    state.controllers[hand] = state.temp_frame;

    read_frame(1, &mut state.temp_frame);
    let hand = (state.temp_frame.which_hand - 1) as usize;
    state.controllers[hand] = state.temp_frame;

    state.timedelta = Instant::now().duration_since(state.timestamp);
    state.timestamp = Instant::now();
}

pub fn read_frame (which: i32, frame_data: &mut ControllerFrame) {
    unsafe { sixenseGetNewestData(which, frame_data); }
}


