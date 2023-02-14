
use crate::HydraFrame;


//
// Formatters
//

fn minibar (x: f32, n: u8) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < n {
        if x > (i as f32) / n as f32 {
            s.push_str("█");
        } else {
            s.push_str("░");
        }
        i += 1;
    }
    s
}

fn minimask (x: u32) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < 8 {
        if x & (1 << i) != 0 {
            s.push_str("•");
        } else {
            s.push_str("◦");
        }
        i += 1;
    }
    s
}

pub fn format_frame (frame: HydraFrame) -> String {
    format!("{}@{}/{}:{}:[ {: >6.2} {: >6.2} {: >6.2} | {: >5.3} {: >5.2} {: >5.2} {: >5.2} ]", 
             minibar(frame.trigger, 8),
             minibar(frame.joystick_x, 4), minibar(frame.joystick_y, 4),
             minimask(frame.buttons),
             frame.pos[0], frame.pos[1], frame.pos[2],
             frame.rot_quat[0], frame.rot_quat[1], frame.rot_quat[2], frame.rot_quat[3]
            )
}

