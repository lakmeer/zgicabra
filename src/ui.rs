
use std::io::{Write};

use crate::hydra::format_frame;

use crate::ZgiState;



//
// Formatters
//

pub fn minigauge (x: f32) -> String {
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

pub fn minibar (x: f32, n: u8) -> String {
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

pub fn minimask (x: u32) -> String {
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



//
// Interface Layout
//

const BANNER_TEXT:&str = "█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";


//
// Main Drawing Function
//

pub fn header () {
    print!("{}{}{}{}{}",
           termion::clear::All,
           termion::cursor::Hide,
           termion::cursor::Goto(1,1),
           BANNER_TEXT,
           termion::cursor::Goto(1,3)
    );
}

pub fn draw_all (state: &mut ZgiState) {
    write!(state.screen, "{}{}{}", termion::clear::All, termion::cursor::Goto(1,1), BANNER_TEXT).unwrap();
    write!(state.screen, "{}| MIDI Port: '{}'", termion::cursor::Goto(1,3), state.port_name).unwrap();
    //write!(state.screen, "{}L> {}", termion::cursor::Goto(1,5), format_frame(state.frames[0].last())).unwrap();
    //write!(state.screen, "{}R> {}", termion::cursor::Goto(1,6), format_frame(state.frames[1].last())).unwrap();
    state.screen.flush().unwrap();
}

