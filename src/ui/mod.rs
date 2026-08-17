
use std::f32::consts::PI;
use rgb::RGB8;

use textplots::{ColorPlot,Chart,Shape};

use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,Direction,Joystick,NoteState,SignalState,Voice};
use crate::audio::{AudioHandles,AudioErrors,SwarmView};
use crate::tools::*;

mod tw;
mod utils;
mod panel_debug;
mod panel_main;
mod voice_panel;

use utils::*;
use panel_debug::{draw_debug_panel};
use panel_main::{draw_main_panel};
use voice_panel::{draw_voice_panel};

type Screen = termion::screen::AlternateScreen<std::io::Stdout>;


const TEXT_WIDTH : u16 = 76;

const MAIN_PANEL_Y  : u16 = 1;
const AUDIO_PANEL_Y : u16 = 23;
const DEBUG_PANEL_Y : u16 = 44;
const BOTTOM_Y      : u16 = 70;



pub fn draw_all (
    zgicabra: &Zgicabra,
    history: &Vec<Zgicabra>,
    delta_history: &Vec<DeltaEvent>,
    audio: &AudioHandles) {

    draw_main_panel(1, TEXT_WIDTH, zgicabra);

    print!("{}{}", goto(1, AUDIO_PANEL_Y), barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

    draw_voice_panel(AUDIO_PANEL_Y, zgicabra, audio);

    print!("{}{}", goto(1, DEBUG_PANEL_Y), barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

    //draw_palette(10, 40);

    draw_debug_panel(DEBUG_PANEL_Y, &zgicabra, &history, &delta_history);

    print!("{}{}", goto(1, BOTTOM_Y),
        barcode_string(TEXT_WIDTH.into(), zgicabra.level == 0.0));

}

fn draw_palette (x: u16, y: u16) {
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  0), fg(WHITE), fg(GREEN_0), fg(BLUE_0), fg(RED_0));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  1), fg(BLACK), fg(GREEN_1), fg(BLUE_1), fg(RED_1));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  2), fg(WHITE), fg(GREEN_2), fg(BLUE_2), fg(RED_2));
    print!("{}{}▐█▌{}▐█▌{}▐█▌{}▐█▌", goto(x, y +  3), fg(BLACK), fg(GREEN_3), fg(BLUE_3), fg(RED_3));
    print!("{}", termion::color::Fg(termion::color::Reset));
}

fn draw_ascii_frame (x: u16, y: u16, width: u16, height: u16) {
    let inner = (width - 2) as usize;
    print!("{}{}", goto(x, y), format!("╔{}╗", "═".repeat(inner)));
    for row in 1..height - 1 {
        print!("{}║{}║", goto(x, y + row), " ".repeat(inner));
    }
    print!("{}╚{}╝", goto(x, y + height - 1), "═".repeat(inner));
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

