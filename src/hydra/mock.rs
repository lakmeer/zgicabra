
//
// Mock Hydra backend
//
// Generates synthetic wand motion via steady sine waves, so the rest of the
// app can be developed and tested without real Hydra hardware attached. The
// analog triggers are simulated from the keyboard, toggled on/off (rather than
// held) since terminals don't deliver real key-up events:
//
//   'z' - toggle the left trigger
//   '.' - toggle the right trigger
//   'a' - cycle to the previous Voice (dev stand-in for the physical Rocking button)
//   's' - cycle to the next Voice
//

use std::f32::consts::PI;

use termion::AsyncReader;
use termion::event::Key;
use termion::input::{Keys,TermRead};

use crate::tools::sin;

use super::{ControllerFrame,LEFT_HAND,RIGHT_HAND};

// Puts stdin in cbreak mode: keystrokes are available immediately (no waiting
// for Enter) without local echo. Unlike termion's `into_raw_mode()` (which
// uses POSIX `cfmakeraw` and also disables output post-processing), this
// leaves stdout's normal `\n` -> `\r\n` translation alone, so it doesn't
// break the rest of the app's plain `print!`/`println!` output.
struct CbreakGuard {
    original: libc::termios,
}

impl CbreakGuard {
    fn enable() -> CbreakGuard {
        unsafe {
            let mut term: libc::termios = std::mem::zeroed();
            libc::tcgetattr(libc::STDIN_FILENO, &mut term);
            let original = term;

            term.c_lflag &= !(libc::ICANON | libc::ECHO);
            term.c_cc[libc::VMIN]  = 1;
            term.c_cc[libc::VTIME] = 0;

            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term);

            CbreakGuard { original }
        }
    }
}

impl Drop for CbreakGuard {
    fn drop (&mut self) {
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original); }
    }
}

pub struct MockBackend {
    keys: Keys<AsyncReader>,
    left_trigger:  bool,
    right_trigger: bool,
    voice_cycle: i8,
    quit: bool,
    sequence: u8,
    _cbreak_guard: CbreakGuard, // restores the terminal on drop
}

impl MockBackend {
    pub fn new() -> MockBackend {
        let cbreak_guard = CbreakGuard::enable();

        println!("Hydra::start - mock backend active. 'z'/'.' toggle triggers, 'a'/'s' cycle voice, 'q' quits.");

        MockBackend {
            keys: termion::async_stdin().keys(),
            left_trigger:  false,
            right_trigger: false,
            voice_cycle: 0,
            quit: false,
            sequence: 0,
            _cbreak_guard: cbreak_guard,
        }
    }

    pub fn should_quit (&mut self) -> bool {
        self.poll_keys();
        self.quit
    }

    // Net voice-cycle direction accumulated since the last call; resets to
    // zero on read. See hydra::take_voice_cycle.
    pub fn take_voice_cycle (&mut self) -> i8 {
        let v = self.voice_cycle;
        self.voice_cycle = 0;
        v
    }

    pub fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        self.poll_keys();

        self.sequence = self.sequence.wrapping_add(1);

        controllers[0] = self.wand_frame(LEFT_HAND,  0.0,     self.left_trigger);
        controllers[1] = self.wand_frame(RIGHT_HAND, PI,      self.right_trigger);
    }

    fn poll_keys (&mut self) {
        while let Some(Ok(key)) = self.keys.next() {
            match key {
                Key::Char('z') => self.left_trigger  = !self.left_trigger,
                Key::Char('.') => self.right_trigger = !self.right_trigger,
                Key::Char('a') => self.voice_cycle -= 1,
                Key::Char('s') => self.voice_cycle += 1,
                Key::Char('q') => self.quit = true,
                _ => {},
            }
        }
    }

    fn wand_frame (&self, hand: u8, phase: f32, trigger_on: bool) -> ControllerFrame {
        let mut frame = ControllerFrame::new();

        frame.which_hand      = hand;
        frame.enabled         = 1;
        frame.sequence_number = self.sequence;
        frame.trigger         = if trigger_on { 1.0 } else { 0.0 };

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

        frame
    }
}
