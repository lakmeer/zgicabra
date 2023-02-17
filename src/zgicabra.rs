
use std::time::Duration;
use std::io::{Error};
use core::f32::consts::PI;

use crate::sixense;
use crate::sixense::ControllerFrame;
use crate::hydra::HydraState;


const JOYSTICK_DEADZONE: f32 = 0.15;


//
// Data Types
//

#[derive(Debug, Clone, Copy)]
pub enum Hand {
    Unknown,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Left,
    UpLeft,
    None,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
}

#[derive(Debug, Clone, Copy)]
pub struct Joystick {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub theta: f32,
    pub quadrant: Direction,
    pub octant: Direction,
    pub clicked: bool,
}

impl Joystick {
    pub fn new() -> Joystick {
        Joystick {
            x: 0.0,
            y: 0.0,
            r: 0.0,
            theta: 0.0,
            quadrant: Direction::None,
            octant: Direction::None,
            clicked: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Wand {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub vel: [f32; 3],
    pub acc: [f32; 3],
    pub jerk: [f32; 3],
    pub scalar_vel: f32,
    pub scalar_acc: f32,
    pub scalar_jerk: f32,
    pub trigger: f32,
    pub bumper: bool,
    pub home: bool,
    pub buttons: [bool; 4],
    pub stick: Joystick,
    pub hand: Hand,
}

impl Wand {
    pub fn new() -> Wand {
        Wand {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0, 0.0],
            vel: [0.0, 0.0, 0.0],
            acc: [0.0, 0.0, 0.0],
            jerk: [0.0, 0.0, 0.0],
            scalar_vel: 0.0,
            scalar_acc: 0.0,
            scalar_jerk: 0.0,
            trigger: 0.0,
            bumper: false,
            home: false,
            buttons: [false, false, false, false],
            stick: Joystick::new(),
            hand: Hand::Unknown,
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

    latest_state.left.acc         = derivative_r3(&latest_state.left.vel,  &prev_state.left.vel,  delta);
    latest_state.right.acc        = derivative_r3(&latest_state.right.vel, &prev_state.right.vel, delta);
    latest_state.left.scalar_acc  = (hyp(&latest_state.left.acc)  + &prev_state.left.scalar_acc) /2.0;
    latest_state.right.scalar_acc = (hyp(&latest_state.right.acc) + &prev_state.right.scalar_acc)/2.0;

    latest_state.left.jerk         = derivative_r3(&latest_state.left.acc,  &prev_state.left.acc,  delta);
    latest_state.right.jerk        = derivative_r3(&latest_state.right.acc, &prev_state.right.acc, delta);
    latest_state.left.scalar_jerk  = (hyp(&latest_state.left.jerk)  + &prev_state.left.scalar_jerk) /2.0;
    latest_state.right.scalar_jerk = (hyp(&latest_state.right.jerk) + &prev_state.right.scalar_jerk)/2.0;


    Ok(())
}


//
// Helpers
//

fn copy_frame_to_wand (frame: &ControllerFrame, wand: &mut Wand, prev_wand: &Wand) {
    wand.pos = frame.pos.clone();
    wand.rot = frame.rot_quat.clone();

    wand.trigger = frame.trigger;

    wand.pos[0] = (wand.pos[0] + prev_wand.pos[0])/2.0;
    wand.pos[1] = (wand.pos[1] + prev_wand.pos[1])/2.0;
    wand.pos[2] = (wand.pos[2] + prev_wand.pos[2])/2.0;

    wand.bumper = button_mask(frame.buttons, sixense::BUTTON_BUMPER);
    wand.home   = button_mask(frame.buttons, sixense::BUTTON_HOME);

    // Tag this wand which hand it is
    wand.hand = match frame.which_hand {
        sixense::LEFT_HAND  => Hand::Left,
        sixense::RIGHT_HAND => Hand::Right,
        _                   => Hand::Unknown,
    };

    // Reverse button mapping for left hand
    match wand.hand {
        Hand::Unknown => {},
        Hand::Left => {
            wand.buttons[0] = button_mask(frame.buttons, sixense::BUTTON_2);
            wand.buttons[1] = button_mask(frame.buttons, sixense::BUTTON_1);
            wand.buttons[2] = button_mask(frame.buttons, sixense::BUTTON_4);
            wand.buttons[3] = button_mask(frame.buttons, sixense::BUTTON_3);
        },
        Hand::Right => {
            wand.buttons[0] = button_mask(frame.buttons, sixense::BUTTON_1);
            wand.buttons[1] = button_mask(frame.buttons, sixense::BUTTON_2);
            wand.buttons[2] = button_mask(frame.buttons, sixense::BUTTON_3);
            wand.buttons[3] = button_mask(frame.buttons, sixense::BUTTON_4);
        },
    }


    copy_joystick_to_wand(frame, wand);
}

fn copy_joystick_to_wand (frame: &ControllerFrame, wand: &mut Wand) {
    wand.stick.x        = frame.joystick_x;
    wand.stick.y        = frame.joystick_y;
    wand.stick.r        = (wand.stick.x * wand.stick.x + wand.stick.y * wand.stick.y).sqrt();
    wand.stick.theta    = rad_to_cycles(wand.stick.y.atan2(wand.stick.x));
    wand.stick.quadrant = joystick_quadrant(&wand.stick);
    wand.stick.octant   = joystick_octant(&wand.stick);
    wand.stick.clicked  = (frame.buttons & 0b100000000) != 0;
}

fn joystick_quadrant (stick: &Joystick) -> Direction {
    if stick.r < JOYSTICK_DEADZONE { return Direction::None; }

    match stick.theta * 8.0 {
        t if t > 0.0 && t <= 1.0 => Direction::Up,
        t if t > 1.0 && t <= 3.0 => Direction::Right,
        t if t > 3.0 && t <= 5.0 => Direction::Down,
        t if t > 5.0 && t <= 7.0 => Direction::Left,
        t if t > 7.0 && t <= 8.0 => Direction::Up,
        _ => Direction::None,
    }
}

fn joystick_octant (stick: &Joystick) -> Direction {
    if stick.r < JOYSTICK_DEADZONE { return Direction::None; }

    match stick.theta * 8.0 {
        t if t > 0.0 && t <= 0.5 => Direction::Up,
        t if t > 0.5 && t <= 1.5 => Direction::UpRight,
        t if t > 1.5 && t <= 2.5 => Direction::Right,
        t if t > 2.5 && t <= 3.5 => Direction::DownRight,
        t if t > 3.5 && t <= 4.5 => Direction::Down,
        t if t > 4.5 && t <= 5.5 => Direction::DownLeft,
        t if t > 5.5 && t <= 6.5 => Direction::Left,
        t if t > 6.5 && t <= 7.5 => Direction::UpLeft,
        t if t > 7.5 && t <= 8.0 => Direction::Up,
        _ => Direction::None,
    }
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

fn rad_to_cycles (radians: f32) -> f32 {
    1.0 - (radians + PI/2.0 + PI) / (2.0 * PI) % 1.0
}

fn button_mask (buttons: u32, mask: u32) -> bool {
    (buttons & mask) != 0
}

