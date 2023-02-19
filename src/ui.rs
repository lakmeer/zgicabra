
use std::io::{Write, Error};
use std::time::Instant;
use std::f32::consts::PI;

use termion;
use rgb::RGB8;

use textplots;
use textplots::{ColorPlot,Chart,Shape};

use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use rand::prelude::*;
use rand_distr::StandardNormal;

use lazy_static::lazy_static;

use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,Wand,Hand,Direction,Joystick};
use crate::history::History;

use crate::HISTORY_WINDOW;


type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


lazy_static! {
    static ref START_TIME: Instant = Instant::now();
}


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

pub fn draw_all (zgicabra: &Zgicabra, history: &History<Zgicabra>) -> Result<(), Error> {

    // Text dimensions
    const TEXT_WIDTH  : u16 = 80;
    const TEXT_HEIGHT : u16 = TEXT_WIDTH / 2;

    // Pixel dimensions
    const CANVAS_WIDTH  : u16 = TEXT_WIDTH * 2;
    const CANVAS_HEIGHT : u16 = TEXT_HEIGHT * 4;

    // Vector dimensions
    const WIDTH  : f32 = CANVAS_WIDTH  as f32;
    const HEIGHT : f32 = CANVAS_HEIGHT as f32;


    // Canvas
    let mut canvas = Canvas::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);

    draw_banner(TEXT_WIDTH, 1);

    draw_wand(&mut canvas, zgicabra.left,  WIDTH*1.0/4.0, HEIGHT/4.0, WIDTH/6.0, sin(0.0)); //zgicabra.left.rot[0]);
    draw_wand(&mut canvas, zgicabra.right, WIDTH*3.0/4.0, HEIGHT/4.0, WIDTH/6.0, 0.0); //zgicabra.right.rot[1]);

    // Output canvas
    print!("{}{}", termion::cursor::Goto(1,2), &mut canvas.frame());

    println!("{}", barcode_string(TEXT_WIDTH.into()));

    draw_bend(zgicabra.pitchbend, zgicabra.separation, 41, 47, 70, 20);

    // flush

    //draw_graph(history);

    Ok(())
}


//
// Sub-drawing Functions
//

fn draw_wand (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, oct_rad: f32, angle: f32) {

    let color = rgb_f32(nsin(0.0), nsin(0.25), nsin(0.5));
    let facing = angle - PI/2.0;
    let stick_facing = match wand.stick.octant {
        Direction::None => facing,
        _ => facing - PI*3.0/4.0 + PI/4.0 * wand.stick.octant as i32 as f32
    };


    // Joystick Octants Outline

    for i in 0..8 {
        let a = facing + (i as f32/4.0) * PI + 0.125 * PI;
        line(canvas, x, y, x + oct_rad * a.cos(), y + oct_rad * a.sin(), color);
    }
    

    // Joystick Selected Octant

    if wand.stick.octant != Direction::None {
        line(canvas,
             x + 1.2 * oct_rad * (stick_facing - PI/8.0).cos(),
             y + 1.2 * oct_rad * (stick_facing - PI/8.0).sin(),
             x + 1.2 * oct_rad * (stick_facing + PI/8.0).cos(), 
             y + 1.2 * oct_rad * (stick_facing + PI/8.0).sin(),
             color);
    }


    // Trigger

    for i in 0..64 {
        let a = facing - (i as f32 / 64.0) * PI * 3.0/4.0 - PI*5.0/8.0;

        let px = x + oct_rad * 1.1 * a.cos();
        let py = y + oct_rad * 1.1 * a.sin();

        let h = 8.0 * rand_uniform(wand.trigger);
        let white = 0.25 + rand_uniform(0.75);
        let color = rgb_f32(white, white, white);
        line(canvas, px, py, px + h * a.cos(), py + h * a.sin(), color);

        /*
        let len = oct_rad * (0.83 + 0.15 * rand::thread_rng().sample::<f32,_>(StandardNormal));
        let a = facing + rand_uniform(0.85) - PI/8.0 + PI/4.0 * wand.stick.octant as i32 as f32;
        line(canvas, x, y, x + len * a.cos(), y + len * a.sin(), PixelColor::White);
        */
    }

}

fn draw_banner (width: u16, y: u16) {
    let banner_text = " zgicabra ";
    let stripe_length = (width - banner_text.len() as u16) / 2;

    print!("{}{}{}{}", termion::cursor::Goto(1,y),
        barcode_string(stripe_length.into()),
        banner_text,
        barcode_string(stripe_length.into()));
}


fn draw_bend (bend: f32, sep: f32, center_x: u32, y: u32, max_width: u32, max_height: u32) {
    let width  = max_width * 2;
    let height = max_height * 4;

    let mut canvas = drawille::Canvas::new(width + 1, height + 4);

    let w = width  as f32;
    let h = height as f32;

    let left  = w/2.0 - w * sep/2000.0;
    let right = w -left;

    // Border
    /*
    canvas.line(0, 0, w as u32, 0);
    canvas.line(0, 0, 0, h as u32);
    canvas.line(w as u32, 0, w as u32, h as u32);
    canvas.line(0, h as u32, w as u32, h as u32);
    */

    let a = bend * PI/2.0;
    let b = h * (sep/2000.0).clamp(-1.0, 1.0);

    let p1 = (left,  h/2.0);
    let p2 = (left  + b * a.cos(), h/2.0 - b * a.sin());
    let p3 = (right - b * a.cos(), h/2.0 - b * a.sin());
    let p4 = (right, h/2.0);

    let m = 1 + (sep/200.0) as u32;

    for x in 0..200 {
        let t = x as f32/200.0;
        let q1 = lerp_tuple(lerp_tuple(p1, p2, t), lerp_tuple(p2, p3, t), t);
        let q2 = lerp_tuple(lerp_tuple(p2, p3, t), lerp_tuple(p3, p4, t), t);
        let (x, y) = lerp_tuple(q1, q2, t);

        for n in 0..m {
            let j = 3.0 * (rand::thread_rng().sample::<f32,_>(StandardNormal)) * bend * (t * PI).sin();
            let c = match j.abs() {
                j if j > 2.6 => drawille::PixelColor::Blue,
                j if j > 1.7 => drawille::PixelColor::Cyan,
                j if j > 2.2 => drawille::PixelColor::BrightBlue,
                j if j > 1.2 => drawille::PixelColor::BrightCyan,
                _ => drawille::PixelColor::White,
            };
            let dy = y + j - m as f32 / 2.0 + n as f32;

            canvas.set_colored(0 + x as u32, 3 - 1 + dy as u32, c);
        }
    }

    let (px, py) = (center_x as u16 - max_width as u16 / 2, y as u16);
    drawille_paste(&mut canvas.rows(), px, py);
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


//
// Lil' Helpers
//

fn drawille_paste (rows: &mut Vec<String>, x: u16, y: u16) {
    for (ix, row) in rows.iter().enumerate() {
        print!("{}{}", termion::cursor::Goto(x,y+ix as u16), row);
    }
}

fn line (canvas: &mut Canvas, x1: f32, y1: f32, x2: f32, y2: f32, color: PixelColor) {
    canvas.line_colored(
        x1.round() as u32,
        y1.round() as u32,
        x2.round() as u32,
        y2.round() as u32,
        color);
}

fn rgb (r: u8, g: u8, b: u8) -> PixelColor {
    TrueColor { r, g, b }
}

fn rgb_f32 (r: f32, g: f32, b: f32) -> PixelColor {
    TrueColor {
        r: (r * 255.0) as u8,
        g: (g * 255.0) as u8,
        b: (b * 255.0) as u8
    }
}

fn rand_barcode_char_as_str () -> char {
    " │║┃▌▐▕█▊▋▌▍▎▏".chars().choose(&mut rand::thread_rng()).unwrap()
}

fn lerp (a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_tuple ((ax, ay): (f32, f32), (bx, by): (f32, f32), t: f32) -> (f32, f32) {
    (lerp(ax, bx, t), lerp(ay, by, t))
}

fn barcode_string (len: usize) -> String {
    let mut s = String::new();
    for _ in 0..len {
        s.push(rand_barcode_char_as_str());
    }
    s
}

fn time_now () -> f32 {
    START_TIME.elapsed().as_millis() as f32 / 1000.0
}

fn sin (phase: f32) -> f32 {
    (time_now() + phase).sin()
}

fn nsin (phase: f32) -> f32 {
    sin(phase) * 0.5 + 0.5
}

fn rand_normal (n: f32) -> f32 {
    n * rand::thread_rng().sample::<f32,_>(StandardNormal)
}

fn rand_uniform (n: f32) -> f32 {
    n * rand::random::<f32>()
}



//
// Plots
//

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


