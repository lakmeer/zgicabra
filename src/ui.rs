
use std::io::{Write, Error};
use std::f32::consts::PI;

use termion;
use textplots;
use drawille;
use rand::prelude::*;

use textplots::{Plot,ColorPlot,Chart,Shape};
use rgb::RGB8;

use crate::sixense::ControllerFrame;
use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,Wand,Hand,Direction,Joystick};
use crate::history::History;

use crate::HISTORY_WINDOW;


type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


const BARCODE: &str    = " ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";
const BANNER_TEXT:&str = "█║▌▌║│▌█║▌▌║║║▌║║▌▌│▌█│║▌▌│║█▌║║║▌│ zgicabra ▌▌▌│║▌║║▌█║▌║▌║█║▌║│▌█║║▌▌║║║▌║║█▌│";

const BLUE_0:RGB8  = RGB8 { r: 120, g: 150, b: 255 };
const BLUE_1:RGB8  = RGB8 { r: 150, g: 200, b: 255 };
const BLUE_2:RGB8  = RGB8 { r:  60, g:  80, b: 155 };
const BLUE_3:RGB8  = RGB8 { r: 180, g: 180, b: 180 };

const GREEN_0:RGB8 = RGB8 { r: 120, g: 255, b: 150 };
const GREEN_1:RGB8 = RGB8 { r: 150, g: 255, b: 200 };
const GREEN_2:RGB8 = RGB8 { r:  60, g: 155, b: 80 };
const GREEN_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };



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
    print!("{}",        termion::color::Fg(termion::color::White));
    print!("{}{}",      termion::cursor::Goto(1,1), BANNER_TEXT);
    print!("{}L> {}",   termion::cursor::Goto(1,3), format_wand(&zgicabra.left));
    print!("{}R> {}",   termion::cursor::Goto(1,4), format_wand(&zgicabra.right));
    print!("{}{}",      termion::cursor::Goto(1,6), zgicabra.separation);
    //print!("{}",        termion::cursor::Goto(1,8));
    print!("{}{}",      termion::cursor::Goto(6,36), separation_bar(zgicabra.separation/1000.0, 70));

    draw_wand(zgicabra.left,  5,  9);
    draw_wand(zgicabra.right, 45, 9);
    draw_bend(zgicabra.pitchbend, 5, 37);


    //draw_graph(history);

    Ok(())
}

fn rand_barcode_char_as_str () -> char {
    BARCODE.chars().choose(&mut rand::thread_rng()).unwrap()
}

fn separation_bar (sep: f32, len: i16) -> String {
    let mut bar   = String::new();
    let mut space = String::new();

    let sep = sep * len as f32;
    let barlen = (sep).round() as i16;
    println!("\nx {} - {}    ", sep, barlen);

    for i in 0..barlen {
        bar.push(rand_barcode_char_as_str());
    }

    for i in 0..(len - barlen)/2 {
        space.push(' ');
    }

    space.clone() + &bar + &space
}

fn drawille_circle (canvas: &mut drawille::Canvas, x: f32, y: f32, r: f32, color: drawille::PixelColor) {
    for i in 0..90 {
        let theta = i as f32 * PI / 45.0;
        let x = x + r * theta.cos();
        let y = y + r * theta.sin();
        canvas.set_colored(x as u32, y as u32, color);
    }
}

fn drawille_octants (canvas: &mut drawille::Canvas, x: f32, y: f32, r: f32, color: drawille::PixelColor, octant: Direction) {
    let sides = 8.0;
    let inc = 2.0 * PI / sides;
    for i in 0..sides as i32 {
        let theta = (i as f32 + 0.5) * inc;
        let x1 = 30.0 + 10.0 * theta.cos();
        let y1 = 30.0 - 10.0 * theta.sin();
        let x2 = 30.0 + 10.0 * (theta + inc).cos();
        let y2 = 30.0 - 10.0 * (theta + inc).sin();
        canvas.line_colored(x1 as u32, y1 as u32, x2 as u32, y2 as u32, color);
        canvas.line_colored(x1 as u32, y1 as u32, x as u32, y as u32, color);

        match octant {
            Direction::Up        => canvas.line_colored(x1 as u32, y1 as u32, x1 as u32,  0, color),
            Direction::Down      => canvas.line_colored(x1 as u32, y1 as u32, x1 as u32, 10, color),
            Direction::Left      => canvas.line_colored(x1 as u32, y1 as u32,  0, y1 as u32, color),
            Direction::Right     => canvas.line_colored(x1 as u32, y1 as u32, 10, y1 as u32, color),
            Direction::UpRight   => canvas.line_colored(x1 as u32, y1 as u32, 10,  0, color),
            Direction::UpLeft    => canvas.line_colored(x1 as u32, y1 as u32,  0,  0, color),
            Direction::DownRight => canvas.line_colored(x1 as u32, y1 as u32, 10, 10, color),
            Direction::DownLeft  => canvas.line_colored(x1 as u32, y1 as u32,  0, 10, color),

            _ => {}
        };
    }
}

fn draw_bend (bend: i16, x: u16, y: u16) {
    const WIDTH  : u32 = 141;
    const HEIGHT : u32 = 46;

    let mut canvas = drawille::Canvas::new(WIDTH, HEIGHT);

    canvas.line_colored(1, 3, WIDTH - 1, 3, drawille::PixelColor::White);
    canvas.line_colored(1, 3, 1, HEIGHT - 1, drawille::PixelColor::White);
    canvas.line_colored(1, HEIGHT - 1, WIDTH - 1, HEIGHT - 1, drawille::PixelColor::White);
    canvas.line_colored(WIDTH - 1, 3, WIDTH - 1, HEIGHT - 1, drawille::PixelColor::White);

    canvas.text(1, 1, 20, &format!("{}", bend));

    drawille_paste(&mut canvas.rows(), x+1, y+1);
}


fn draw_wand (wand: Wand, x: u16, y: u16) {
    const WIDTH  : f32 = 58.0;
    const HEIGHT : f32 = 95.0;

    let mut turtle = drawille::Turtle::new(0.0, 0.0);

    let my_color = match wand.hand {
        Hand::Left => drawille::PixelColor::Blue,
        Hand::Right => drawille::PixelColor::Green,
        Hand::Unknown => drawille::PixelColor::Red
    };

    let my_bright = match wand.hand {
        Hand::Left => drawille::PixelColor::BrightBlue,
        Hand::Right => drawille::PixelColor::BrightGreen,
        Hand::Unknown => drawille::PixelColor::BrightRed
    };



    //
    // Canvas outputs
    //

    // Border

    turtle.up();
    turtle.teleport(1.0, 1.0);
    turtle.down();

    for i in 0..2 {
        turtle.forward(WIDTH);
        turtle.right(90.0);
        turtle.forward(HEIGHT);
        turtle.right(90.0);
    }


    // Bumper Beam

    turtle.up();
    turtle.teleport(7.0, 9.0);
    turtle.color(my_color);
    turtle.left(90.0);
    turtle.down();

    draw_trigger(&mut turtle, 45.0, 5.0, my_color, wand.bumper as i8 as f32);


    // Trigger Beam

    turtle.up();
    turtle.teleport(7.0, 19.0);

    draw_trigger(&mut turtle, 45.0, 9.0, my_color, wand.trigger);


    // Joystick Octants

    draw_octants(&mut turtle, 29.5, 54.0, 19.0, my_color, wand.stick.octant);


    // Buttons

    let button_y = 92.0;

    match wand.hand {
        Hand::Left => {
            drawille_button(&mut turtle,  8.0, button_y, wand.buttons[3], 45.0, my_color);
            drawille_button(&mut turtle, 20.0, button_y, wand.buttons[1],  0.0, my_color);
            drawille_button(&mut turtle, 32.0, button_y, wand.buttons[0],  0.0, my_color);
            drawille_button(&mut turtle, 44.0, button_y, wand.buttons[2],-45.0, my_color);
        },
        Hand::Right => {
            drawille_button(&mut turtle,  8.0, button_y, wand.buttons[2], 45.0, my_color);
            drawille_button(&mut turtle, 20.0, button_y, wand.buttons[0],  0.0, my_color);
            drawille_button(&mut turtle, 32.0, button_y, wand.buttons[1],  0.0, my_color);
            drawille_button(&mut turtle, 44.0, button_y, wand.buttons[3],-45.0, my_color);
        },
        _ => {}
    }

    turtle.up();
    turtle.teleport(18.0, 62.0);
    turtle.right(180.0);
    turtle.color(drawille::PixelColor::White);
    turtle.down();


    // Print without disrupting existing output
    drawille_paste(&mut turtle.cvs.rows(), x+1, y+1);
    print!("{}", termion::color::Fg(termion::color::White));

}

fn draw_trigger (turtle: &mut drawille::Turtle, w: f32, h: f32, my_color: drawille::PixelColor, z: f32) {

    turtle.up();
    turtle.color(my_color);
    turtle.back(h/2.0);
    turtle.down();

    for i in 0..2 {
        turtle.forward(h);
        turtle.right(90.0);
        turtle.forward(w);
        turtle.right(90.0);
    }

    if z > 0.0 {
        turtle.up();
        turtle.right(90.0);
        turtle.forward(1.0);
        turtle.left(90.0);
        turtle.forward(h/2.0);
        turtle.down();

        turtle.color(drawille::PixelColor::White);

        let h = (h - 1.0) * z as f32;

        turtle.up();
        turtle.back(h/2.1);
        turtle.down();

        scribble(turtle, w - 1.0, h - 1.0, true);

        turtle.forward(h);
        turtle.color(my_color);
    }
}


fn scribble (turtle: &mut drawille::Turtle, w: f32, h: f32, z: bool) {
    for i in 0..(w/2.0).round() as u16 {
        if rand::random() { turtle.down(); }
        turtle.forward(h);
        turtle.right(90.0);
        if z { turtle.up(); }
        turtle.forward(1.0);
        turtle.right(90.0);

        if rand::random() { turtle.down(); }
        turtle.forward(h);
        turtle.left(90.0);
        if z { turtle.up(); }
        turtle.forward(1.0);
        turtle.left(90.0);
    }
}

fn draw_octants(turtle: &mut drawille::Turtle, x: f32, y: f32, oct_edge: f32, my_color: drawille::PixelColor, octant: Direction) {

    let oct_rad = oct_edge * 1.30656; // oct_edge * 0.5 / (PI / 8.0).sin();

    turtle.up();
    turtle.teleport(x, y);
    turtle.left(22.5);
    turtle.color(my_color);
    turtle.down();

    for i in 0..8 {
        turtle.forward(oct_rad);
        turtle.right(22.5 + 90.0);
        turtle.forward(oct_edge);                                                      
        turtle.right(22.5 + 90.0);
        turtle.up();
        turtle.forward(oct_rad);
        turtle.down();
        turtle.right(180.0);
    }


    // Joystick Octants: Selected Octant

    turtle.color(drawille::PixelColor::White);
    let facing = 225.0 + 45.0 * octant as i32 as f32;

    if octant != Direction::None {
        turtle.right(facing);

        for i in 0..40 {
            if rand::random::<f32>() < 0.2 { turtle.down(); } else { turtle.up(); }
            turtle.forward(oct_rad * 0.93);
            turtle.up();
            turtle.back(oct_rad * 0.93);
            turtle.right(45.0/40.0);
        }

        turtle.left(45.0);
        turtle.left(facing);
    }


    // Joystick Octants: Border touchup

    turtle.up();
    turtle.forward(oct_rad);
    turtle.right(90.0 + 22.5);
    turtle.color(my_color);
    turtle.down();

    for i in 0..8 {
        turtle.forward(oct_edge);
        turtle.right(45.0);
    }

    turtle.left(90.0);
}


fn drawille_button (turtle: &mut drawille::Turtle, x: f32, y: f32, pressed: bool, skew: f32, my_color: drawille::PixelColor) {
    turtle.up();
    turtle.teleport(x, y);
    turtle.color(my_color);
    turtle.down();

    let h = 9.0;

    for i in 0..2 {
        turtle.forward(h);
        turtle.right(90.0);
        turtle.forward(h);
        turtle.right(90.0);
    }

    if pressed {
        turtle.up();
        turtle.teleport(x + 1.0, y - 1.0);
        turtle.color(drawille::PixelColor::White);
        turtle.down();

        scribble(turtle, h-2.0, h-2.0, true);
    }
}

fn drawille_paste (rows: &mut Vec<String>, x: u16, y: u16) {
    for (ix, row) in rows.iter().enumerate() {
        print!("{}{}", termion::cursor::Goto(x,y+ix as u16), row);
    }
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
        _ => "×".to_string()
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
    format!("{}▕{}▏]{}[ -{}-{}{}{}{}-",
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

