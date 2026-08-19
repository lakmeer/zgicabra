
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
mod panel_voice;

use utils::*;
use panel_debug::{draw_debug_panel};
use panel_main::{draw_main_panel};
use panel_voice::{draw_voice_panel};

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

    print!("{}{}", goto(1, AUDIO_PANEL_Y), barcode_string((TEXT_WIDTH - 3).into(), zgicabra.trigger_total == 0.0));

    draw_voice_panel(AUDIO_PANEL_Y, zgicabra, audio);

    print!("{}{}", goto(1, DEBUG_PANEL_Y), barcode_string((TEXT_WIDTH - 0).into(), zgicabra.trigger_total == 0.0));

    draw_debug_panel(DEBUG_PANEL_Y, &zgicabra, &history, &delta_history);

    print!("{}{}", goto(1, BOTTOM_Y),
        barcode_string(TEXT_WIDTH.into(), zgicabra.trigger_total == 0.0));

}

