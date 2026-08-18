
use std::f32::consts::PI;
use termion;
use rgb::RGB8;
use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;
use rand::prelude::IteratorRandom;

pub const BLUE_0:RGB8  = RGB8 { r: 120, g: 150, b: 255 };
pub const BLUE_1:RGB8  = RGB8 { r: 150, g: 200, b: 255 };
pub const BLUE_2:RGB8  = RGB8 { r:  60, g:  80, b: 155 };
pub const BLUE_3:RGB8  = RGB8 { r: 180, g: 180, b: 180 };

pub const GREEN_0:RGB8 = RGB8 { r: 120, g: 255, b: 150 };
pub const GREEN_1:RGB8 = RGB8 { r: 150, g: 255, b: 200 };
pub const GREEN_2:RGB8 = RGB8 { r:  60, g: 155, b: 80 };
pub const GREEN_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };

pub const RED_0:RGB8 = RGB8 { r: 200, g: 80, b: 80 };
pub const RED_1:RGB8 = RGB8 { r: 200, g: 150, b: 150 };
pub const RED_2:RGB8 = RGB8 { r: 180, g: 50, b: 40 };
pub const RED_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };

pub const WHITE:RGB8 = RGB8 { r: 255, g: 255, b: 255 };
pub const BLACK:RGB8 = RGB8 { r: 0, g: 0, b: 0 };

pub const FG_RESET: termion::color::Fg<termion::color::Reset> = termion::color::Fg(termion::color::Reset);


// General

pub fn lerp (a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}


pub fn lerp_tuple ((ax, ay): (f32, f32), (bx, by): (f32, f32), t: f32) -> (f32, f32) {
    (lerp(ax, bx, t), lerp(ay, by, t))
}


// Termion

pub fn goto (x: u16, y: u16) -> termion::cursor::Goto {
    termion::cursor::Goto(x, y)
}

pub fn fg (color: RGB8) -> termion::color::Fg<termion::color::Rgb> {
    termion::color::Fg(termion::color::Rgb(color.r, color.g, color.b))
}

pub fn bg (color: RGB8) -> termion::color::Bg<termion::color::Rgb> {
    termion::color::Bg(termion::color::Rgb(color.r, color.g, color.b))
}


// Drawille

pub fn pset (canvas: &mut Canvas, x1: f32, y1: f32, color: PixelColor) {
    canvas.set_colored(
        x1.round() as u32,
        y1.round() as u32,
        color);
}

pub fn line (canvas: &mut Canvas, x1: f32, y1: f32, x2: f32, y2: f32, color: PixelColor) {
    canvas.line_colored(
        x1.round() as u32,
        y1.round() as u32,
        x2.round() as u32,
        y2.round() as u32,
        color);
}

pub fn polygon (canvas: &mut Canvas, x1: f32, y1: f32, n: u16, r: f32, a: f32, color: PixelColor) {
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

pub fn circle (canvas: &mut Canvas, x: f32, y: f32, r: f32, color: PixelColor) {
    let a = 0.0;
    let b = 2.0 * PI;
    let n = 100;
    for i in 0..n {
        let a1 = a +   i   as f32 * 2.0 * PI / n as f32;
        let a2 = a + (i+1) as f32 * 2.0 * PI / n as f32;
        let x2 = x + r * a1.cos();
        let y2 = y + r * a1.sin();
        let x3 = x + r * a2.cos();
        let y3 = y + r * a2.sin();
        line(canvas, x2, y2, x3, y3, color);
    }
}

pub fn drawille_paste (rows: &mut Vec<String>, x: u16, y: u16) {
    for (ix, row) in rows.iter().enumerate() {
        print!("{}{}", goto(x,y+ix as u16), row);
    }
}


// Special

fn rand_barcode_char_as_str (solid: bool) -> char {
    if !solid {
        " │║┃▌▐▕█▊▋▌▍▎▏".chars().choose(&mut rand::thread_rng()).unwrap()
    } else {
        "█".chars().nth(0).unwrap()
    }
}

pub fn barcode_string (len: usize, solid: bool) -> String {
    let mut s = String::new();
    for _ in 0..len {
        s.push(rand_barcode_char_as_str(solid));
    }
    s
}

pub fn draw_palette (x: u16, y: u16) {
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  0), fg(WHITE), fg(GREEN_0), fg(BLUE_0), fg(RED_0));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  1), fg(BLACK), fg(GREEN_1), fg(BLUE_1), fg(RED_1));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  2), fg(WHITE), fg(GREEN_2), fg(BLUE_2), fg(RED_2));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  3), fg(BLACK), fg(GREEN_3), fg(BLUE_3), fg(RED_3));
    print!("{}", termion::color::Fg(termion::color::Reset));
}


