
use std::io::{Write, Error};
use rgb::RGB8;

use termion;
use textplots;

use textplots::{Plot,ColorPlot,Chart,Shape};

use crate::sixense::ControllerFrame;
use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,Wand,Direction,Joystick};
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
    print!("{}L> {}",   termion::cursor::Goto(1,3), format_wand(&zgicabra.left));
    print!("{}R> {}",   termion::cursor::Goto(1,4), format_wand(&zgicabra.right));
    print!("{}{}",      termion::cursor::Goto(1,6), zgicabra.separation);
    print!("{}",        termion::cursor::Goto(1,8));

    draw_graph(history);

    Ok(())
}



//
// Other Formatters
//

pub fn minigauge (x: f32) -> String {
    let y = (x * 8.0) as u8;
    match y {
        0 => "▕ ▏".to_string(),
        1 => "▕▁▏".to_string(),
        2 => "▕▂▏".to_string(),
        3 => "▕▃▏".to_string(),
        4 => "▕▄▏".to_string(),
        5 => "▕▅▏".to_string(),
        6 => "▕▆▏".to_string(),
        7 => "▕▇▏".to_string(),
        8 => "▕█▏".to_string(),
        _ => "▕×▏".to_string()
    }
}

pub fn minidir (d: Direction) -> String {
    match d {
        Direction::None      => "(·)".to_string(),
        Direction::Up        => "(↑)".to_string(),
        Direction::UpRight   => "(↗)".to_string(),
        Direction::Right     => "(→)".to_string(),
        Direction::DownRight => "(↘)".to_string(),
        Direction::Down      => "(↓)".to_string(),
        Direction::DownLeft  => "(↙)".to_string(),
        Direction::Left      => "(←)".to_string(),
        Direction::UpLeft    => "(↖)".to_string(),
    }
}

pub fn ministick (stick: Joystick) -> String {
    if stick.clicked {
        "(⬤)".to_string()
    } else {
        minidir(stick.octant)
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
    format!("{}{}]{}[ -{}-{}{}{}{}-",
            ministick(wand.stick),
            minigauge(wand.trigger),
            if wand.bumper     { "█" } else { "░" },
            if wand.home       { "H" } else { "░" },
            if wand.buttons[0] { "1" } else { "░" },
            if wand.buttons[1] { "2" } else { "░" },
            if wand.buttons[2] { "3" } else { "░" },
            if wand.buttons[3] { "4" } else { "░" },
            )
}


const BLUE_0:RGB8  = RGB8 { r: 120, g: 150, b: 255 };
const BLUE_1:RGB8  = RGB8 { r: 150, g: 200, b: 255 };
const BLUE_2:RGB8  = RGB8 { r:  60, g:  80, b: 155 };
const BLUE_3:RGB8  = RGB8 { r: 180, g: 180, b: 180 };

const GREEN_0:RGB8 = RGB8 { r: 120, g: 255, b: 150 };
const GREEN_1:RGB8 = RGB8 { r: 150, g: 255, b: 200 };
const GREEN_2:RGB8 = RGB8 { r:  60, g: 155, b: 80 };
const GREEN_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };

pub fn draw_graph (history: &History<Zgicabra>) {

    let n = history.len();

    let mut left_pos  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_pos : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_vel  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_vel : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_acc  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_acc : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_jerk : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_jerk: [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];

    for i in 0..n {
        match history.get(i) {
            None => { },
            Some(frame) => {
                left_pos[i]   = (i as f32, frame.left.pos[0]);
                right_pos[i]  = (i as f32, frame.right.pos[0]);
                left_vel[i]   = (i as f32, frame.left.scalar_vel   *  -100.0);
                right_vel[i]  = (i as f32, frame.right.scalar_vel  *   100.0);
                left_acc[i]   = (i as f32, frame.left.scalar_acc   *  -800.0);
                right_acc[i]  = (i as f32, frame.right.scalar_acc  *   800.0);
                left_jerk[i]  = (i as f32, frame.left.scalar_jerk  * -60000.0);
                right_jerk[i] = (i as f32, frame.right.scalar_jerk *  60000.0);
            }
        }
    }

    Chart::new_with_y_range(140, 140, 0.0, n as f32, -500.0, 500.0)
        .linecolorplot(&Shape::Lines(&left_jerk), GREEN_3)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_jerk), BLUE_3)
        .linecolorplot(&Shape::Lines(&right_acc),  BLUE_2)
        .linecolorplot(&Shape::Lines(&right_vel),  BLUE_0)
        .linecolorplot(&Shape::Lines(&right_pos),  BLUE_1)
        .nice();
}

