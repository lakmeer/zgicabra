
use std::io::{Write, Error};
use std::time::Instant;
use std::f32::consts::PI;

use rgb::RGB8;

use rand::prelude::IteratorRandom;
use textplots::{ColorPlot,Chart,Shape};

use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use crate::midi_event::MidiEvent;
use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,Direction,Joystick,NoteState,SignalState};
use crate::tools::*;

use crate::HISTORY_WINDOW;


type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


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

pub fn draw_all (zgicabra: &Zgicabra, history: &Vec<Zgicabra>) {

    // Text dimensions
    const TEXT_WIDTH  : u16 = 76;
    const TEXT_HEIGHT : u16 = TEXT_WIDTH / 4;

    // Pixel dimensions
    const CANVAS_WIDTH  : u16 = TEXT_WIDTH * 2;
    const CANVAS_HEIGHT : u16 = TEXT_HEIGHT * 4;

    // Vector dimensions
    const WIDTH  : f32 = CANVAS_WIDTH  as f32;
    const HEIGHT : f32 = CANVAS_HEIGHT as f32;

    // Canvas
    let mut canvas = Canvas::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);

    draw_banner(TEXT_WIDTH, 1, zgicabra.level == 0.0);

    if !zgicabra.docked {
        draw_wand(&mut canvas, zgicabra.left,  WIDTH*1.0/4.0, HEIGHT/2.0, WIDTH/6.0);
        draw_wand(&mut canvas, zgicabra.right, WIDTH*3.0/4.0, HEIGHT/2.0, WIDTH/6.0);

        if zgicabra.level > 0.0  {
            draw_bend(&mut canvas, zgicabra.separation,
                      zgicabra.left.twist, zgicabra.right.twist,
                      (WIDTH*1.0/4.0) as u32,
                      (WIDTH*3.0/4.0) as u32,
                      (HEIGHT/2.0) as u32,
                      WIDTH/6.0,
                      zgicabra.level);
        }
    } else {
        draw_wand_fixed(&mut canvas, zgicabra.left,  WIDTH*1.0/4.0, HEIGHT/2.0, WIDTH/6.0);
        draw_wand_fixed(&mut canvas, zgicabra.right, WIDTH*3.0/4.0, HEIGHT/2.0, WIDTH/6.0);
    }

    // Output canvas
    print!("{}{}", termion::cursor::Goto(1, 2), &mut canvas.frame());
    print!("{}{}", termion::cursor::Goto(1, TEXT_HEIGHT + 4), barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

    //print!("{}", history[0].sequence_number);
}


//
// Sub-drawing Functions
//

fn draw_wand (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32) {

    let color = electric(wand.trigger * rand_uniform(1.0));
    let facing = -wand.twist - PI/2.0 + if wand.hand == Hand::Left { PI/8.0 } else { -PI/8.0 };
    let stick_facing = match wand.stick.octant {
        Direction::None => facing,
        _ => facing - PI*3.0/4.0 + PI/4.0 * wand.stick.octant as i32 as f32
    };

    // Joystick Spokes
    draw_joystick_spokes(canvas, wand, x, y, radius, facing, color);
    draw_joystick_position(canvas, wand, x, y, radius, facing, stick_facing);

    // Trigger
    draw_trigger_fx(canvas, wand, x, y, radius, stick_facing);

    // Buttons
    draw_buttons(canvas, &wand, x, y, radius, facing);
    if wand.home { draw_home_button(canvas, x, y, radius, facing); }
    if wand.bumper { draw_bumper(canvas, x, y, radius, facing); }
}


fn draw_wand_fixed (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32) {
    draw_joystick_spokes(canvas, wand, x, y, radius, -PI/2.0, PixelColor::White);
}


fn draw_joystick_spokes (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32, angle: f32, color: PixelColor) {
    for i in 0..8 {
        let a = angle + (i as f32/4.0) * PI + 0.125 * PI;
        line(canvas, x, y, x + radius * a.cos(), y + radius * a.sin(), color);
    }
}


fn draw_joystick_position (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32, facing: f32, stick_facing: f32) {
    if wand.stick.octant != Direction::None {
        let a = if wand.trigger > 0.0 { stick_facing } else { facing + wand.stick.theta * 2.0 * PI };
        line(canvas,
             x + 1.15 * radius * (a - PI/8.0).cos(),
             y + 1.15 * radius * (a - PI/8.0).sin(),
             x + 1.15 * radius * (a + PI/8.0).cos(), 
             y + 1.15 * radius * (a + PI/8.0).sin(),
             PixelColor::White);
    }
}


fn draw_trigger_fx (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32, stick_facing: f32) {
    if wand.trigger > 0.0 {
        for i in 0..128 {
            match wand.stick.octant {

                Direction::None => {
                    let a = i as f32 / 128.0 * 2.0 * PI;
                    let (j, c) = breakup(wand.trigger, 1.0);
                    let len = (j * 0.6 + 0.2) * radius;

                    pset(canvas, x + len * a.cos(), y + len * a.sin(), c);

                    let a = i as f32 / 128.0 * 2.0 * PI;
                    let (j, c) = breakup(wand.trigger, 7.0);
                    let len = 0.5 * wand.trigger * (1.0 - wand.stick.r) * radius + 2.0 * sin(3.9, a as f32 * 0.7);
                    pset(canvas, x + (len - j * 2.0) * a.cos(), y + (len - j * 2.0) * a.sin(), c);
                    pset(canvas, x + (len + j * 2.0) * a.cos(), y + (len + j * 2.0) * a.sin(), PixelColor::White);
                    pset(canvas, x + (len) * a.cos(), y + (len) * a.sin(), c);
                },

                _ => {
                    // TODO: Collect sparks in from behind facing direction to focus them on the
                    // selected quadrant like a cardiod
                    let a = stick_facing - (i as f32 / 128.0 * PI/4.0) + PI * 2.0 * (1.0 - wand.trigger) + PI/8.0;
                    let (j, c) = breakup(ease_in(wand.trigger * wand.trigger), 2.0);
                    let len = (j * 0.7) * radius * wand.trigger;

                    pset(canvas, x + len.abs() * a.cos(), y + len.abs() * a.sin(), c);

                    let a = stick_facing - i as f32 / 128.0 * PI/4.0 + PI/8.0;
                    let (j, c) = breakup(ease_in(wand.trigger * wand.trigger), 7.0);
                    let len = 0.8 * wand.trigger * wand.stick.r * radius - 2.0 * sin(4.0, a as f32);

                    pset(canvas, x + (len + j) * a.cos(), y + (len + j) * a.sin(), c);
                    pset(canvas, x + len * a.cos(), y + len * a.sin(), PixelColor::White);
                }

            }
        }
    }
}


fn draw_bumper (canvas: &mut Canvas, x: f32, y: f32, radius: f32, angle: f32) {
    for i in 0..3 {
        line(canvas,
             x + 1.3 * radius * (i as f32 * PI/4.0 - PI/4.0 + angle - PI/8.0).cos(),
             y + 1.3 * radius * (i as f32 * PI/4.0 - PI/4.0 + angle - PI/8.0).sin(),
             x + 1.3 * radius * (i as f32 * PI/4.0 - PI/4.0 + angle + PI/8.0).cos(), 
             y + 1.3 * radius * (i as f32 * PI/4.0 - PI/4.0 + angle + PI/8.0).sin(),
             electric(rand_uniform(1.0)));
    }
}


fn draw_buttons (canvas: &mut Canvas, wand: &Wand, x: f32, y: f32, radius: f32, angle: f32) {
    for (ix, button) in wand.buttons.iter().enumerate() {
        let angular_offset = ix as f32 * PI/4.0;

        let pos = match wand.hand {
            Hand::Right   => angle - PI/2.0 - PI/8.0 - angular_offset,
            Hand::Left    => angle + PI/2.0 + PI/8.0 + angular_offset,
            Hand::Neither => angle,
        };

        let c = electric(rand_uniform(1.0));
        let px = x + 1.3 * radius * pos.cos();
        let py = y + 1.3 * radius * pos.sin();

        if *button {
            polygon(canvas, px, py, 3, 3.0, pos + rand_uniform(PI), c);
        } else {
            pset(canvas, px, py, PixelColor::White);
        }
    }
}


fn draw_home_button (canvas: &mut Canvas, x: f32, y: f32, radius: f32, angle: f32) {
    for i in 3..6 {
        line(canvas,
             x + 1.3 * radius * (i as f32 * PI/4.0 + angle - PI/10.0).cos(),
             y + 1.3 * radius * (i as f32 * PI/4.0 + angle - PI/10.0).sin(),
             x + 1.3 * radius * (i as f32 * PI/4.0 + angle + PI/10.0).cos(), 
             y + 1.3 * radius * (i as f32 * PI/4.0 + angle + PI/10.0).sin(),
             electric(rand_uniform(1.0)));
    }
}
                     

fn draw_banner (width: u16, y: u16, solid: bool) {
    let banner_text = " zgicabra ";
    let stripe_length = (width - banner_text.len() as u16) / 2;

    print!("{}{}{}{}", termion::cursor::Goto(1,y),
        barcode_string(stripe_length.into(), solid),
        banner_text,
        barcode_string(stripe_length.into(), solid));
}


fn draw_bend (canvas: &mut Canvas, sep: f32, left_angle: f32, right_angle: f32, left: u32, right: u32, y: u32, r: f32, level: f32) {
    let p1 = (left  as f32,  y as f32);
    let p2 = (left  as f32 + r * left_angle.cos(),  y as f32 - r * left_angle.sin());
    let p3 = (right as f32 - r * right_angle.cos(), y as f32 + r * right_angle.sin());
    let p4 = (right as f32, y as f32);

    let m = 1 + (sep/500.0).powf(2.0) as u32;
    let bend = left_angle - right_angle;

    // Debug lines
    //line(canvas, p1.0, p1.1, p2.0, p2.1, PixelColor::Yellow);
    //line(canvas, p3.0, p3.1, p4.0, p4.1, PixelColor::Magenta);

    for x in 0..400 {
        let t = x as f32/400.0;
        let q1 = lerp_tuple(lerp_tuple(p1, p2, t), lerp_tuple(p2, p3, t), t);
        let q2 = lerp_tuple(lerp_tuple(p2, p3, t), lerp_tuple(p3, p4, t), t);
        let (x, y) = lerp_tuple(q1, q2, t);

        canvas.set_colored(x as u32, 2 + y as u32, PixelColor::White);

        for n in 0..m {
            let (j, c) = breakup(level, 4.0);
            let c = electric(j.abs() / 3.0);
            let dy = y + 3.0 * j * (t*PI).sin().powf(2.0) - m as f32 / 2.0 + n as f32;
            canvas.set_colored(x as u32, 2 + dy as u32, c);
        }
    }
}



//
// Drawille Wrappers
//

fn pset (canvas: &mut Canvas, x1: f32, y1: f32, color: PixelColor) {
    canvas.set_colored(
        x1.round() as u32,
        y1.round() as u32,
        color);
}

fn line (canvas: &mut Canvas, x1: f32, y1: f32, x2: f32, y2: f32, color: PixelColor) {
    canvas.line_colored(
        x1.round() as u32,
        y1.round() as u32,
        x2.round() as u32,
        y2.round() as u32,
        color);
}

fn polygon (canvas: &mut Canvas, x1: f32, y1: f32, n: u16, r: f32, a: f32, color: PixelColor) {
    for i in 0..n {
        let a1 = a +   i   as f32 * 2.0 * PI / n as f32;
        let a2 = a + (i+1) as f32 * 2.0 * PI / n as f32;
        let x2 = x1 + r * a1.cos();
        let y2 = y1 + r * a1.sin();
        let x3 = x1 + r * a2.cos();
        let y3 = y1 + r * a2.sin();
        line(canvas, x2, y2, x3, y3, color);
    }
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

fn electric (n: f32) -> PixelColor {
    match 3.0 * n.abs() {
        i if i > 2.6 => drawille::PixelColor::Blue,
        i if i > 1.7 => drawille::PixelColor::Cyan,
        i if i > 2.2 => drawille::PixelColor::BrightBlue,
        i if i > 1.2 => drawille::PixelColor::BrightCyan,
        _ => drawille::PixelColor::White,
    }
}

fn breakup (n: f32, r: f32) -> (f32, PixelColor) {
    let j = r * rand_normal(1.0) * n;
    let c = match j.abs() {
        j if j > 2.6 => drawille::PixelColor::Blue,
        j if j > 1.7 => drawille::PixelColor::Cyan,
        j if j > 2.2 => drawille::PixelColor::BrightBlue,
        j if j > 1.2 => drawille::PixelColor::BrightCyan,
        _ => drawille::PixelColor::White,
    };
    (j / 3.0, c)
}

fn drawille_paste (rows: &mut Vec<String>, x: u16, y: u16) {
    for (ix, row) in rows.iter().enumerate() {
        print!("{}{}", termion::cursor::Goto(x,y+ix as u16), row);
    }
}


//
// Lil' Helpers
//

fn lerp (a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_tuple ((ax, ay): (f32, f32), (bx, by): (f32, f32), t: f32) -> (f32, f32) {
    (lerp(ax, bx, t), lerp(ay, by, t))
}

fn barcode_string (len: usize, solid: bool) -> String {
    let mut s = String::new();
    for _ in 0..len {
        s.push(rand_barcode_char_as_str(solid));
    }
    s
}

fn rand_barcode_char_as_str (solid: bool) -> char {
    if !solid {
        " │║┃▌▐▕█▊▋▌▍▎▏".chars().choose(&mut rand::thread_rng()).unwrap()
    } else {
        "█".chars().nth(0).unwrap()
    }
}


//
// Plots and Readouts
//

pub fn draw_graph (history: &Vec<Zgicabra>) {

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
                left_vel[i]   = (i as f32, frame.left.scalar_vel   *   -100.0);
                right_vel[i]  = (i as f32, frame.right.scalar_vel  *    100.0);
                left_acc[i]   = (i as f32, frame.left.scalar_acc   *   -800.0);
                right_acc[i]  = (i as f32, frame.right.scalar_acc  *    800.0);
                left_jerk[i]  = (i as f32, frame.left.scalar_jerk  * -60000.0);
                right_jerk[i] = (i as f32, frame.right.scalar_jerk *  60000.0);
            }
        }
    }

    print!("{}", termion::cursor::Goto(1, 39));
    Chart::new_with_y_range(140, 140, 0.0, n as f32, -500.0, 500.0)
        .linecolorplot(&Shape::Lines(&left_jerk), GREEN_3)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_jerk), BLUE_3)
        .linecolorplot(&Shape::Lines(&right_acc),  BLUE_2)
        .linecolorplot(&Shape::Lines(&right_vel),  BLUE_0)
        .linecolorplot(&Shape::Lines(&right_pos),  BLUE_1)
        .display();
}

pub fn draw_events (delta_events: &Vec<DeltaEvent>, midi_events: &Vec<MidiEvent>) {
    print!("{}", termion::cursor::Goto(1, 26));
    for i in 0..22 {
        println!("                                                                                 ");
    }

    println!("{}Delta events: {}", termion::cursor::Goto(1, 26), delta_events.len());
    for (ix, event) in delta_events.iter().enumerate() {
        println!("{}- {:?}", termion::cursor::Goto(1, 28 + ix as u16), event);
    }

    println!("{}MIDI events:  {}", termion::cursor::Goto(29, 26), midi_events.len());
    for (ix, event) in midi_events.iter().enumerate() {
        println!("{}- {:?}", termion::cursor::Goto(30, 28 + ix as u16), event);
    }
}

pub fn draw_note_state (note_state: &NoteState, signal_state: &SignalState) {
    println!("{}Note: [{}]", termion::cursor::Goto(58, 26), if note_state.on { note_state.current } else { 0 });
    println!("{}- Root:    {}", termion::cursor::Goto(58, 28), format_note(note_state.root));
    println!("{}- Current: {}", termion::cursor::Goto(58, 29), format_note(note_state.current));
    println!("{}- Pitch:   {}", termion::cursor::Goto(58, 30), note_state.bend);

    println!("{}Signals:", termion::cursor::Goto(58, 32));
    println!("{}- Filter: {}", termion::cursor::Goto(58, 34), signal_state.filter);
    println!("{}- Fuzz:   {}", termion::cursor::Goto(58, 35), signal_state.fuzz);
    println!("{}- Width:  {}", termion::cursor::Goto(58, 36), signal_state.width);
    println!("{}- Thump:  {}", termion::cursor::Goto(58, 37), signal_state.thump);
    println!("{}- |vel|:  {}", termion::cursor::Goto(58, 38), signal_state.velocity);
    println!("{}- |acc|:  {}", termion::cursor::Goto(58, 39), signal_state.acceleration);
    println!("{}- |jrk|:  {}", termion::cursor::Goto(58, 40), signal_state.jerk);
}

