
use std::time::Duration;

use crate::hydra::HydraState;
use crate::sixense::ControllerFrame;


//
// Data Types
//

pub enum Direction {
    LEFT,
    UPLEFT,
    NONE,
    UP,
    UPRIGHT,
    RIGHT,
    DOWNRIGHT,
    DOWN,
    DOWNLEFT,
}

pub struct Joystick {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub theta: f32,
    pub quadrant: Direction,
    pub octant: Direction,
}

impl Joystick {
    pub fn new() -> Joystick {
        Joystick {
            x: 0.0,
            y: 0.0,
            r: 0.0,
            theta: 0.0,
            quadrant: Direction::NONE,
            octant: Direction::NONE,
        }
    }
}

pub struct Wand {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub vel: [f32; 3],
    pub acc: [f32; 3],
    pub scalar_vel: f32,
    pub scalar_acc: f32,
    pub trigger: f32,
    pub bumper: bool,
    pub home: bool,
    pub buttons: [bool; 4],
    pub stick: Joystick,
}

impl Wand {
    pub fn new() -> Wand {
        Wand {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0, 0.0],
            vel: [0.0, 0.0, 0.0],
            acc: [0.0, 0.0, 0.0],
            scalar_vel: 0.0,
            scalar_acc: 0.0,
            trigger: 0.0,
            bumper: false,
            home: false,
            buttons: [false, false, false, false],
            stick: Joystick::new()
        }
    }
}


pub struct ControlSignal {
    value: f32,
    channel: u8,
}


//
// Main Datatype
//

pub struct Zgicabra {
    pub left:  Wand,
    pub right: Wand,
    pub separation: f32,
    pub root_note: u8,
    pub pitch: i16,
}

impl Zgicabra {
    pub fn new() -> Zgicabra {
        Zgicabra {
            left:  Wand::new(),
            right: Wand::new(),
            separation: 0.0,
            root_note: 60,
            pitch: 0,
        }
    }
}


//
// Module Functions
//

use std::io::{Error};

pub fn update (zgicabra: &mut Zgicabra, hydra_state: &HydraState) -> Result<(), Error> {

    // Recent hydra history

    let left_frame  = hydra_state.get_nth_most_recent_frame(0, 0).unwrap();
    let right_frame = hydra_state.get_nth_most_recent_frame(1, 0).unwrap();


    // Map immediately updated values

    copy_frame_to_wand(&left_frame,  &mut zgicabra.left);
    copy_frame_to_wand(&right_frame, &mut zgicabra.right);

    zgicabra.separation = zgicabra.left.pos[0] - zgicabra.right.pos[0];


    // Get frame before this frame (could be None)

    let left_prev_frame   = hydra_state.get_nth_most_recent_frame(0, 1);
    let right_prev_frame  = hydra_state.get_nth_most_recent_frame(1, 1);

    if left_prev_frame.is_none() || right_prev_frame.is_none() {
        return Ok(());
    }

    let left_prev_frame   = left_prev_frame.unwrap();
    let right_prev_frame  = right_prev_frame.unwrap();


    // Time derivatives

    let delta:f32 = hydra_state.timedelta.as_millis() as f32;

    zgicabra.left.vel  = derivative_r3(&left_frame.pos,  &left_prev_frame.pos, delta);
    zgicabra.left.scalar_vel = hyp(&zgicabra.left.vel);

    zgicabra.right.vel = derivative_r3(&right_frame.pos, &right_prev_frame.pos, delta);
    zgicabra.right.scalar_vel = hyp(&zgicabra.right.vel);


    Ok(())
}

fn copy_frame_to_wand (frame: &ControllerFrame, wand: &mut Wand) {
    wand.pos = frame.pos.clone();
    wand.rot = frame.rot_quat.clone();
    wand.trigger = frame.trigger;
}

fn derivative_r3 (a: &[f32;3], b: &[f32;3], delta: f32) -> [f32;3] {
    [ (a[0] - b[0]) / delta, (a[1] - b[1]) / delta, (a[2] - b[2]) / delta ]
}

fn hyp (v: &[f32;3]) -> f32 {
    let dx = v[0] * v[0];
    let dy = v[1] * v[1];
    let dz = v[2] * v[2];
    (dx + dy + dz).sqrt()
}


