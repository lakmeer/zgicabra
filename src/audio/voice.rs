
//
// Voice: the currently-selected sound generator. Every Voice is wired in
// parallel into Engine and ticked every sample; each one silences itself
// when the live selector doesn't match its own INDEX, so only the selected
// voice burns CPU despite the whole graph staying wired.
//
// Each concrete Voice owns its tunable params directly, as `Shared` cells
// (for anything outside the audio thread needs to read, e.g. gui.rs's
// read-only meters) or plain fields (for anything nobody outside the audio
// thread touches). The audio thread is the only writer of every per-voice
// value -- tuning happens live via MIDI CC (apply_cc below, see
// src/audio/cc_input.rs), not GUI knob-dragging -- so there's no separate
// GUI-facing handle type to keep in sync.
//
// CC registry (per-voice, non-overlapping; distinct from the global
// performance-signal CCs 1-4/7-8 in hydra/midi.rs): Reese 20-27, Growl
// 30-34, Basic 40-44, Swarm 50-54. See each voice's apply_cc for the exact
// cc -> field mapping.
//

use fundsp::prelude64::*;

pub trait Voice: AudioNode<Inputs = U2, Outputs = U2> {
    const INDEX: usize;
    fn name (&self) -> &'static str;
    // The 5 hand-riddable performance signals (W/F/B/Z/T, see gui.rs
    // draw_signal_state), always passed in full so a voice that doesn't
    // care about one just ignores the argument.
    fn set_signal (&mut self, bend: f32, filter: f32, fuzz: f32, width: f32, thump: f32);
    // Called once per cpal callback chunk, before that block's tick()
    // calls -- extension point for a voice that needs block-driven
    // inference (a NamStage internally, say): fill a scratch buffer across
    // tick() calls, run it here.
    fn on_block_start (&mut self, _block_len: usize) {}
    // Applies one MIDI CC message (value pre-normalized 0..1) to whichever
    // of this voice's params that cc number maps to; unmapped cc numbers
    // are ignored. See the CC registry above.
    fn apply_cc (&mut self, cc: u8, value: f32);
}

// Pitch-thump envelope: each voice owns its own copy so `thump` is a signal
// a voice reacts to, not a pre-bent freq it's handed.
#[derive(Clone)]
pub(super) struct ThumpMod {
    trigger: Shared,
    peak:    Shared,
    decay:   Shared,
    last_trigger:    f32,
    elapsed_samples: f32,
    sample_rate:     f32,
}

impl ThumpMod {
    pub(super) fn new (trigger: Shared, peak: Shared, decay: Shared) -> ThumpMod {
        ThumpMod { trigger, peak, decay, last_trigger: 0.0, elapsed_samples: 0.0, sample_rate: DEFAULT_SR as f32 }
    }

    pub(super) fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }

    // Returns a pitch multiplier to apply to this voice's freq -- 1.0 at
    // rest, bumped up (decaying over thump_decay seconds) each time
    // thump_trigger changes, scaled by thump_peak and the live signal_thump.
    pub(super) fn tick (&mut self, signal_thump: f32) -> f32 {
        let trigger = self.trigger.value();
        if trigger != self.last_trigger {
            self.last_trigger = trigger;
            self.elapsed_samples = 0.0;
        }

        let t = self.elapsed_samples / self.sample_rate;
        self.elapsed_samples += 1.0;

        let decay_sec  = self.decay.value().max(0.001);
        let pitch_bump = self.peak.value() * signal_thump;
        let decay = (-5.0 * t / decay_sec).exp();
        1.0 + decay * pitch_bump
    }
}

