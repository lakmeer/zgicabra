
use std::io::{Write, Error};
use rgb::RGB8;

use termion;
use textplots;

use textplots::{Plot,ColorPlot,Chart,Shape};

use crate::sixense::ControllerFrame;
use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,Wand};
use crate::history::History;

use crate::HISTORY_WINDOW;


type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


const BANNER_TEXT:&str = "█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";



//
// Main Drawing Functions
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

pub fn draw_all (zgicabra: &Zgicabra, history: &History<Zgicabra>) -> Result<(), Error> {

    print!("{}{}",      termion::cursor::Goto(1,1), BANNER_TEXT);
    //print!("{}| MIDI Port: '{}'", termion::cursor::Goto(1,3), port_name);
    print!("{}L> {}",   termion::cursor::Goto(1,3), format_wand(&zgicabra.left));
    print!("{}R> {}",   termion::cursor::Goto(1,4), format_wand(&zgicabra.right));
    print!("{}{}",      termion::cursor::Goto(1,6), zgicabra.separation);
    print!("{}",        termion::cursor::Goto(1,8));

    draw_test_graph(history);

    //screen.flush()?;

    Ok(())
}



//
// Other Formatters
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

pub fn format_frame (maybe_frame: Option<&ControllerFrame>) -> String {
    match maybe_frame {
        None => "No Data".to_string(),
        Some(frame) => {
            format!("{}/{}:|{}|:{}:[ {: >6.2} {: >6.2} {: >6.2} | {: >5.3} {: >5.2} {: >5.2} {: >5.2} ]", 
                    minibar(frame.joystick_x, 4),
                    minibar(frame.joystick_y, 4),
                    minimask(frame.buttons),
                    minigauge(frame.trigger),
                    frame.pos[0], frame.pos[1], frame.pos[2],
                    frame.rot_quat[0], frame.rot_quat[1],
                    frame.rot_quat[2], frame.rot_quat[3]
                   )
        }
    }
}

pub fn format_wand (wand: &Wand) -> String {
    format!("{} {} {}",
            minibar(wand.trigger, 16),
            minibar(wand.scalar_vel, 16),
            wand.pos[0],
            )
}


const RED:RGB8    = RGB8 { r: 255, g: 120, b: 100 };
const GREEN:RGB8  = RGB8 { r: 100, g: 255, b: 120 };
const BLUE:RGB8   = RGB8 { r: 100, g: 120, b: 255 };
const LGREEN:RGB8 = RGB8 { r: 180, g: 255, b: 200 };
const LBLUE:RGB8  = RGB8 { r: 180, g: 200, b: 255 };

pub fn draw_test_graph (history: &History<Zgicabra>) {

    let n = history.len();

    let mut left_pos  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_pos : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_vel  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_vel : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];

    for i in 0..n {
        match history.get(i) {
            None => { },
            Some(frame) => {
                left_pos[i]  = (i as f32, frame.left.pos[0]);
                right_pos[i] = (i as f32, frame.right.pos[0]);
                left_vel[i]  = (i as f32, frame.left.scalar_vel * 200.0);
                right_vel[i] = (i as f32, frame.right.scalar_vel * 200.0);
            }
        }
    }

    Chart::new_with_y_range(140, 140, 0.0, n as f32, -500.0, 500.0)
        .linecolorplot(&Shape::Lines(&left_pos), GREEN)
        .linecolorplot(&Shape::Lines(&right_pos), BLUE)
        .linecolorplot(&Shape::Lines(&left_vel), LGREEN)
        .linecolorplot(&Shape::Lines(&right_vel), LBLUE)
        .display();
}

