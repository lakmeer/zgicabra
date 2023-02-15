
use libc::{c_float, c_int, c_uint, c_uchar, c_ushort};


//
// Link Sixense library
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

#[link(name="sixense_x64")]
extern {
    pub fn sixenseInit();
    pub fn sixenseExit();
    pub fn sixenseGetNewestData(which: c_int, data: *mut ControllerFrame);
}


//
// Wrappers
// TODO: Learn what the correct thing is to do here
//

pub fn init() {
    unsafe {
        sixenseInit();
    }
}

pub fn exit() {
    unsafe {
        sixenseExit();
    }
}

pub fn read_frame(which: c_int, frame: &mut ControllerFrame) {
    unsafe {
        sixenseGetNewestData(which, frame);
    }
}


