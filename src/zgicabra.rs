
use std::fmt;
use std::time::Duration;
use std::io::{Error};
use core::f32::consts::PI;

use crate::hydra;
use crate::hydra::{HydraState,ControllerFrame};
use crate::tools::*;

const JOYSTICK_DEADZONE: f32 = 0.15;


//
// Data Types
//

#[derive(Debug, Clone, Copy)]
pub enum Voice {
    Classic    = 0,
    Eternal    = 1,
    Pennysack  = 2,
    Submission = 3
}

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
pub struct NoteState {
    pub on: bool,
    pub root: u8,
    pub bend: f32,
    pub current: u8,
}

impl NoteState {
    pub fn new() -> NoteState {
        NoteState {
            on: false,
            root: 42,
            bend: 0.0,
            current: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SignalState {
    pub filter:       f32,
    pub fuzz:         f32,
    pub width:        f32,
    pub thump:        f32,
    pub velocity:     f32,
    pub acceleration: f32,
    pub jerk:         f32,
}

impl SignalState {
    pub fn new() -> SignalState {
        SignalState {
            filter: 0.0,
            fuzz:   0.0,
            width:  0.0,
            thump:  0.0,
            velocity:     0.0,
            acceleration: 0.0,
            jerk:         0.0,
        }
    }
}


//
// Delta Events
//

type Note  = u8;

#[derive(Debug, Clone)]
pub enum DeltaEvent {
    NoteStart(Note),
    NoteChange(Note, Note),
    NoteEnd(Note),
    FilterLevel(f32),
    FuzzLevel(f32),
    WidthLevel(f32),
    PitchBend(f32),
    VoiceChange(Voice),
    TuneUp(),
    TuneDown(),
    NextVoice(),
    PrevVoice(),
    ThumpToggle(),
    FuzzToggle(),
    Panic()
}


//
// Main Datatype
//

#[derive(Debug, Clone)]
pub struct Zgicabra {
    pub left:  Wand,
    pub right: Wand,
    pub separation: f32,
    pub docked: bool,
    pub level: f32,
    pub sequence_number: u8,
    pub note: NoteState,
    pub signal: SignalState,
    pub voice: Voice,
}

impl Zgicabra {
    pub fn new() -> Zgicabra {
        Zgicabra {
            left:  Wand::new(),
            right: Wand::new(),
            separation: 0.0,
            docked: false,
            level: 0.0,
            sequence_number: 0,
            note: NoteState::new(),
            signal: SignalState::new(),
            voice: Voice::Classic,
        }
    }
}


//
// Module Functions
//

pub fn update (curr_state: &mut Zgicabra, prev_state: &Zgicabra, hydra_state: &HydraState, deltas: &mut Vec<DeltaEvent>) {

    // Sequence number happens always

    curr_state.sequence_number = hydra_state.controllers[0].sequence_number;
    curr_state.docked = hydra_state.controllers[0].is_docked != 0
                     || hydra_state.controllers[1].is_docked != 0;


    // Map immediately updated values (and avg position with previous frame)

    copy_frame_to_wand(&hydra_state.controllers[0], &mut curr_state.left,  &prev_state.left);
    copy_frame_to_wand(&hydra_state.controllers[1], &mut curr_state.right, &prev_state.right);


    // Time derivatives

    let dt:f32 = hydra_state.timedelta.as_millis() as f32;

    curr_state.left.vel   = derivative_r3(&curr_state.left.pos,  &prev_state.left.pos,  dt);
    curr_state.right.vel  = derivative_r3(&curr_state.right.pos, &prev_state.right.pos, dt);
    curr_state.left.acc   = derivative_r3(&curr_state.left.vel,  &prev_state.left.vel,  dt);
    curr_state.right.acc  = derivative_r3(&curr_state.right.vel, &prev_state.right.vel, dt);
    curr_state.left.jerk  = derivative_r3(&curr_state.left.acc,  &prev_state.left.acc,  dt);
    curr_state.right.jerk = derivative_r3(&curr_state.right.acc, &prev_state.right.acc, dt);

    curr_state.left.scalar_vel   = (hyp(&curr_state.left.vel)   + &prev_state.left.scalar_vel)   / 2.0;
    curr_state.right.scalar_vel  = (hyp(&curr_state.right.vel)  + &prev_state.right.scalar_vel)  / 2.0;
    curr_state.left.scalar_acc   = (hyp(&curr_state.left.acc)   + &prev_state.left.scalar_acc)   / 2.0;
    curr_state.right.scalar_acc  = (hyp(&curr_state.right.acc)  + &prev_state.right.scalar_acc)  / 2.0;
    curr_state.left.scalar_jerk  = (hyp(&curr_state.left.jerk)  + &prev_state.left.scalar_jerk)  / 2.0;
    curr_state.right.scalar_jerk = (hyp(&curr_state.right.jerk) + &prev_state.right.scalar_jerk) / 2.0;


    // Two-handed values

    curr_state.separation = (curr_state.left.pos[0] - curr_state.right.pos[0]).abs();
    curr_state.note.bend  = curr_state.left.twist - curr_state.right.twist;
    curr_state.note.bend  = curr_state.note.bend.powf(3.0).clamp(-2.0, 2.0) * 0.5;

    let trigger_total = curr_state.left.trigger + curr_state.right.trigger;
    curr_state.level  = smoothstep(0.0, 1.0, trigger_total.clamp(0.0, 1.0));


    // Triggers and notes

    let left_trigger_start  = curr_state.left.trigger  > prev_state.left.trigger  && prev_state.left.trigger  == 0.0;
    let left_trigger_end    = prev_state.left.trigger  > curr_state.left.trigger  && curr_state.left.trigger  == 0.0;
    let right_trigger_start = curr_state.right.trigger > prev_state.right.trigger && prev_state.right.trigger == 0.0;
    let right_trigger_end   = prev_state.right.trigger > curr_state.right.trigger && curr_state.right.trigger == 0.0;

    // If note is note currently on and either trigger begins to be pressed
    if (left_trigger_start || right_trigger_start) && !curr_state.note.on {
        deltas.push(DeltaEvent::NoteStart(curr_state.note.current));
        curr_state.note.on = true;
    }

    // If note is currently on and left trigger is released and right trigger is not pressed at all
    if curr_state.note.on && (left_trigger_end && curr_state.right.trigger == 0.0) {
        deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
        curr_state.note.on = false;
    }

    // If note is currently on and right trigger is released and left trigger is not pressed at all
    if curr_state.note.on && (right_trigger_end && curr_state.left.trigger == 0.0) {
        deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
        curr_state.note.on = false;
    }

    // Update note to stick position
    let new_note = (curr_state.note.root as i8
        + stick_to_note_offset(&curr_state.left)
        + stick_to_note_modifier(&curr_state.right)) as u8;

    if curr_state.note.current != new_note && curr_state.note.on {
        deltas.push(DeltaEvent::NoteChange(curr_state.note.current, new_note));
        curr_state.note.current = new_note;
    }


    // Double-stick-click for panic

    if curr_state.left.stick.clicked && curr_state.right.stick.clicked {
        deltas.push(DeltaEvent::Panic());
    }


    // Button Events

    fn each_wand (prev: Wand, curr: Wand, deltas: &mut Vec<DeltaEvent>) {
        if curr.bumper && !prev.bumper {
            //deltas.push(DeltaEvent::BumperDown(curr.hand));
        }
        if !curr.bumper && prev.bumper {
            //deltas.push(DeltaEvent::BumperUp(curr.hand));
        }
        if curr.home && !prev.home {
            //deltas.push(DeltaEvent::HomeDown(curr.hand));
        }
        if !curr.home && prev.home {
            //deltas.push(DeltaEvent::HomeUp(curr.hand));
        }
    }

    each_wand(prev_state.left,  curr_state.left,  deltas);
    each_wand(prev_state.right, curr_state.right, deltas);


    // Buttons

    /*                    ╭─────[ - Tune + ]─────╮
              ┏━━━┓     ┏━┷━┓                  ┏━┷━┓     ┏━━━┓
            ╭─┨ 4 ┃     ┃ 1 ┃                  ┃ 1 ┃     ┃ 4 ┠─╮
            │ ┗━━━┛     ┗━━━┛                  ┗━━━┛     ┗━━━┛ │
    THUMP ]─┤                                                  ├─[ FUZZ
            │   ┏━━━┓ ┏━━━┓                      ┏━━━┓ ┏━━━┓   │
            ╰───┨ 3 ┃ ┃ 2 ┃                      ┃ 2 ┃ ┃ 3 ┠───╯
                ┗━━━┛ ┗━┯━┛                      ┗━┯━┛ ┗━━━┛
                        ╰──────[ - Voices + ]──────╯                        */ 

    // Two-handed buttons
    for i in 0..4 {
        if curr_state.left.buttons[i] && curr_state.right.buttons[i] &&
            (!prev_state.left.buttons[i] || !prev_state.right.buttons[i]) {

            // Rocking left-or-right
            // Delta is negative if left hand button was most recently unpressed, positive otherwise
            let rock_direction:i8 = if !prev_state.left.buttons[i] { -1 } else { 1 };

            match i {
                0 => curr_state.note.root = curr_state.note.root + 1,
                1 => curr_state.note.root = ((curr_state.note.root as i8) + rock_direction) as u8,
                _ => {},
            }
        }
    }

    // Thumbsmashes
    for hand in [Hand::Left, Hand::Right].iter() {
        let curr = if *hand == Hand::Left { &curr_state.left } else { &curr_state.right };
        let prev = if *hand == Hand::Left { &prev_state.left } else { &prev_state.right };

        if curr.buttons[3] && curr.buttons[4] && (!prev.buttons[3] || !prev.buttons[4]) {
            match hand {
                Hand::Left  => deltas.push(DeltaEvent::ThumpToggle()),
                Hand::Right => deltas.push(DeltaEvent::FuzzToggle()),
                Hand::Neither => {},
            }
        }
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

    wand.bumper = button_mask(frame.buttons, hydra::BUTTON_BUMPER);
    wand.home   = button_mask(frame.buttons, hydra::BUTTON_HOME);

    wand.hand = match frame.which_hand {
        hydra::LEFT_HAND  => Hand::Left,
        hydra::RIGHT_HAND => Hand::Right,
        _                   => Hand::Neither,
    };

    // Reverse button mapping for left hand
    match wand.hand {
        Hand::Neither => {},
        Hand::Left => {
            wand.buttons[0] = button_mask(frame.buttons, hydra::BUTTON_4);
            wand.buttons[1] = button_mask(frame.buttons, hydra::BUTTON_2);
            wand.buttons[2] = button_mask(frame.buttons, hydra::BUTTON_1);
            wand.buttons[3] = button_mask(frame.buttons, hydra::BUTTON_3);
        },
        Hand::Right => {
            wand.buttons[0] = button_mask(frame.buttons, hydra::BUTTON_3);
            wand.buttons[1] = button_mask(frame.buttons, hydra::BUTTON_1);
            wand.buttons[2] = button_mask(frame.buttons, hydra::BUTTON_2);
            wand.buttons[3] = button_mask(frame.buttons, hydra::BUTTON_4);
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

fn stick_to_note_offset(&wand: &Wand) -> i8 {
    match wand.stick.octant {
        Direction::Left      => -4,
        Direction::UpLeft    => -2,
        Direction::Up        =>  2,
        Direction::UpRight   =>  3,
        Direction::Right     =>  5,
        Direction::DownRight =>  7,
        Direction::Down      =>  8,
        Direction::DownLeft  => 10,
        _ => 0,
    }
}

fn stick_to_note_modifier(&wand: &Wand) -> i8 {
    match wand.stick.octant {
        Direction::Left      =>   1,
        Direction::Right     =>  -1,
        Direction::Up        =>  12,
        Direction::Down      => -12,
        Direction::UpLeft    =>  13,
        Direction::UpRight   =>  11,
        Direction::DownLeft  => -11,
        Direction::DownRight => -13,
        _ => 0,
    }
}


//
// Other Helpers
//

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

