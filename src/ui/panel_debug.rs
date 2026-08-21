
//
// Debug Panel
//
// - Wand position & derivatives graph
// - Raw zgicabra state
// - Recent DeltaEvents
//

use crate::tools::*;
use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,SignalState};
use crate::plot::{ColorPlot,Chart,Shape};
use crate::HISTORY_WINDOW;

use super::tw;
use super::utils::*;

pub fn draw_debug_panel (y: u16, zgicabra: &Zgicabra, history: &Vec<Zgicabra>, delta_history: &Vec<DeltaEvent>) {
    const RHS:u16 = 50;

    draw_graph(RHS as u32 - 4, 23, y, &history);

    println!("{}Root:    {:>17}",   goto(RHS, y +  2), format_note(zgicabra.note.root));
    println!("{}Current: {:>17}",   goto(RHS, y +  3), format_note(zgicabra.note.current));
    println!("{}Pitch:   {:>17.4}", goto(RHS, y +  4), zgicabra.note.bend);
    println!("{}Filter:  {:>17.4}", goto(RHS, y +  5), zgicabra.signal.filter);
    println!("{}Fuzz:    {:>17.4}", goto(RHS, y +  6), zgicabra.signal.fuzz);
    println!("{}Width:   {:>17.4}", goto(RHS, y +  7), zgicabra.signal.width);
    println!("{}Thump:   {:>17.4}", goto(RHS, y +  8), zgicabra.signal.thump);
    println!("{}Level:   {:>17.4}", goto(RHS, y +  9), zgicabra.signal.level);
    println!("{}Total:   {:>17.4}", goto(RHS, y + 10), zgicabra.trigger_total);

    let default_color = termion::color::Fg(termion::color::White);

    print!("{}{}", goto(RHS, y), default_color);

    print!("{}[{:>5.2} {:>5.2} {:>5.2} {:>5.2} ]", goto(RHS, y + 12), 
        zgicabra.left.rot[0],
        zgicabra.left.rot[1],
        zgicabra.left.rot[2],
        zgicabra.left.rot[3]);
    print!("{}[{:>5.2} {:>5.2} {:>5.2} {:>5.2} ]", goto(RHS, y + 13), 
        zgicabra.right.rot[0],
        zgicabra.right.rot[1],
        zgicabra.right.rot[2],
        zgicabra.right.rot[3]);

    for row in 0..10 {
        println!("{}{}", goto(RHS, y + 15 + row as u16), " ".repeat(25));
        let color = if row == 0 { fg(tw::WHITE) } else { fg(tw::SLATE_500) };
        match delta_history.iter().rev().nth(row) {
            Some(e) => println!("{}{}- {:?}", goto(RHS, y + 15 + row as u16), color, e),
            None    => println!("{}{}- ",     goto(RHS, y + 15 + row as u16), color),
        }
    }

    print!("{}{}", goto(RHS, y), default_color);
}

fn draw_graph (w: u32, h: u32, y: u16, history: &Vec<Zgicabra>) {

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

    Chart::new(w, h, 0.0, n as f32, -600.0, 600.0)
        .position(2, y + 2)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_acc),  BLUE_2)
        .linecolorplot(&Shape::Lines(&right_vel),  BLUE_0)
        .linecolorplot(&Shape::Lines(&right_pos),  BLUE_1)
        .display();
}
