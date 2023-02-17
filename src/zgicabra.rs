
use std::time::Duration;
use std::io::{Error};

use crate::hydra::HydraState;
use crate::sixense::ControllerFrame;


//
// Data Types
//

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
pub struct ControlSignal {
    value: f32,
    channel: u8,
}


//
// Main Datatype
//

#[derive(Debug, Clone, Copy)]
pub struct Zgicabra {
    pub left:  Wand,
    pub right: Wand,
    pub separation: f32,
    pub root_note: u8,
    pub pitchbend: i16,
    pub filter: ControlSignal,
}

impl Zgicabra {
    pub fn new() -> Zgicabra {
        Zgicabra {
            left:  Wand::new(),
            right: Wand::new(),
            separation: 0.0,
            root_note: 65,
            pitchbend: 0,
            filter: ControlSignal { value: 0.0, channel: 25 },
        }
    }
}


//
// Module Functions
//

pub fn update (latest_state: &mut Zgicabra, prev_state: &Zgicabra, hydra_state: &HydraState) -> Result<(), Error> {

    // Recent hydra history

    let left_frame  = hydra_state.controllers[0];
    let right_frame = hydra_state.controllers[1];


    // Map immediately updated values (avg position with previous frame)

    copy_frame_to_wand(&left_frame,  &mut latest_state.left,  &prev_state.left);
    copy_frame_to_wand(&right_frame, &mut latest_state.right, &prev_state.right);

    latest_state.separation = latest_state.left.pos[0] - latest_state.right.pos[0];


    // Time derivatives

    let delta:f32 = hydra_state.timedelta.as_millis() as f32;

    latest_state.left.vel         = derivative_r3(&left_frame.pos,  &prev_state.left.pos,  delta);
    latest_state.right.vel        = derivative_r3(&right_frame.pos, &prev_state.right.pos, delta);
    latest_state.left.scalar_vel  = (hyp(&latest_state.left.vel)  + &prev_state.left.scalar_vel) /2.0;
    latest_state.right.scalar_vel = (hyp(&latest_state.right.vel) + &prev_state.right.scalar_vel)/2.0;


    Ok(())
}

fn copy_frame_to_wand (frame: &ControllerFrame, wand: &mut Wand, prev_wand: &Wand) {
    wand.pos = frame.pos.clone();
    wand.rot = frame.rot_quat.clone();
    wand.trigger = frame.trigger;

    wand.pos[0] = (wand.pos[0] + prev_wand.pos[0])/2.0;
    wand.pos[1] = (wand.pos[1] + prev_wand.pos[1])/2.0;
    wand.pos[2] = (wand.pos[2] + prev_wand.pos[2])/2.0;

    // TODO: The rest of this
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

