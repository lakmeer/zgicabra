
use std::io::{Write, Error};
use rgb::RGB8;

use termion;
use textplots;

use textplots::{Plot,ColorPlot,Chart,Shape};

use crate::sixense::ControllerFrame;
use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,Wand};
use crate::history::History;


const BANNER_TEXT:&str = "█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";

type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


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

pub fn draw_all (screen: &mut Screen, port_name: &str, zgicabra: &Zgicabra, hydra_state: &HydraState ) -> Result<(), Error> {
    write!(screen, "{}{}", termion::cursor::Goto(1,1), BANNER_TEXT)?;
    write!(screen, "{}| MIDI Port: '{}'", termion::cursor::Goto(1,3), port_name)?;

    write!(screen, "{}L> {}", termion::cursor::Goto(1,5), format_wand(&zgicabra.left))?;
    write!(screen, "{}R> {}", termion::cursor::Goto(1,6), format_wand(&zgicabra.right))?;

    write!(screen, "{}{}", termion::cursor::Goto(1,8), zgicabra.separation)?;

    //draw_history(screen, hydra_state)?;

    write!(screen, "{}", termion::cursor::Goto(1,10)).unwrap();
    draw_test_graph(screen, hydra_state);

    screen.flush()?;

    Ok(())
}

pub fn draw_history (screen: &mut Screen, state: &HydraState) -> Result<(), Error> {
    for i in 0..state.frame_history[0].size() {
        let ix = i as u16;
        write!(screen, "{}L> {}  ", termion::cursor::Goto(1,10+ix),  format_frame(state.frame_history[0].get(i)))?;
        write!(screen, "{}L> {}  ", termion::cursor::Goto(1,21+ix), format_frame(state.frame_history[1].get(i)))?;
    }

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

const RED:RGB8   = RGB8 { r: 255, g: 120, b: 100 };
const GREEN:RGB8 = RGB8 { r: 100, g: 255, b: 120 };
const BLUE:RGB8  = RGB8 { r: 100, g: 120, b: 255 };

use crate::hydra::HISTORY_WINDOW;

pub fn draw_test_graph (screen: &mut Screen, state: &HydraState) {

    let n = state.frame_history[0].size();

    let mut left_pos  :[ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_pos :[ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];

    for i in 0..n {
        left_pos[i]  = (i as f32, match state.frame_history[0].get(i) {
            None => 0.0,
            Some(frame) => frame.pos[0]
        });

        right_pos[i] = (i as f32, match state.frame_history[1].get(i) {
            None => 0.0,
            Some(frame) => frame.pos[0]
        });
    }

    Chart::new_with_y_range(120, 80, 0.0, n as f32, -500.0, 500.0)
        .linecolorplot(&Shape::Lines(&left_pos), GREEN)
        .linecolorplot(&Shape::Lines(&right_pos), BLUE)
        .display();
}

