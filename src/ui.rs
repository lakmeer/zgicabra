
use std::io::{Write, Error};
use std::time::Instant;
use std::f32::consts::PI;
use rgb::RGB8;

use rand::prelude::IteratorRandom;
use textplots::{ColorPlot,Chart,Shape};

use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use crate::tw;
use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,Direction,Joystick,NoteState,SignalState,Voice};
use crate::audio::{AudioHandles,AudioErrors};
use crate::tools::*;

use crate::HISTORY_WINDOW;


type Screen = termion::screen::AlternateScreen<std::io::Stdout>;

const TEXT_WIDTH  : u16 = 76;
const TEXT_HEIGHT : u16 = TEXT_WIDTH / 4;

const CANVAS_WIDTH  : u16 = TEXT_WIDTH * 2;
const CANVAS_HEIGHT : u16 = TEXT_HEIGHT * 4;

const WIDTH  : f32 = CANVAS_WIDTH  as f32;
const HEIGHT : f32 = CANVAS_HEIGHT as f32;

const AUDIO_PANEL_Y:u16 = 23;
const DEBUG_PANEL_Y:u16 = 44;
const BOTTOM_Y:u16      = 70;

const BLUE_0:RGB8  = RGB8 { r: 120, g: 150, b: 255 };
const BLUE_1:RGB8  = RGB8 { r: 150, g: 200, b: 255 };
const BLUE_2:RGB8  = RGB8 { r:  60, g:  80, b: 155 };
const BLUE_3:RGB8  = RGB8 { r: 180, g: 180, b: 180 };

const GREEN_0:RGB8 = RGB8 { r: 120, g: 255, b: 150 };
const GREEN_1:RGB8 = RGB8 { r: 150, g: 255, b: 200 };
const GREEN_2:RGB8 = RGB8 { r:  60, g: 155, b: 80 };
const GREEN_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };

const RED_0:RGB8 = RGB8 { r: 200, g: 80, b: 80 };
const RED_1:RGB8 = RGB8 { r: 200, g: 150, b: 150 };
const RED_2:RGB8 = RGB8 { r: 180, g: 50, b: 40 };
const RED_3:RGB8 = RGB8 { r: 180, g: 180, b: 180 };

const WHITE:RGB8 = RGB8 { r: 255, g: 255, b: 255 };
const BLACK:RGB8 = RGB8 { r: 0, g: 0, b: 0 };


pub fn draw_all (
    zgicabra: &Zgicabra,
    history: &Vec<Zgicabra>,
    delta_history: &Vec<DeltaEvent>,
    audio: &AudioHandles) {

    let mut canvas = Canvas::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);

    draw_main_panel(1, &mut canvas, zgicabra);

    // technically overdraws previous output
    draw_banner(TEXT_WIDTH, 1, zgicabra.level == 0.0);
    draw_sequence_pie(zgicabra.sequence_number, TEXT_WIDTH, 1);

    print!("{}{}", goto(1, AUDIO_PANEL_Y),
        barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

    draw_audio_panel(AUDIO_PANEL_Y, zgicabra, audio);

    print!("{}{}", goto(1, DEBUG_PANEL_Y),
        barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

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

    //if zgicabra.most_recent_wand == Hand::Left {
        //print!("{}{}{:^38}", goto(0, 21),  termion::color::Fg(termion::color::LightBlue), format!("X"));
    //}

    //if zgicabra.most_recent_wand == Hand::Right {
        //print!("{}{}{:^38}", goto(38, 21), termion::color::Fg(termion::color::LightBlue), format!("X"));
    //}

    //draw_palette(10, 40);

    draw_graph(DEBUG_PANEL_Y, &history);
    draw_note_state(38, DEBUG_PANEL_Y + 2, &zgicabra);
    draw_events(38, DEBUG_PANEL_Y + 10, &delta_history);

    print!("{}{}", goto(1, BOTTOM_Y),
        barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

}

fn draw_range_label (x: u16, y: u16, label: &str, value: f32, min: f32, max: f32) {
    let norm_value = (value - min) / (max - min);
    let num_half_bars = (norm_value * 16.0).round() as usize;
    let odd_bar = num_half_bars % 2 == 1;
    let num_bars = num_half_bars / 2;
    let range_bar = format!("{}{}", "█".repeat(num_bars), if odd_bar { "▌" } else { "" });

    print!("{}{:>13} {:<8} {:<5.2}", goto(x, y), label, range_bar.to_string(), value);
}

fn draw_audio_panel (y: u16, zgicabra: &Zgicabra, audio: &AudioHandles) {

    draw_range_label(2, y +  2, "master_vol",  audio.master_vol.value(), 0.0, 1.0);

    draw_range_label(2, y +  4, "main_sub",    audio.main_sub_lvl.value(),   0.0, 1.0);
    draw_range_label(2, y +  5, "dry_subl",    audio.dry_sub_lvl.value(),    0.0, 1.0);
    draw_range_label(2, y +  6, "thump_peak",  audio.thump_peak.value(),     1.0, 2.0);
    draw_range_label(2, y +  7, "thump_decay", audio.thump_decay.value(),    0.0, 1.0);
    draw_range_label(2, y +  8, "amp_boost",   audio.amp_boost.value(),      0.0, 1.0);
    draw_range_label(2, y +  9, "amp_blend",   audio.amp_blend.value(),      0.0, 1.0);
    draw_range_label(2, y + 10, "amp_xover",   audio.amp_crossover.value(),  0.0, 2000.0);
    draw_range_label(2, y + 11, "rev_dry",     audio.reverb_dry.value(),     0.0, 1.0);
    draw_range_label(2, y + 12, "rev_decay",   audio.reverb_decay.value(),   0.0, 1.0);
    draw_range_label(2, y + 13, "rev_damp",    audio.reverb_damp.value(),    0.0, 1.0);
    draw_range_label(2, y + 14, "rev_size",    audio.reverb_size.value(),    0.0, 100.0);
    draw_range_label(2, y + 15, "lim_thresh",  audio.limiter_thresh.value(), 0.0, 1.0);

    if matches!(zgicabra.voice, Voice::VoiceB) {
        let growl = &audio.voice_b;
        let input = [
            ("bass_drive",    growl.bass_drive_input.value()),
            ("filter",        growl.filter_input.value()),
            ("space",         growl.space_input.value()),
            ("warp",          growl.warp_input.value()),
            ("nam_crossover", growl.nam_crossover_input.value()),
        ];
        let live = [
            ("filter",    growl.filter_live.value()),
            ("warp",      growl.warp_live.value()),
            ("freq_mult", growl.freq_mult_live.value()),
        ];

        for (row, (name, value)) in input.iter().enumerate() {
            print!("{}{:<14}{:>7.4}", goto(2, y + 1 + row as u16), name, value);
        }
        for (row, (name, value)) in live.iter().enumerate() {
            print!("{}{:<14}{:>7.4}", goto(26, y + 1 + row as u16), name, value);
        }
    }

    draw_audio_errors(y + 19, &audio.errors);
}

fn draw_main_panel (y: u16, canvas: &mut Canvas, zgicabra: &Zgicabra) {

    if !zgicabra.docked {
        draw_wand(canvas, zgicabra.left,  WIDTH*1.0/4.0, HEIGHT/2.0, WIDTH/6.0);
        draw_wand(canvas, zgicabra.right, WIDTH*3.0/4.0, HEIGHT/2.0, WIDTH/6.0);

        if zgicabra.level > 0.0  {
            draw_bend(canvas, zgicabra.separation,
                      zgicabra.left.twist, zgicabra.right.twist,
                      (WIDTH*1.0/4.0) as u32,
                      (WIDTH*3.0/4.0) as u32,
                      (HEIGHT/2.0) as u32,
                      WIDTH/6.0,
                      zgicabra.level);
        }
    } else {
        draw_wand_fixed(canvas, zgicabra.left,  WIDTH*1.0/4.0, HEIGHT/2.0, WIDTH/6.0);
        draw_wand_fixed(canvas, zgicabra.right, WIDTH*3.0/4.0, HEIGHT/2.0, WIDTH/6.0);
    }

    print!("{}{}", goto(1, y), &mut canvas.frame());

    // Voice, width, root, thump/fuzz

    print!("{}{}{}",
        goto(37, y + 18),
        fg(tw::WHITE),
        format_note_name(zgicabra.note.root));

    let width_color = if zgicabra.signal.width > 0.0 { tw::GREEN_400 } else { tw::RED_400 };
    print!("{}{}{:^56}{}",
        goto(11, y + 19),
        fg(width_color),
        "━".repeat((zgicabra.signal.width.abs() * 28.0).round() as usize * 2),
        termion::color::Fg(termion::color::Reset));

    print!("{}{:^76}", goto(0, y + 20), format!("{:?}", zgicabra.voice));

    draw_toggle_box(             5, y + 18, GREEN_0, zgicabra.signal.thump > 0.0);
    draw_toggle_box(TEXT_WIDTH - 8, y + 18, RED_0,   zgicabra.signal.fuzz > 0.0);
}

fn draw_palette (x: u16, y: u16) {
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  0), fg(WHITE), fg(GREEN_0), fg(BLUE_0), fg(RED_0));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  1), fg(BLACK), fg(GREEN_1), fg(BLUE_1), fg(RED_1));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  2), fg(WHITE), fg(GREEN_2), fg(BLUE_2), fg(RED_2));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  3), fg(BLACK), fg(GREEN_3), fg(BLUE_3), fg(RED_3));
    print!("{}", termion::color::Fg(termion::color::Reset));
}

fn draw_toggle_box (x: u16, y: u16, on_color: RGB8, on: bool) {
    print!("{}┌───┐", goto(x, y + 0));
    if on {
        print!("{}│{}▐█▌{}│", goto(x, y + 1), fg(on_color), termion::color::Fg(termion::color::Reset));
    } else {
        print!("{}│   │", goto(x, y + 1));
    }
    print!("{}└───┘", goto(x, y + 2));
}

fn draw_ascii_frame (x: u16, y: u16, width: u16, height: u16) {
    let inner = (width - 2) as usize;
    print!("{}{}", goto(x, y), format!("╔{}╗", "═".repeat(inner)));
    for row in 1..height - 1 {
        print!("{}║{}║", goto(x, y + row), " ".repeat(inner));
    }
    print!("{}╚{}╝", goto(x, y + height - 1), "═".repeat(inner));
}

fn draw_wand (canvas: &mut Canvas, wand: Wand, x: f32, y: f32, radius: f32) {

    let color = electric(wand.trigger * rand_uniform(1.0));
    let facing = -wand.twist - PI/2.0 + if wand.hand == Hand::Left { PI/8.0 } else { -PI/8.0 };
    let stick_facing = match wand.stick.octant {
        Direction::None => facing,
        _ => facing - PI*3.0/4.0 + PI/4.0 * wand.stick.octant as i32 as f32
    };

    draw_joystick_spokes(canvas, wand, x, y, radius, facing, color);
    draw_joystick_position(canvas, wand, x, y, radius, facing, stick_facing);
    draw_trigger_fx(canvas, wand, x, y, radius, stick_facing);
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

    print!("{}{}{}{}", goto(1,y),
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
        print!("{}{}", goto(x,y+ix as u16), row);
    }
}

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

pub fn draw_graph (y: u16, history: &Vec<Zgicabra>) {

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

    print!("{}", goto(1, y+2));
    Chart::new_with_y_range(70, 90, 0.0, n as f32, -600.0, 600.0)
        .linecolorplot(&Shape::Lines(&left_jerk), GREEN_3)
        .linecolorplot(&Shape::Lines(&left_acc),  GREEN_2)
        .linecolorplot(&Shape::Lines(&left_vel),  GREEN_1)
        .linecolorplot(&Shape::Lines(&left_pos),  GREEN_0)
        .linecolorplot(&Shape::Lines(&right_jerk), BLUE_3)
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

pub fn draw_events (x: u16, y: u16, delta_history: &Vec<DeltaEvent>) {
    print!("{}{}", goto(x, y), termion::color::Fg(termion::color::White));

    for row in 0..12 {
        match delta_history.iter().rev().nth(row) {
            Some(e) => println!("{}- {:?}", goto(x, y + row as u16), e),
            None    => println!("{}-",      goto(x, y + row as u16)),
        }
    }
}

pub fn draw_note_state (x: u16, y: u16, state: &Zgicabra) {
    println!("{}Root:    {:>17}",   goto(x, y + 0), format_note(state.note.root));
    println!("{}Current: {:>17}",   goto(x, y + 1), format_note(state.note.current));
    println!("{}Pitch:   {:>17.4}", goto(x, y + 2), state.note.bend);
    println!("{}Filter:  {:>17.4}", goto(x, y + 3), state.signal.filter);
    println!("{}Fuzz:    {:>17.4}", goto(x, y + 4), state.signal.fuzz);
    println!("{}Width:   {:>17.4}", goto(x, y + 5), state.signal.width);
    println!("{}Thump:   {:>17.4}", goto(x, y + 6), state.signal.thump);
}

fn draw_sequence_pie (seq: u8, x: u16, y: u16) {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let step = (seq as usize / 32).min(LEVELS.len() - 1);
    print!("{}{}", goto(x, y), LEVELS[step]);
}

pub fn draw_audio_errors (y: u16, errors: &AudioErrors) {
    let log = errors.lock().unwrap();
    print!("{} {}▪", goto(2, y), fg(GREEN_0));
    print!("{} {}", goto(2, y), fg(RED_0));
    for e in log.iter() {
        print!("▪");
    }
    print!("{}", termion::color::Fg(termion::color::Reset));
}


fn goto (x: u16, y: u16) -> termion::cursor::Goto {
    termion::cursor::Goto(x, y)
}

fn fg (color: RGB8) -> termion::color::Fg<termion::color::Rgb> {
    termion::color::Fg(termion::color::Rgb(color.r, color.g, color.b))
}
