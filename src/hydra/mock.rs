
//
// Mock Hydra backend
//
// Generates synthetic wand motion via steady sine waves, so the rest of the
// app can be developed and tested without real Hydra hardware attached. The
// analog triggers and buttons are simulated from the keyboard/gui, toggled
// on/off (rather than held) since terminals don't deliver real key-up events:
//
//   'z' - toggle the left trigger
//   '.' - toggle the right trigger
//   'a' - cycle to the previous Voice (dev stand-in for the physical Rocking button)
//   's' - cycle to the next Voice
//   '-' - tune down 1 semitone (dev stand-in for the physical Tune button)
//   '=' - tune up 1 semitone
//   arrow keys - drive the left wand's joystick to a full deflection
//     up/down/left/right; held combinations (e.g. Up+Right still toggled on
//     together) give the correct diagonal, same toggle-since-no-key-up
//     reasoning as the triggers above
//
// The right wand's joystick and every wand's 4 buttons have no keyboard
// mapping (there's no physical control for them) -- they're gui.rs-only,
// driven straight through MockControls.
//

use std::f32::consts::PI;
use std::collections::VecDeque;
use std::sync::{Arc,Mutex};
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};

use termion::AsyncReader;
use termion::event::Key;
use termion::input::{Keys,TermRead};

use midir::{MidiInput,MidiInputConnection,Ignore};

use crate::tools::{sin, AtomicF32};
use crate::zgicabra::{DeltaEvent, Voice};

use super::{Backend,ControllerFrame,LEFT_HAND,RIGHT_HAND,BUTTON_1,BUTTON_2,BUTTON_3,BUTTON_4};

// Optional MIDI controller support: on boot, take the first available MIDI
// input port (if any) and feed its CC/pitch-bend messages straight into
// atomics that main.rs's engine loop pushes onto ZgicabraBridge's existing
// SignalOverride mechanism each tick (see MockControls::midi_* below and
// SignalOverride::set in zgicabra.rs) -- same override path the gui already
// uses to drive signal state, just fed from MIDI instead of imgui widgets.
// No controller present -> quietly skip, same as the rest of mock.rs's
// "works fine with nothing plugged in" ethos.
const CC_FILTER: u8 = 1;
const CC_WIDTH:  u8 = 2;
const CC_FUZZ:   u8 = 3;
const CC_THUMP:  u8 = 4;

// Connects to the first available MIDI input port, if any, and stores
// incoming CC 1-4 / pitch-bend values straight into the given atomics, and
// pushes Note On/Off as DeltaEvents onto `notes` (drained each tick by
// hydra::take_midi_notes -- discrete events, so unlike the CC/bend atomics
// above they go through the normal DeltaEvent pipeline rather than the
// SignalOverride mechanism). Monophonic, last-note-priority, same as a
// single wand trigger: a second Note On while one is already held emits
// NoteChange rather than a second NoteStart; Note Off only ends the note if
// it matches the currently-held one. Returns None (without panicking) if no
// MIDI backend/port is available -- the caller just proceeds without MIDI
// input, same as running with no Hydra hardware attached.
fn connect_midi (filter: Arc<AtomicF32>, width: Arc<AtomicF32>, fuzz: Arc<AtomicF32>, thump: Arc<AtomicF32>, bend: Arc<AtomicF32>, notes: Arc<Mutex<VecDeque<DeltaEvent>>>) -> Option<MidiInputConnection<()>> {
    let mut midi_in = MidiInput::new("zgicabra").ok()?;
    midi_in.ignore(Ignore::None);

    let ports = midi_in.ports();
    let port = ports.first()?;
    let name = midi_in.port_name(port).unwrap_or_default();

    println!("Hydra::start - MIDI controller found: {name}");

    let mut held_note: Option<u8> = None;

    midi_in.connect(port, "zgicabra-midi-in", move |_stamp, message, _| {
        match message {
            [status, cc, value] if status & 0xF0 == 0xB0 => {
                let level = *value as f32 / 127.0;
                match *cc {
                    CC_FILTER => filter.store(level),
                    CC_WIDTH  => width.store(level),
                    CC_FUZZ   => fuzz.store(level),
                    CC_THUMP  => thump.store(level),
                    _ => {},
                }
            },
            [status, lsb, msb] if status & 0xF0 == 0xE0 => {
                let raw = ((*msb as u16) << 7) | *lsb as u16;
                bend.store((raw as f32 - 8192.0) / 8192.0);
            },
            [status, note, velocity] if status & 0xF0 == 0x90 && *velocity > 0 => {
                let event = match held_note {
                    Some(prev) => DeltaEvent::NoteChange(prev, *note),
                    None       => DeltaEvent::NoteStart(*note),
                };
                held_note = Some(*note);
                notes.lock().unwrap().push_back(event);
            },
            [status, note, _] if status & 0xF0 == 0x80 || (status & 0xF0 == 0x90) => {
                if held_note == Some(*note) {
                    held_note = None;
                    notes.lock().unwrap().push_back(DeltaEvent::NoteEnd(*note));
                }
            },
            // Program Change: absolute voice select (PC 0-3, one per voice
            // slot -- see VOICE_NAMES in gui.rs) instead of the rocking
            // button/keyboard's relative cycle().
            [status, program] if status & 0xF0 == 0xC0 => {
                notes.lock().unwrap().push_back(DeltaEvent::VoiceChange(Voice::from_index(*program)));
            },
            _ => {},
        }
    }, ()).ok()
}

const BUTTON_BITS: [u32; 4] = [BUTTON_1, BUTTON_2, BUTTON_3, BUTTON_4];

// Shared handle onto a running MockBackend's togglable inputs, so something
// other than the keyboard (e.g. gui.rs) can drive the same mock wand state.
// Every field is the same Arc'd atomic the backend itself reads each frame,
// so writes here take effect immediately with no polling/sync needed.
#[derive(Clone)]
pub struct MockControls {
    pub left_trigger:  Arc<AtomicBool>,
    pub right_trigger: Arc<AtomicBool>,

    pub left_stick_x:  Arc<AtomicF32>,
    pub left_stick_y:  Arc<AtomicF32>,
    pub right_stick_x: Arc<AtomicF32>,
    pub right_stick_y: Arc<AtomicF32>,

    // Raw physical button numbers (1-4, matching the ASCII diagram in
    // zgicabra.rs), one set per wand.
    pub left_buttons:  [Arc<AtomicBool>; 4],
    pub right_buttons: [Arc<AtomicBool>; 4],

    // Toggles the steady sine wander on pos/rot_quat (see wand_frame) --
    // off by default so a mock wand sits still until asked to drift.
    pub sine_drift: Arc<AtomicBool>,

    voice_cycle: Arc<AtomicI8>,
    tune_cycle:  Arc<AtomicI8>,

    // Latest CC 1-4 / pitch-bend values from the MIDI listener (see
    // connect_midi), read fresh each tick -- unlike voice/tune_cycle these
    // aren't drain-on-read, they're a live "current value" the engine loop
    // pushes onto ZgicabraBridge's SignalOverride each frame. Stay at 0.0
    // untouched if `midi_connected` is false.
    pub midi_filter: Arc<AtomicF32>,
    pub midi_width:  Arc<AtomicF32>,
    pub midi_fuzz:   Arc<AtomicF32>,
    pub midi_thump:  Arc<AtomicF32>,
    pub midi_bend:   Arc<AtomicF32>,
    pub midi_connected: bool,
}

impl MockControls {
    // Same accumulate-since-last-read semantics as MockBackend::take_voice_cycle/
    // take_tune_cycle -- bump() adds a step, the background loop drains it.
    pub fn bump_voice_cycle (&self, delta: i8) {
        self.voice_cycle.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn bump_tune_cycle (&self, delta: i8) {
        self.tune_cycle.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn toggle (flag: &Arc<AtomicBool>) {
        flag.fetch_xor(true, Ordering::Relaxed);
    }
}

use super::CbreakGuard;

pub struct MockBackend {
    keys: Keys<AsyncReader>,
    left_trigger:  Arc<AtomicBool>,
    right_trigger: Arc<AtomicBool>,
    voice_cycle: Arc<AtomicI8>,
    tune_cycle:  Arc<AtomicI8>,

    left_stick_x:  Arc<AtomicF32>,
    left_stick_y:  Arc<AtomicF32>,
    right_stick_x: Arc<AtomicF32>,
    right_stick_y: Arc<AtomicF32>,

    left_buttons:  [Arc<AtomicBool>; 4],
    right_buttons: [Arc<AtomicBool>; 4],

    sine_drift: Arc<AtomicBool>,

    quit: bool,
    sequence: u8,
    _cbreak_guard: CbreakGuard, // restores the terminal on drop

    midi_filter: Arc<AtomicF32>,
    midi_width:  Arc<AtomicF32>,
    midi_fuzz:   Arc<AtomicF32>,
    midi_thump:  Arc<AtomicF32>,
    midi_bend:   Arc<AtomicF32>,
    midi_connected: bool,
    midi_notes: Arc<Mutex<VecDeque<DeltaEvent>>>,
    _midi_connection: Option<MidiInputConnection<()>>, // held to keep the callback alive; disconnects on drop
}

impl MockBackend {
    pub fn new() -> MockBackend {
        let cbreak_guard = CbreakGuard::enable();

        println!("Hydra::start - mock backend active. 'z'/'.' toggle triggers, 'a'/'s' cycle voice, '-'/'=' tune, arrows steer left stick, 'q' quits.");

        let midi_filter = Arc::new(AtomicF32::new(0.0));
        let midi_width  = Arc::new(AtomicF32::new(0.0));
        let midi_fuzz   = Arc::new(AtomicF32::new(0.0));
        let midi_thump  = Arc::new(AtomicF32::new(0.0));
        let midi_bend   = Arc::new(AtomicF32::new(0.0));

        let midi_notes: Arc<Mutex<VecDeque<DeltaEvent>>> = Arc::new(Mutex::new(VecDeque::new()));

        let midi_connection = connect_midi(midi_filter.clone(), midi_width.clone(), midi_fuzz.clone(), midi_thump.clone(), midi_bend.clone(), midi_notes.clone());
        let midi_connected = midi_connection.is_some();
        if !midi_connected {
            println!("Hydra::start - no MIDI controller found, proceeding without MIDI input.");
        }

        MockBackend {
            keys: termion::async_stdin().keys(),
            left_trigger:  Arc::new(AtomicBool::new(false)),
            right_trigger: Arc::new(AtomicBool::new(false)),
            voice_cycle: Arc::new(AtomicI8::new(0)),
            tune_cycle:  Arc::new(AtomicI8::new(0)),
            left_stick_x:  Arc::new(AtomicF32::new(0.0)),
            left_stick_y:  Arc::new(AtomicF32::new(0.0)),
            right_stick_x: Arc::new(AtomicF32::new(0.0)),
            right_stick_y: Arc::new(AtomicF32::new(0.0)),
            left_buttons:  std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),
            right_buttons: std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),
            sine_drift: Arc::new(AtomicBool::new(false)),
            quit: false,
            sequence: 0,
            _cbreak_guard: cbreak_guard,
            midi_filter,
            midi_width,
            midi_fuzz,
            midi_thump,
            midi_bend,
            midi_connected,
            midi_notes,
            _midi_connection: midi_connection,
        }
    }

    // Note On/Off DeltaEvents accumulated since the last call; drains the
    // queue. See connect_midi's doc comment for the monophonic mapping.
    pub fn take_midi_notes (&mut self) -> Vec<DeltaEvent> {
        self.midi_notes.lock().unwrap().drain(..).collect()
    }

    // Shared handle onto this backend's inputs for a UI thread to drive directly.
    pub fn controls (&self) -> MockControls {
        MockControls {
            left_trigger:  self.left_trigger.clone(),
            right_trigger: self.right_trigger.clone(),
            left_stick_x:  self.left_stick_x.clone(),
            left_stick_y:  self.left_stick_y.clone(),
            right_stick_x: self.right_stick_x.clone(),
            right_stick_y: self.right_stick_y.clone(),
            left_buttons:  self.left_buttons.clone(),
            right_buttons: self.right_buttons.clone(),
            sine_drift: self.sine_drift.clone(),
            voice_cycle: self.voice_cycle.clone(),
            tune_cycle:  self.tune_cycle.clone(),
            midi_filter: self.midi_filter.clone(),
            midi_width:  self.midi_width.clone(),
            midi_fuzz:   self.midi_fuzz.clone(),
            midi_thump:  self.midi_thump.clone(),
            midi_bend:   self.midi_bend.clone(),
            midi_connected: self.midi_connected,
        }
    }

    pub fn should_quit (&mut self) -> bool {
        self.poll_keys();
        self.quit
    }

    // Net voice-cycle direction accumulated since the last call; resets to
    // zero on read. See hydra::take_voice_cycle.
    pub fn take_voice_cycle (&mut self) -> i8 {
        self.voice_cycle.swap(0, Ordering::Relaxed)
    }

    // Net tune direction accumulated since the last call; resets to zero on
    // read. See hydra::take_tune_cycle.
    pub fn take_tune_cycle (&mut self) -> i8 {
        self.tune_cycle.swap(0, Ordering::Relaxed)
    }

    pub fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        self.poll_keys();

        self.sequence = self.sequence.wrapping_add(1);

        controllers[0] = self.wand_frame(LEFT_HAND,  0.0, self.left_trigger.load(Ordering::Relaxed),
            self.left_stick_x.load(), self.left_stick_y.load(), &self.left_buttons);
        controllers[1] = self.wand_frame(RIGHT_HAND, PI,  self.right_trigger.load(Ordering::Relaxed),
            self.right_stick_x.load(), self.right_stick_y.load(), &self.right_buttons);
    }

    fn poll_keys (&mut self) {
        // Arrow keys toggle the left stick to a full deflection on that axis;
        // opposite-direction pairs cancel to 0, e.g. Up then Down returns to
        // 0 rather than -1, matching the old boolean-toggle behaviour.
        let toggle_axis = |axis: &AtomicF32, delta: f32| {
            let v = axis.load();
            axis.store(if v == 0.0 { delta } else { 0.0 });
        };

        while let Some(Ok(key)) = self.keys.next() {
            match key {
                Key::Char('z') => MockControls::toggle(&self.left_trigger),
                Key::Char('.') => MockControls::toggle(&self.right_trigger),
                Key::Char('a') => { self.voice_cycle.fetch_add(-1, Ordering::Relaxed); },
                Key::Char('s') => { self.voice_cycle.fetch_add(1, Ordering::Relaxed); },
                Key::Char('-') => { self.tune_cycle.fetch_add(-1, Ordering::Relaxed); },
                Key::Char('=') => { self.tune_cycle.fetch_add(1, Ordering::Relaxed); },
                Key::Up    => toggle_axis(&self.left_stick_y,  1.0),
                Key::Down  => toggle_axis(&self.left_stick_y, -1.0),
                Key::Left  => toggle_axis(&self.left_stick_x, -1.0),
                Key::Right => toggle_axis(&self.left_stick_x,  1.0),
                Key::Char('q') => self.quit = true,
                _ => {},
            }
        }
    }

    fn wand_frame (&self, hand: u8, phase: f32, trigger_on: bool, stick_x: f32, stick_y: f32, buttons: &[Arc<AtomicBool>; 4]) -> ControllerFrame {
        let mut frame = ControllerFrame::new();

        frame.which_hand      = hand;
        frame.enabled         = 1;
        frame.sequence_number = self.sequence;
        frame.trigger         = if trigger_on { 1.0 } else { 0.0 };
        frame.joystick_x      = stick_x.clamp(-1.0, 1.0);
        frame.joystick_y      = stick_y.clamp(-1.0, 1.0);

        if self.sine_drift.load(Ordering::Relaxed) {
            frame.pos = [
                sin(0.13, phase)       * 200.0,
                sin(0.11, phase + 1.0) * 200.0,
                sin(0.09, phase + 2.0) * 200.0,
            ];

            frame.rot_quat = [
                sin(0.19, phase),
                sin(0.17, phase + 0.5),
                sin(0.15, phase + 1.5),
                0.0,
            ];
        }

        for (bit, pressed) in BUTTON_BITS.iter().zip(buttons.iter()) {
            if pressed.load(Ordering::Relaxed) {
                frame.buttons |= *bit;
            }
        }

        frame
    }
}

impl Backend for MockBackend {
    fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        MockBackend::update(self, controllers)
    }

    fn should_quit (&mut self) -> bool {
        MockBackend::should_quit(self)
    }

    fn take_voice_cycle (&mut self) -> i8 {
        MockBackend::take_voice_cycle(self)
    }

    fn take_tune_cycle (&mut self) -> i8 {
        MockBackend::take_tune_cycle(self)
    }

    fn mock_controls (&self) -> Option<MockControls> {
        Some(self.controls())
    }

    fn take_midi_notes (&mut self) -> Vec<DeltaEvent> {
        MockBackend::take_midi_notes(self)
    }
}
