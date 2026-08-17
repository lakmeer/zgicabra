use textplots::{ColorPlot,Chart,Shape};

use crate::tools::*;
use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,SignalState};

use crate::HISTORY_WINDOW;

use super::utils::*;


//
// Debug Panel
//
// - Wand position & derivatives graph
// - Raw zgicabra state 
// - Recent DeltaEvents
//

fn draw_graph (y: u16, history: &Vec<Zgicabra>) {

    let n = history.len();

    let mut left_pos  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_pos : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_vel  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_vel : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut left_acc  : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];
    let mut right_acc : [ (f32, f32); HISTORY_WINDOW ] = [ (0.0, 0.0); HISTORY_WINDOW ];

    for i in 0..n {
        match history.get(i) {
            None => { },
            Some(frame) => {
                left_pos[i]   = (i as f32, frame.left.pos[0]);
                right_pos[i]  = (i as f32, frame.right.pos[0]);
                left_vel[i]   = (i as f32, frame.left.scalar_vel   *   -100.0);
                right_vel[i]  = (i as f32, frame.right.scalar_vel  *    100.0);
                left_acc[i]   = (i as f32, frame.left.scalar_acc   *   -800.0);
                right_acc[i]  = (i as f32, frame.right.scalar_acc  *    800.0);
            }
        }
    }

    print!("{}", goto(1, y+2));
    Chart::new_with_y_range(70, 90, 0.0, n as f32, -600.0, 600.0)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_acc),  BLUE_2)
        .linecolorplot(&Shape::Lines(&right_vel),  BLUE_0)
        .linecolorplot(&Shape::Lines(&right_pos),  BLUE_1)
        .display();

    // Blank right side for next draw phase
    for i in y+1..y+25 {
        print!("{}{}{}", 
            goto(38, i),
            termion::color::Fg(termion::color::White), 
            " ".repeat(38));
    }
}

fn draw_events (x: u16, y: u16, delta_history: &Vec<DeltaEvent>) {
    print!("{}{}", goto(x, y), termion::color::Fg(termion::color::White));

    for row in 0..12 {
        match delta_history.iter().rev().nth(row) {
            Some(e) => println!("{}- {:?}", goto(x, y + row as u16), e),
            None    => println!("{}-",      goto(x, y + row as u16)),
        }
    }
}

fn draw_note_state (x: u16, y: u16, state: &Zgicabra) {
    println!("{}Root:    {:>17}",   goto(x, y + 0), format_note(state.note.root));
    println!("{}Current: {:>17}",   goto(x, y + 1), format_note(state.note.current));
    println!("{}Pitch:   {:>17.4}", goto(x, y + 2), state.note.bend);
    println!("{}Filter:  {:>17.4}", goto(x, y + 3), state.signal.filter);
    println!("{}Fuzz:    {:>17.4}", goto(x, y + 4), state.signal.fuzz);
    println!("{}Width:   {:>17.4}", goto(x, y + 5), state.signal.width);
    println!("{}Thump:   {:>17.4}", goto(x, y + 6), state.signal.thump);
}


pub fn draw_debug_panel (y: u16, zgicabra: &Zgicabra, history: &Vec<Zgicabra>, delta_history: &Vec<DeltaEvent>) {
    draw_graph(y, &history);
    draw_note_state(38, y + 2, &zgicabra);
    draw_events(38, y + 10, &delta_history);

    /*
    print!("{}{:^40}", goto(0, 19), format!("[{:.3} {:.3} {:.3} {:.3}]",
        zgicabra.left.rot[0],
        zgicabra.left.rot[1],
        zgicabra.left.rot[2],
        zgicabra.left.rot[3]));
    print!("{}{:^40}", goto(40, 19), format!("[{:.2} {:.3} {:.3} {:.3}]",
        zgicabra.right.rot[0],
        zgicabra.right.rot[1],
        zgicabra.right.rot[2],
        zgicabra.right.rot[3]));
    */
}
