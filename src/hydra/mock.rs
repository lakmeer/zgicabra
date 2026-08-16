
//
// Mock Hydra backend
//
// Generates synthetic wand motion via steady sine waves, so the rest of the
// app can be developed and tested without real Hydra hardware attached.
// Triggers/buttons are simulated from the keyboard/gui, toggled on/off
// (rather than held) since terminals don't deliver real key-up events:
//
//   'z' / '.' - toggle left/right trigger
//   'a' / 's' - cycle voice (dev stand-in for the physical Rocking button)
//   '-' / '=' - tune down/up 1 semitone (stand-in for the Tune button)
//   arrow keys - toggle the left wand's joystick to full deflection
//
// The right wand's joystick and every wand's 4 buttons have no keyboard
// mapping -- gui.rs-only, driven straight through MockControls.
//

use std::f32::consts::{PI, TAU};
use std::collections::VecDeque;
use std::sync::{Arc,Mutex};
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};
use std::time::Instant;

use termion::AsyncReader;
use termion::event::Key;
use termion::input::{Keys,TermRead};

use crate::tools::{sin, AtomicF32};
use crate::zgicabra::DeltaEvent;

use super::{Backend,ControllerFrame,LEFT_HAND,RIGHT_HAND,BUTTON_1,BUTTON_2,BUTTON_3,BUTTON_4};
use super::midi;

// Audition sequence: One 16-beat loop at 120bpm.
const SEQ_BPM: f32 = 120.0;
const SEQ_NOTES: [(u8, f32); 6] = [
    (48, 1.5), // C3
    (42, 1.5), // F#2
    (41, 5.0), // F2
    (43, 1.5), // G2
    (37, 1.5), // C#2
    (36, 5.0), // C2
];

fn seq_beat_seconds () -> f32 { 60.0 / SEQ_BPM }

fn seq_total_seconds () -> f32 {
    SEQ_NOTES.iter().map(|(_, beats)| beats).sum::<f32>() * seq_beat_seconds()
}

// Which note is sounding `t` seconds into the loop.
fn seq_note_at (t: f32) -> u8 {
    let mut acc = 0.0;
    for (note, beats) in SEQ_NOTES {
        acc += beats * seq_beat_seconds();
        if t < acc { return note; }
    }
    SEQ_NOTES.last().unwrap().0
}

const BUTTON_BITS: [u32; 4] = [BUTTON_1, BUTTON_2, BUTTON_3, BUTTON_4];

// Shared handle onto a running MockBackend's togglable inputs, so something
// other than the keyboard (e.g. gui.rs) can drive the same mock wand state.
// Every field is the same Arc'd atomic the backend itself reads each frame,
// so writes here take effect immediately with no polling/sync needed.
#[derive(Clone)]
pub struct MockControls {
    pub left_trigger:  Arc<AtomicF32>,
    pub right_trigger: Arc<AtomicF32>,

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

    // See hydra::midi -- inert (0.0/false) on targets with no MIDI support.
    pub midi: midi::MidiState,

    // Audition sequence player toggle (see step_sequence) and its published
    // filter sweep.
    pub seq_playing: Arc<AtomicBool>,
    pub seq_filter:  Arc<AtomicF32>,
}

impl MockControls {
    // Accumulate-since-last-read: bump() adds a step, the background loop
    // drains it (see MockBackend::take_voice_cycle/take_tune_cycle).
    pub fn bump_voice_cycle (&self, delta: i8) {
        self.voice_cycle.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn bump_tune_cycle (&self, delta: i8) {
        self.tune_cycle.fetch_add(delta, Ordering::Relaxed);
    }

    pub fn toggle (flag: &Arc<AtomicBool>) {
        flag.fetch_xor(true, Ordering::Relaxed);
    }

    // Snaps an analog trigger between fully released and fully pulled.
    pub fn toggle_trigger (trigger: &Arc<AtomicF32>) {
        trigger.store(if trigger.load() > 0.5 { 0.0 } else { 1.0 });
    }
}

use super::CbreakGuard;

pub struct MockBackend {
    keys: Option<Keys<AsyncReader>>,
    left_trigger:  Arc<AtomicF32>,
    right_trigger: Arc<AtomicF32>,
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

    // rot_left/rot_right feed straight into wand_frame's rot_quat twist slot
    // rather than through a SignalOverride -- they drive zgicabra's own
    // rotation->bend math, not bypass it the way midi.bend does.
    midi: midi::MidiState,
    notes: Arc<Mutex<VecDeque<DeltaEvent>>>, // Note On/Off/PC events, MIDI or audition-sequence sourced
    _midi_connection: midi::Connection, // held to keep the callback alive; disconnects on drop

    // Audition sequence player (see SEQ_NOTES above): seq_playing is the
    // GUI-driven toggle, seq_filter is the published filter sweep, the rest
    // is this backend's own stepping state.
    seq_playing: Arc<AtomicBool>,
    seq_filter:  Arc<AtomicF32>,
    seq_elapsed: f32,
    seq_note:    Option<u8>,
    seq_last_tick: Instant,
}

impl MockBackend {
    pub fn new() -> MockBackend {
        let cbreak_guard = CbreakGuard::enable();

        println!("Hydra::start - mock backend active. 'z'/'.' toggle triggers, 'a'/'s' cycle voice, '-'/'=' tune, arrows steer left stick, 'q' quits.");

        let notes: Arc<Mutex<VecDeque<DeltaEvent>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (midi, _midi_connection) = midi::connect(notes.clone());

        // termion::async_stdin() panics its worker thread (non-fatally, but
        // noisily) if /dev/tty can't be opened, e.g. no controlling terminal.
        // Probe first and skip the keyboard-control feature rather than crash it.
        let keys = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").ok()
            .map(|_| termion::async_stdin().keys());

        MockBackend {
            keys,
            left_trigger:  Arc::new(AtomicF32::new(0.0)),
            right_trigger: Arc::new(AtomicF32::new(0.0)),
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
            midi,
            notes,
            _midi_connection,
            seq_playing: Arc::new(AtomicBool::new(false)),
            seq_filter:  Arc::new(AtomicF32::new(0.0)),
            seq_elapsed: 0.0,
            seq_note:    None,
            seq_last_tick: Instant::now(),
        }
    }

    // Note On/Off/PC DeltaEvents accumulated since the last call (MIDI input
    // and/or the audition sequence player, see step_sequence below); drains
    // the queue.
    pub fn take_midi_notes (&mut self) -> Vec<DeltaEvent> {
        self.notes.lock().unwrap().drain(..).collect()
    }

    // Steps the audition sequence loop (see SEQ_NOTES) if seq_playing is on,
    // pushing NoteStart/NoteChange/NoteEnd onto the queue take_midi_notes
    // drains, and publishing the filter sweep in seq_filter (fed onto
    // SignalState via bridge.filter.set in main.rs). Resets to the top of
    // the loop each time playback is (re)started.
    fn step_sequence (&mut self) {
        let now = Instant::now();
        let dt  = now.duration_since(self.seq_last_tick).as_secs_f32();
        self.seq_last_tick = now;

        if !self.seq_playing.load(Ordering::Relaxed) {
            if let Some(prev) = self.seq_note.take() {
                self.notes.lock().unwrap().push_back(DeltaEvent::NoteEnd(prev));
            }
            return;
        }

        if self.seq_note.is_none() {
            self.seq_elapsed = 0.0;
        }
        self.seq_elapsed = (self.seq_elapsed + dt) % seq_total_seconds();

        let note  = seq_note_at(self.seq_elapsed);
        let event = match self.seq_note {
            None                        => Some(DeltaEvent::NoteStart(note)),
            Some(prev) if prev != note  => Some(DeltaEvent::NoteChange(prev, note)),
            _                           => None,
        };
        if let Some(event) = event {
            self.notes.lock().unwrap().push_back(event);
        }
        self.seq_note = Some(note);

        // Slow sine drift independent of the melody's rhythm.
        let period = seq_total_seconds() * 1.0;
        let filter = (self.seq_elapsed / period * TAU).sin() * 0.5 + 0.5;
        self.seq_filter.store(filter);
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
            midi: self.midi.clone(),
            seq_playing: self.seq_playing.clone(),
            seq_filter:  self.seq_filter.clone(),
        }
    }

    pub fn should_quit (&mut self) -> bool {
        self.poll_keys();
        self.quit
    }

    // Net voice-cycle direction since the last call; resets to zero on read.
    pub fn take_voice_cycle (&mut self) -> i8 {
        self.voice_cycle.swap(0, Ordering::Relaxed)
    }

    // Net tune direction since the last call; resets to zero on read.
    pub fn take_tune_cycle (&mut self) -> i8 {
        self.tune_cycle.swap(0, Ordering::Relaxed)
    }

    pub fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        self.poll_keys();
        self.step_sequence();

        self.sequence = self.sequence.wrapping_add(1);

        controllers[0] = self.wand_frame(LEFT_HAND,  0.0, self.left_trigger.load(), self.midi.rot_left.load(),
            self.left_stick_x.load(), self.left_stick_y.load(), &self.left_buttons);
        controllers[1] = self.wand_frame(RIGHT_HAND, PI,  self.right_trigger.load(), self.midi.rot_right.load(),
            self.right_stick_x.load(), self.right_stick_y.load(), &self.right_buttons);
    }

    fn poll_keys (&mut self) {
        // Arrow keys toggle the left stick to full deflection on that axis;
        // opposite-direction pairs cancel back to 0.
        let toggle_axis = |axis: &AtomicF32, delta: f32| {
            let v = axis.load();
            axis.store(if v == 0.0 { delta } else { 0.0 });
        };

        let Some(keys) = self.keys.as_mut() else { return };
        while let Some(Ok(key)) = keys.next() {
            match key {
                Key::Char('z') => MockControls::toggle_trigger(&self.left_trigger),
                Key::Char('.') => MockControls::toggle_trigger(&self.right_trigger),
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

    fn wand_frame (&self, hand: u8, phase: f32, trigger: f32, twist: f32, stick_x: f32, stick_y: f32, buttons: &[Arc<AtomicBool>; 4]) -> ControllerFrame {
        let mut frame = ControllerFrame::new();

        frame.which_hand      = hand;
        frame.enabled         = 1;
        frame.sequence_number = self.sequence;
        frame.trigger         = trigger.clamp(0.0, 1.0);
        frame.joystick_x      = stick_x.clamp(-1.0, 1.0);
        frame.joystick_y      = stick_y.clamp(-1.0, 1.0);

        // rot_quat[2] (twist) is CC7/8-driven regardless of sine_drift.
        frame.rot_quat[2] = twist.clamp(-1.0, 1.0);

        if self.sine_drift.load(Ordering::Relaxed) {
            frame.pos = [
                sin(0.13, phase)       * 200.0,
                sin(0.11, phase + 1.0) * 200.0,
                sin(0.09, phase + 2.0) * 200.0,
            ];

            frame.rot_quat[0] = sin(0.19, phase);
            frame.rot_quat[1] = sin(0.17, phase + 0.5);
            frame.rot_quat[3] = 0.0;
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
