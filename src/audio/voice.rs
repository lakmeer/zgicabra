
//
// Voice: the currently-selected sound generator. Every Voice is wired in
// parallel into Engine and ticked every sample; each one silences itself
// when the live selector doesn't match its own INDEX, so only the selected
// voice burns CPU despite the whole graph staying wired.
//
// Each concrete Voice owns whatever Shared params it needs. A paired
// lightweight *Handle struct (just the Shared cells, no DSP state) is what
// AudioHandles/gui.rs hold, so the GUI thread can read/write live params
// without touching the audio thread's own copy. A plain *Params struct
// (just f32s) is the snapshot/performance-default shape -- see VoiceParams.
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

// (name, value) pairs for every param a voice exposes -- the shape
// snapshot.rs needs to save/load one voice's params as flat text.
pub trait VoiceParams: Default {
    fn voice_name () -> &'static str;
    fn fields (&self) -> Vec<(&'static str, f32)>;
    fn from_fields (fields: &[(String, f32)]) -> Self;
}

