
//
// Voice: the currently-selected sound generator, hardcoded and baked --
// replaces the old GenNode/FxNode swap-pool + mod matrix. Every Voice is
// wired in parallel into the graph and ticked every sample (see
// Engine in mod.rs); each one silences itself when the live selector
// doesn't match its own INDEX, so only the selected voice actually burns
// CPU despite the whole graph "remaining wired in".
//
// Each concrete Voice owns whatever Shared params it needs -- there's no
// fixed p1-p4 shape in the trait itself (contrast the old GenNode). A
// paired lightweight *Handle struct (just the Shared cells, no DSP state)
// is what AudioHandles/gui.rs hold, so the GUI thread can read/write live
// params without needing mutable access to the audio thread's own copy.
// A plain *Params struct (just f32s) is the snapshot/performance-default
// shape -- see VoiceParams and snapshot.rs.
//

use fundsp::prelude64::*;

use crate::zgicabra::SignalState;

pub trait Voice: AudioNode<Inputs = U2, Outputs = U2> {
    const INDEX: usize;
    fn name (&self) -> &'static str;
    // Read-only access to the live signal (thump, bend/pitch, velocity,
    // etc) for whatever internal modulation a voice wants -- see ThumpMod
    // below for the one every voice currently uses.
    fn set_signal (&mut self, signal: &SignalState);
    // Extension point for a future voice wrapping a NamStage internally:
    // NamStage::process_block needs a real block, so such a voice would
    // fill a scratch buffer across its own tick() calls and run inference
    // here, mirroring Engine's pre_nam/run_nam/post_nam split for the
    // global amp stage. Called once per cpal callback chunk on every voice.
    fn on_block_start (&mut self, _block_len: usize) {}
}

// Pitch-thump envelope: was a single instance computed centrally in Engine
// and baked into the freq handed to every voice; now each voice owns its
// own copy so the "thump" signal is something a voice reacts to (a signal
// of intent) rather than a pre-bent freq it's just handed. Every voice
// below uses an identical clone of the technique -- a future voice is free
// to do something else with signal.thump instead of embedding this.
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

