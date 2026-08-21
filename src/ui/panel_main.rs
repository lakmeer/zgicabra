use std::f32::consts::PI;

use rgb::RGB8;
use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use crate::hydra::HydraState;
use crate::zgicabra::{Zgicabra,DeltaEvent,Hand,Wand,Direction};
use crate::tools::*;

use super::utils::*;
use super::tw;

const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];


fn draw_toggle_box (x: u16, y: u16, on_color: RGB8, on: bool) {
    print!("{}┌───┐", goto(x, y + 0));
    if on {
        print!("{}│{}▐█▌{}│", goto(x, y + 1), fg(on_color), termion::color::Fg(termion::color::Reset));
    } else {
        print!("{}│   │", goto(x, y + 1));
    }
    print!("{}└───┘", goto(x, y + 2));
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


fn draw_bend (canvas: &mut Canvas, sep: f32, left_angle: f32, right_angle: f32, left: u32, right: u32, y: u32, r: f32, trigger_total: f32) {
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
            let (j, c) = breakup(trigger_total, 4.0);
            let c = electric(j.abs() / 3.0);
            let dy = y + 3.0 * j * (t*PI).sin().powf(2.0) - m as f32 / 2.0 + n as f32;
            canvas.set_colored(x as u32, 2 + dy as u32, c);
        }
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




//
// Main Panel
//

pub fn draw_main_panel (y: u16, width: u16, zgicabra: &Zgicabra) {

    let text_height   : u16 = width / 4;
    let canvas_width  : u16 = width * 2;
    let canvas_height : u16 = text_height * 4;

    let width_f  : f32 = canvas_width  as f32;
    let height_f : f32 = canvas_height as f32;

    let mut canvas = Canvas::new(canvas_width as u32, canvas_height as u32);

    if !zgicabra.docked {
        draw_wand(&mut canvas, zgicabra.left,  width_f*1.0/4.0, height_f/2.0, width_f/6.0);
        draw_wand(&mut canvas, zgicabra.right, width_f*3.0/4.0, height_f/2.0, width_f/6.0);

        if zgicabra.trigger_total > 0.0 {
            draw_bend(&mut canvas, zgicabra.separation,
                      zgicabra.left.twist, zgicabra.right.twist,
                      (width_f*1.0/4.0) as u32,
                      (width_f*3.0/4.0) as u32,
                      (height_f/2.0) as u32,
                      width_f/6.0,
                      zgicabra.trigger_total);
        }
    } else {
        draw_wand_fixed(&mut canvas, zgicabra.left,  width_f*1.0/4.0, height_f/2.0, width_f/6.0);
        draw_wand_fixed(&mut canvas, zgicabra.right, width_f*3.0/4.0, height_f/2.0, width_f/6.0);
    }

    print!("{}{}", goto(1, y), &mut canvas.frame());


    // Header and sequence number

    let banner_text = " zgicabra ";
    let stripe_length = (width - banner_text.len() as u16) / 2;

    print!("{}{}{}{}", goto(1,y),
        barcode_string(stripe_length.into(), zgicabra.trigger_total == 0.0),
        banner_text,
        barcode_string(stripe_length.into(), zgicabra.trigger_total == 0.0));

    let phase = zgicabra.seq_num as usize % 128;
    let step = (phase * LEVELS.len() / 128).min(LEVELS.len() - 1);
    let invert = if zgicabra.seq_num < 128 {
        format!("{}", termion::style::Invert)
    } else {
        format!("{}", termion::style::NoInvert)
    };
    let led = format!("{}{}{}{}{}",
        invert,
        fg(tw::GREEN_500),
        LEVELS[step],
        termion::style::NoInvert,
        FG_RESET);

    print!("{}▌{}▐", goto(width - 2, y), led);

    // Voice, width, root, thump/fuzz, most-recent-wand

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

    if zgicabra.most_recent_wand == Hand::Left {
        print!("{}{}▪{}", goto(35, y + 18), fg(BLUE_0), FG_RESET);
    }
    if zgicabra.most_recent_wand == Hand::Right {
        print!("{}{}▪{}", goto(41, y + 18), fg(BLUE_0), FG_RESET);
    }

    print!("{}{:>13} {}", goto(2, y + 16), "delta", zgicabra.left.trigger_delta);
    print!("{}{:>13} {}", goto(40, y + 16), "delta", zgicabra.right.trigger_delta);

    draw_toggle_box(        5, y + 18, GREEN_0, zgicabra.signal.thump > 0.0);
    draw_toggle_box(width - 8, y + 18, RED_0,   zgicabra.signal.fuzz > 0.0);

}
