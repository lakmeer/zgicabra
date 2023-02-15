
use crate::sixense::ControllerFrame;


//
// Hydra Utilities
//

pub fn blank_frame() -> ControllerFrame {
    ControllerFrame {
        pos: [0.0, 0.0, 0.0],
        rot_mat: [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        joystick_x: 0.0,
        joystick_y: 0.0,
        trigger: 0.0,
        buttons: 0,
        sequence_number: 0,
        rot_quat: [0.0, 0.0, 0.0, 0.0],
        firmware_revision: 0,
        hardware_revision: 0,
        packet_type: 0,
        magnetic_frequency: 0,
        enabled: 0,
        controller_index: 0,
        is_docked: 0,
        which_hand: 0,
        hemi_tracking_enabled: 0,
    }
}


use crate::ui::{minibar, minigauge, minimask};

pub fn format_frame (frame: ControllerFrame) -> String {
    format!("[{}]@[{}/{}]:{}:[ {: >6.2} {: >6.2} {: >6.2} | {: >5.3} {: >5.2} {: >5.2} {: >5.2} ]", 
             minigauge(frame.trigger),
             minibar(frame.joystick_x, 4),
             minibar(frame.joystick_y, 4),
             minimask(frame.buttons),
             frame.pos[0], frame.pos[1], frame.pos[2],
             frame.rot_quat[0], frame.rot_quat[1], frame.rot_quat[2], frame.rot_quat[3]
            )
}
