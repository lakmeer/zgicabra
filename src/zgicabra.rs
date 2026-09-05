
// Turns raw Hydra state into the more-complex Zgicabra state.

use std::fmt;
use std::time::Duration;
use std::io::{Error};
use core::f32::consts::PI;

use crate::hydra;
use crate::hydra::{HydraState,ControllerFrame};
use crate::tools::*;

const JOYSTICK_DEADZONE: f32 = 0.15;
const TRIGGER_MODE: TriggerMode = TriggerMode::Full;
const REPEAT_MODE: RepeatMode = RepeatMode { on_stick: false, on_swap: false, on_trigger: true };


//
// Data Types
//

// Trigger behaviour
enum TriggerMode {
    Full,     // Triggers start and end a note only when it reaches zero
    Instant,  // Triggers start and end a note as soon as it changes direction
}

// Note Repeat behaviour
struct RepeatMode {
    on_stick: bool,
    on_swap: bool,
    on_trigger: bool,
}


// Which hand is engaged
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hand {
    Neither,
    Left,
    Right,
}

// Octant of the joystick
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

// Joystick state
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

// Whole wand state
#[derive(Debug, Clone, Copy)]
pub struct Wand {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub vel: [f32; 3],
    pub acc: [f32; 3],
    pub pitch: f32,
    pub twist: f32,
    pub scalar_vel: f32,
    pub scalar_acc: f32,
    pub trigger: f32,
    pub trigger_delta: f32,
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
            pitch: 0.0,
            twist: 0.0,
            scalar_vel: 0.0,
            scalar_acc: 0.0,
            trigger: 0.0,
            trigger_delta: 0.0,
            bumper: false,
            home: false,
            buttons: [false, false, false, false],
            stick: Joystick::new(),
            hand: Hand::Neither,
        }
    }
}

// Note currently being produced
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
            root: 35,
            bend: 0.0,
            current: 0,
        }
    }
}


// State that is piped constantly to the sound engine
#[derive(Debug, Clone, Copy)]
pub struct SignalState {
    pub level:        f32,
    pub bend:         f32,
    pub filter:       f32,
    pub width:        f32,
    pub depth:        f32,
    pub alpha:        f32,
    pub omega:        f32,
    pub vel:          f32,
    pub acc:          f32,
}

impl SignalState {
    pub fn new() -> SignalState {
        SignalState {
            level:        0.0,
            bend:         0.0,
            filter:       0.0,
            width:        0.0,
            depth:        0.0,
            alpha:        0.0,
            omega:        0.0,
            vel:          0.0,
            acc:          0.0,
        }
    }
}


//
// Delta Events
//

type Note = u8;

pub fn note_name(note: Note) -> String {
    const NAMES: [&str; 12] = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
    let octave = (note / 12) as i8 - 1;
    let name = NAMES[(note % 12) as usize];
    format!("{}{}", name, octave)
}

#[derive(Debug, Clone)]
pub enum DeltaEvent {
    NoteStart(Note),
    NoteChange(Note, Note),
    NoteEnd(Note),
    WidthLevel(f32),
    VoiceChange(i8), // -1 or 1
    RootChange(Note),
    Panic(),
    BumperDown(Hand),
    BumperUp(Hand),
    HomeDown(Hand),
    HomeUp(Hand),
    Debug(&'static str),
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
    pub seq_num: u8,
    pub trigger_total: f32,
    pub most_recent_wand: Hand,
    pub note: NoteState,
    pub alpha_lock: bool,
    pub omega_lock: bool,
    pub signal: SignalState,
}

impl Zgicabra {
    pub fn new() -> Zgicabra {
        Zgicabra {
            left:  Wand::new(),
            right: Wand::new(),
            separation: 0.0,
            docked: false,
            seq_num: 0,
            trigger_total: 0.0,
            most_recent_wand: Hand::Neither,
            note: NoteState::new(),
            alpha_lock: false,
            omega_lock: false,
            signal: SignalState::new(),
        }
    }
}


//
// Module Functions
//

pub fn update (curr_state: &mut Zgicabra, prev_state: &Zgicabra, hydra_state: &HydraState, deltas: &mut Vec<DeltaEvent>) {

    curr_state.seq_num = hydra_state.controllers[0].sequence_number;
    curr_state.docked = hydra_state.controllers[0].is_docked != 0
                     || hydra_state.controllers[1].is_docked != 0;

    copy_frame_to_wand(&hydra_state.controllers[0], &mut curr_state.left,  &prev_state.left);
    copy_frame_to_wand(&hydra_state.controllers[1], &mut curr_state.right, &prev_state.right);

    // Compute derivatives

    let dt:f32 = hydra_state.timedelta.as_millis() as f32;

    curr_state.left.vel   = derivative_r3(&curr_state.left.pos,  &prev_state.left.pos,  dt);
    curr_state.right.vel  = derivative_r3(&curr_state.right.pos, &prev_state.right.pos, dt);
    curr_state.left.acc   = derivative_r3(&curr_state.left.vel,  &prev_state.left.vel,  dt);
    curr_state.right.acc  = derivative_r3(&curr_state.right.vel, &prev_state.right.vel, dt);

    curr_state.left.scalar_vel   = (hyp(&curr_state.left.vel)   + &prev_state.left.scalar_vel)   / 2.0;
    curr_state.right.scalar_vel  = (hyp(&curr_state.right.vel)  + &prev_state.right.scalar_vel)  / 2.0;
    curr_state.left.scalar_acc   = (hyp(&curr_state.left.acc)   + &prev_state.left.scalar_acc)   / 2.0;
    curr_state.right.scalar_acc  = (hyp(&curr_state.right.acc)  + &prev_state.right.scalar_acc)  / 2.0;

    // Separation

    curr_state.separation = (curr_state.left.pos[0] - curr_state.right.pos[0]).abs();

    // Bend

    curr_state.note.bend  = curr_state.left.twist/2.5 - curr_state.right.twist/2.5;
    curr_state.note.bend  = (curr_state.note.bend.powf(3.0) * 0.5).clamp(-1.0, 1.0);

    // Trigger state

    // only change the delta if trigger value is not static
    curr_state.left.trigger_delta  = 
        if curr_state.left.trigger == prev_state.left.trigger {
          prev_state.left.trigger_delta
        } else {
          (curr_state.left.trigger - prev_state.left.trigger).sign()
        };

    curr_state.right.trigger_delta =
        if curr_state.right.trigger == prev_state.right.trigger {
          prev_state.right.trigger_delta
        } else {
          (curr_state.right.trigger - prev_state.right.trigger).sign()
        };

    let (left_trigger_start, left_trigger_end) = match TRIGGER_MODE {
        TriggerMode::Full => (
            curr_state.left.trigger > prev_state.left.trigger && prev_state.left.trigger == 0.0,
            curr_state.left.trigger < prev_state.left.trigger && curr_state.left.trigger == 0.0
        ),
        TriggerMode::Instant => (
            curr_state.left.trigger_delta > 0.0 && curr_state.left.trigger_delta != prev_state.left.trigger_delta,
            curr_state.left.trigger_delta < 0.0 && curr_state.left.trigger_delta != prev_state.left.trigger_delta
        ),
    };

    let (right_trigger_start, right_trigger_end) = match TRIGGER_MODE {
        TriggerMode::Full => (
            curr_state.right.trigger > prev_state.right.trigger && prev_state.right.trigger == 0.0,
            curr_state.right.trigger < prev_state.right.trigger && curr_state.right.trigger == 0.0
        ),
        TriggerMode::Instant => (
            curr_state.right.trigger > prev_state.right.trigger,
            curr_state.right.trigger < prev_state.right.trigger
        ),
    };

    if left_trigger_start { curr_state.most_recent_wand = Hand::Left; }
    if right_trigger_start { curr_state.most_recent_wand = Hand::Right; }
    if left_trigger_end && curr_state.right.trigger > 0.0 { curr_state.most_recent_wand = Hand::Right; }
    if right_trigger_end && curr_state.left.trigger > 0.0 { curr_state.most_recent_wand = Hand::Left; }

    curr_state.trigger_total = smoothstep(0.0, 1.0, (curr_state.left.trigger + curr_state.right.trigger).clamp(0.0, 1.0));

    if curr_state.trigger_total == 0.0 {
        curr_state.most_recent_wand = Hand::Neither;
    }



    // Notes & Repeats

    let new_note = (curr_state.note.root as i8
        + stick_to_note_offset(&curr_state.left)
        + stick_to_note_modifier(&curr_state.right)) as u8;

    // If a note is not on, and any trigger started this frame, start a new note
    if !curr_state.note.on {

        if left_trigger_start || right_trigger_start {
            deltas.push(DeltaEvent::NoteStart(new_note));
            curr_state.note.on = true;
        }

    } else {

        // If a note is on, and any trigger ended this frame, and the other trigger
        // is at zero, OR if both triggers are at zero for any reason, end the current note
        if (left_trigger_end && curr_state.right.trigger == 0.0)
        || (right_trigger_end && curr_state.left.trigger == 0.0)
        || curr_state.most_recent_wand == Hand::Neither {
            deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
            curr_state.note.on = false;
        }

        else {
            // If a note is on, but the new note is different
            // repeat note only if RepeatMode.on_stick
            if curr_state.note.current != new_note {
                if REPEAT_MODE.on_stick {
                    deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
                    deltas.push(DeltaEvent::NoteStart(new_note));
                } else {
                    deltas.push(DeltaEvent::NoteChange(curr_state.note.current, new_note));
                }
            }

            // If a note is on, and any trigger started this frame, and the active wand
            // was swapped this frame, repeat note only if RepeatMode.on_trigger
            if REPEAT_MODE.on_trigger {
                if left_trigger_start || right_trigger_start
                && curr_state.most_recent_wand != prev_state.most_recent_wand {
                    deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
                    deltas.push(DeltaEvent::NoteStart(new_note));
                }
            }

            // If a note is on, and any trigger ended this frame, and the other trigger
            // is NOT at zero, and we are not swapping wands, repeat note if RepeatMode.on_swap
            if REPEAT_MODE.on_swap {
                if (left_trigger_end && curr_state.right.trigger > 0.0)
                || (right_trigger_end && curr_state.left.trigger > 0.0) 
                && curr_state.most_recent_wand != prev_state.most_recent_wand {
                    deltas.push(DeltaEvent::NoteEnd(curr_state.note.current));
                    deltas.push(DeltaEvent::NoteStart(new_note));
                }
            }
        }
    }

    curr_state.note.current = new_note;


    // Buttons

    if curr_state.left.stick.clicked && curr_state.right.stick.clicked {
        deltas.push(DeltaEvent::Panic());
    }

    fn each_wand (prev: Wand, curr: Wand, deltas: &mut Vec<DeltaEvent>) {
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
    }

    each_wand(prev_state.left,  curr_state.left,  deltas);
    each_wand(prev_state.right, curr_state.right, deltas);


    //                       ╭─────[ - Tune + ]─────╮
    //           ┏━━━┓     ┏━┷━┓                  ┏━┷━┓     ┏━━━┓
    //         ╭─┨ 4 ┃     ┃ 1 ┃        ││        ┃ 1 ┃     ┃ 4 ┠─╮
    //         │ ┗━━━┛     ┗━━━┛        ││        ┗━━━┛     ┗━━━┛ │
    // ALPHA ]─┤                        ││                        ├─[ OMEGA
    //         │   ┏━━━┓ ┏━━━┓          ││          ┏━━━┓ ┏━━━┓   │
    //         ╰───┨ 3 ┃ ┃ 2 ┃          ││          ┃ 2 ┃ ┃ 3 ┠───╯
    //             ┗━━━┛ ┗━┯━┛                      ┗━┯━┛ ┗━━━┛
    //                     ╰──────[ - Voices + ]──────╯

    // Rocking
    // Triggers on button release, direction from which hand let go last.

    for i in 0..4 {
        if curr_state.left.buttons[i] && curr_state.right.buttons[i] &&
            (!prev_state.left.buttons[i] || !prev_state.right.buttons[i]) {

            let rock_direction:i8 = if !prev_state.left.buttons[i] { -1 } else { 1 };

            match i + 1 { // to match button numbers
                // Tune
                1 => {
                    curr_state.note.root = ((curr_state.note.root as i8) + rock_direction) as u8;
                    deltas.push(DeltaEvent::RootChange(curr_state.note.root));
                },

                // Voice
                2 => {
                    deltas.push(DeltaEvent::VoiceChange(rock_direction));
                },

                _ => {},
            }
        }
    }

    // Thumbsmashes

    for hand in [Hand::Left, Hand::Right].iter() {
        let curr = if *hand == Hand::Left { &curr_state.left } else { &curr_state.right };
        let prev = if *hand == Hand::Left { &prev_state.left } else { &prev_state.right };

        if curr.buttons[2] && curr.buttons[3] && (!prev.buttons[2] || !prev.buttons[3]) {
            match hand {
                Hand::Left  => {
                    curr_state.alpha_lock = !prev_state.alpha_lock;
                },
                Hand::Right => {
                    curr_state.omega_lock = !prev_state.omega_lock;
                }
                Hand::Neither => {},
            }
        }
    }


    // SignalState

    curr_state.signal.bend = curr_state.note.bend;

    // Filter is rot[0] of whichever wand was triggered most recently.
    // TODO: tune lower bound
    curr_state.signal.filter = match curr_state.most_recent_wand {
        Hand::Left  => 0.3 + 0.7 * curr_state.left.rot[0],
        Hand::Right => 0.3 + 0.7 * curr_state.right.rot[0],
        Hand::Neither => prev_state.signal.filter,
    };

    // Physical hand separation ranges ~50 (fingers touching) to ~1500 (full arm span)
    // Sets zero point at about shoulder width. Final range approx 0.3..1.0
    curr_state.signal.width = (curr_state.separation - 500.0) / 1300.0;

    // Fade level as width goes below zero, unless wands are docked (for midi testing)
    curr_state.signal.level =
        if curr_state.docked {
            1.0
        } else {
            smoothstep(0.0, 1.0, 
                unlerp(-0.3, -0.16, curr_state.signal.width)
                .clamp(0.0, 1.0))
        };

    // Whichever wand is moving/accelerating harder, not just most-recently-triggered.
    curr_state.signal.vel = curr_state.left.scalar_vel.max(curr_state.right.scalar_vel);
    curr_state.signal.acc = curr_state.left.scalar_acc.max(curr_state.right.scalar_acc);

    // Depth
    curr_state.signal.depth = match curr_state.most_recent_wand {
        Hand::Left  => curr_state.left.trigger,
        Hand::Right => curr_state.right.trigger,
        Hand::Neither => 0.0,
    };

    // Alt channels
    curr_state.signal.alpha = if curr_state.left.bumper  ^ curr_state.alpha_lock { 1.0 } else { 0.0 };
    curr_state.signal.omega = if curr_state.right.bumper ^ curr_state.omega_lock { 1.0 } else { 0.0 };

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

// Hax
trait Sign {
    fn sign (self) -> f32;
}

impl Sign for f32 {
    fn sign (self) -> f32 {
        if self > 0.0 { 1.0 } else if self < 0.0 { -1.0 } else { 0.0 }
    }
}

