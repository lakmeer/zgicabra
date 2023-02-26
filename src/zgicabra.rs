
use std::fmt;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hand {
    Neither,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    None,
    Left,
    UpLeft,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
}

impl PartialEq for Direction {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Direction::None,      Direction::None)      => true,
            (Direction::Left,      Direction::Left)      => true,
            (Direction::UpLeft,    Direction::UpLeft)    => true,
            (Direction::Up,        Direction::Up)        => true,
            (Direction::UpRight,   Direction::UpRight)   => true,
            (Direction::Right,     Direction::Right)     => true,
            (Direction::DownRight, Direction::DownRight) => true,
            (Direction::Down,      Direction::Down)      => true,
            (Direction::DownLeft,  Direction::DownLeft)  => true,
            _ => false,
        }
    }
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
    pub pitch: f32,
    pub twist: f32,
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
            pitch: 0.0,
            twist: 0.0,
            scalar_vel: 0.0,
            scalar_acc: 0.0,
            scalar_jerk: 0.0,
            trigger: 0.0,
            bumper: false,
            home: false,
            buttons: [false, false, false, false],
            stick: Joystick::new(),
            hand: Hand::Neither,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ControlSignal {
    value: f32,
    channel: u8,
}


//
// Delta Events
//

#[derive(Debug, Clone)]
pub enum DeltaEvent {
    TriggerStart(Hand),
    TriggerBottomOut(Hand),
    TriggerRelease(Hand),
    TriggerEnd(Hand),
    BumperDown(Hand),
    BumperUp(Hand),
    HomeDown(Hand),
    HomeUp(Hand),
    ButtonDown(Hand, u8),
    ButtonUp(Hand, u8),
    StickClickDown(Hand),
    StickClickUp(Hand),
    StickMove(Hand, Direction),
}


//
// Main Datatype
//

#[derive(Debug, Clone)]
pub struct Zgicabra {
    pub left:  Wand,
    pub right: Wand,
    pub separation: f32,
    pub root_note: u8,
    pub bend: f32,
    pub filter: ControlSignal,
    pub docked: bool,
    pub level: f32,
    pub deltas: Vec<DeltaEvent>,
    pub sequence_number: u8,
}

impl Zgicabra {
    pub fn new() -> Zgicabra {
        Zgicabra {
            left:  Wand::new(),
            right: Wand::new(),
            separation: 0.0,
            root_note: 65,
            bend: 0.0,
            filter: ControlSignal { value: 0.0, channel: 25 },
            docked: false,
            level: 0.0,
            deltas: vec![],
            sequence_number: 0,
        }
    }
}


//
// Module Functions
//

pub fn update (latest_state: &mut Zgicabra, prev_state: &Zgicabra, hydra_state: &HydraState) {

    latest_state.sequence_number = hydra_state.controllers[0].sequence_number;


    // Recent hydra history

    let left_frame  = hydra_state.controllers[0];
    let right_frame = hydra_state.controllers[1];


    // Map immediately updated values (avg position with previous frame)

    copy_frame_to_wand(&left_frame,  &mut latest_state.left,  &prev_state.left);
    copy_frame_to_wand(&right_frame, &mut latest_state.right, &prev_state.right);

    latest_state.separation = (latest_state.left.pos[0] - latest_state.right.pos[0]).abs();

    latest_state.bend = latest_state.left.twist - latest_state.right.twist;
    latest_state.bend = latest_state.bend.powf(3.0).clamp(-2.0, 2.0) * 0.5;

    let deltas = &mut latest_state.deltas;

    capture_stick_events(prev_state.left,  latest_state.left,  deltas);
    capture_stick_events(prev_state.right, latest_state.right, deltas);
    capture_button_events(prev_state.left,  latest_state.left,  deltas);
    capture_button_events(prev_state.right, latest_state.right, deltas);
    capture_trigger_events(prev_state.left,  latest_state.left,  deltas);
    capture_trigger_events(prev_state.right, latest_state.right, deltas);


    // Time derivatives

    let delta:f32 = hydra_state.timedelta.as_millis() as f32;

    latest_state.left.vel   = derivative_r3(&left_frame.pos,         &prev_state.left.pos,  delta);
    latest_state.right.vel  = derivative_r3(&right_frame.pos,        &prev_state.right.pos, delta);
    latest_state.left.acc   = derivative_r3(&latest_state.left.vel,  &prev_state.left.vel,  delta);
    latest_state.right.acc  = derivative_r3(&latest_state.right.vel, &prev_state.right.vel, delta);
    latest_state.left.jerk  = derivative_r3(&latest_state.left.acc,  &prev_state.left.acc,  delta);
    latest_state.right.jerk = derivative_r3(&latest_state.right.acc, &prev_state.right.acc, delta);

    latest_state.left.scalar_vel   = (hyp(&latest_state.left.vel)   + &prev_state.left.scalar_vel)   / 2.0;
    latest_state.right.scalar_vel  = (hyp(&latest_state.right.vel)  + &prev_state.right.scalar_vel)  / 2.0;
    latest_state.left.scalar_acc   = (hyp(&latest_state.left.acc)   + &prev_state.left.scalar_acc)   / 2.0;
    latest_state.right.scalar_acc  = (hyp(&latest_state.right.acc)  + &prev_state.right.scalar_acc)  / 2.0;
    latest_state.left.scalar_jerk  = (hyp(&latest_state.left.jerk)  + &prev_state.left.scalar_jerk)  / 2.0;
    latest_state.right.scalar_jerk = (hyp(&latest_state.right.jerk) + &prev_state.right.scalar_jerk) / 2.0;


    // Singleton states

    latest_state.docked = left_frame.is_docked != 0 || right_frame.is_docked != 0;
    latest_state.level  = smoothstep(0.0, 1.0, (latest_state.left.trigger + latest_state.right.trigger).clamp(0.0, 1.0));


    // Control signals

    // TODO

}

pub fn clear (state: &mut Zgicabra, limit: usize) {
    let len = state.deltas.len();

    if len > limit {
        state.deltas = state.deltas.split_off(len - limit);
    }
}



//
// Helpers
//

fn copy_frame_to_wand (frame: &ControllerFrame, wand: &mut Wand, prev_wand: &Wand) {
    wand.pos = frame.pos.clone();
    wand.rot = frame.rot_quat.clone();

    wand.pitch   = frame.rot_quat[1];
    wand.twist   = frame.rot_quat[2] * 2.0;
    wand.trigger = frame.trigger;

    wand.pos[0] = (wand.pos[0] + prev_wand.pos[0])/2.0;
    wand.pos[1] = (wand.pos[1] + prev_wand.pos[1])/2.0;
    wand.pos[2] = (wand.pos[2] + prev_wand.pos[2])/2.0;

    wand.bumper = button_mask(frame.buttons, sixense::BUTTON_BUMPER);
    wand.home   = button_mask(frame.buttons, sixense::BUTTON_HOME);

    wand.hand = match frame.which_hand {
        sixense::LEFT_HAND  => Hand::Left,
        sixense::RIGHT_HAND => Hand::Right,
        _                   => Hand::Neither,
    };

    // Reverse button mapping for left hand
    match wand.hand {
        Hand::Neither => {},
        Hand::Left => {
            wand.buttons[0] = button_mask(frame.buttons, sixense::BUTTON_4);
            wand.buttons[1] = button_mask(frame.buttons, sixense::BUTTON_2);
            wand.buttons[2] = button_mask(frame.buttons, sixense::BUTTON_1);
            wand.buttons[3] = button_mask(frame.buttons, sixense::BUTTON_3);
        },
        Hand::Right => {
            wand.buttons[0] = button_mask(frame.buttons, sixense::BUTTON_3);
            wand.buttons[1] = button_mask(frame.buttons, sixense::BUTTON_1);
            wand.buttons[2] = button_mask(frame.buttons, sixense::BUTTON_2);
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

fn capture_trigger_events (prev: Wand, curr: Wand, deltas: &mut Vec<DeltaEvent>) {
    if curr.trigger > prev.trigger {
        if prev.trigger == 0.0 { deltas.push(DeltaEvent::TriggerStart(curr.hand)); }
        if curr.trigger == 1.0 { deltas.push(DeltaEvent::TriggerBottomOut(curr.hand)); }
    }

    if curr.trigger < prev.trigger {
        if prev.trigger == 1.0 { deltas.push(DeltaEvent::TriggerRelease(curr.hand)); }
        if curr.trigger == 0.0 { deltas.push(DeltaEvent::TriggerEnd(curr.hand)); }
    }
}

fn capture_button_events (prev: Wand, curr: Wand, deltas: &mut Vec<DeltaEvent>) {
   if curr.bumper && !prev.bumper {
        deltas.push(DeltaEvent::BumperDown(curr.hand));
    }
    if !curr.bumper && prev.bumper {
        deltas.push(DeltaEvent::BumperUp(curr.hand));
    }
    if curr.home && !prev.home {
        deltas.push(DeltaEvent::HomeDown(curr.hand));
    }
    if !curr.home && prev.home {
        deltas.push(DeltaEvent::HomeUp(curr.hand));
    }
    for i in 0..4 {
        if curr.buttons[i] && !prev.buttons[i] {
            deltas.push(DeltaEvent::ButtonDown(curr.hand, i as u8));
        }
        if !curr.buttons[i] && prev.buttons[i] {
            deltas.push(DeltaEvent::ButtonUp(curr.hand, i as u8));
        }
    }
}

fn capture_stick_events (prev: Wand, curr: Wand, deltas: &mut Vec<DeltaEvent>) {
    if curr.stick.octant != prev.stick.octant {
        deltas.push(DeltaEvent::StickMove(curr.hand, curr.stick.octant));
    }
    if curr.stick.clicked && !prev.stick.clicked {
        deltas.push(DeltaEvent::StickClickDown(curr.hand));
    }
    if !curr.stick.clicked && prev.stick.clicked {
        deltas.push(DeltaEvent::StickClickUp(curr.hand));
    }
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

fn smoothstep (a: f32, b: f32, t: f32) -> f32 {
    let t = (t - a) / (b - a);
    t * t * (3.0 - 2.0 * t)
}
