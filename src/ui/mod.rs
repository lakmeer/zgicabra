
use std::f32::consts::PI;
use rgb::RGB8;

use drawille::{Canvas,PixelColor};
use drawille::PixelColor::TrueColor;

use crate::hydra::HydraState;
use crate::zgicabra::{DeltaEvent,Zgicabra,Wand,Hand,Direction,Joystick,NoteState,SignalState};
use crate::audio::{AudioHandles,AudioErrors,SwarmView};
use crate::tools::*;

mod tw;
mod utils;
mod comp_meter;
mod panel_debug;
mod panel_main;
mod panel_voice;

use utils::*;
use panel_debug::{draw_debug_panel};
use panel_main::{draw_main_panel};
use panel_voice::{draw_voice_panel};

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

    let buzz = zgicabra.trigger_total >= 0.0;
    let banner_text = " zgicabra ";
    let stripe_length = (TEXT_WIDTH - banner_text.len() as u16) / 2;

    print!("{}", fg(tw::WHITE));

    print!("{}{}{}{}", goto(1,1),
        barcode_string(stripe_length, buzz),
        banner_text,
        barcode_string(stripe_length - 3, buzz));

    draw_main_panel(1, TEXT_WIDTH, zgicabra, audio);

    print!("{}{}", goto(1, AUDIO_PANEL_Y), barcode_string(TEXT_WIDTH - 3, buzz));

    draw_voice_panel(AUDIO_PANEL_Y, zgicabra, audio);

    print!("{}{}", goto(1, DEBUG_PANEL_Y), barcode_string(TEXT_WIDTH - 0, buzz));

    draw_debug_panel(DEBUG_PANEL_Y, &zgicabra, &history, &delta_history, audio);

    print!("{}{}", goto(1, BOTTOM_Y), barcode_string(TEXT_WIDTH, buzz));

}

