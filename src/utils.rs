
use crate::HydraFrame;


//
// Formatters
//

fn minigauge (x: f32) -> String {
    let y = (x * 8.0) as u8;
    match y {
        0 => " ".to_string(),
        1 => "▁".to_string(),
        2 => "▂".to_string(),
        3 => "▃".to_string(),
        4 => "▄".to_string(),
        5 => "▅".to_string(),
        6 => "▆".to_string(),
        7 => "▇".to_string(),
        8 => "█".to_string(),
        _ => "X".to_string()
    }
}

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
    format!("[{}]@[{}/{}]:{}:[ {: >6.2} {: >6.2} {: >6.2} | {: >5.3} {: >5.2} {: >5.2} {: >5.2} ]", 
             minigauge(frame.trigger),
             minibar(frame.joystick_x, 4),
             minibar(frame.joystick_y, 4),
             minimask(frame.buttons),
             frame.pos[0], frame.pos[1], frame.pos[2],
             frame.rot_quat[0], frame.rot_quat[1], frame.rot_quat[2], frame.rot_quat[3]
            )
}

