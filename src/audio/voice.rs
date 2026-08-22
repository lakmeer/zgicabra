
//
// Voice: one sound generator. Every Voice is wired in parallel into Engine,
// held as `Box<dyn Voice>` in a plain homogeneous array (see engine.rs) --
// Voice is deliberately its own small object-safe trait (no fundsp
// AudioNode/AudioUnit supertrait: those carry associated consts/types that
// rule out trait objects) so the array needs no per-concrete-type dispatch.
// Engine is what decides whether a given voice is the selected one on each
// tick, calling tick() for real if so and on_silence() otherwise -- a
// Voice's own tick() always renders, it has no idea whether it's selected.
//
// Each concrete Voice owns its tunable params directly, as `Shared` cells
// (for anything outside the audio thread needs to read) or plain fields (for
// anything nobody outside the audio thread touches). The audio thread is the
// only writer of every per-voice value -- tuning happens live via MIDI CC
// (apply_cc below, see src/audio/cc_input.rs) -- so there's no separate
// external-facing handle type to keep in sync.
//
// CC registry (per-voice, non-overlapping; distinct from CC1, the global
// Mod Wheel -> filter mapping in hydra/midi.rs): Reese 20-27, Growl 30-34,
// Basic 40-44, Swarm 50-54. CC2-8 mirror the front of each voice's own
// range (e.g. Reese's CC2 == CC20) so an 8-knob controller can reach the
// selected voice's params directly without needing CC20+ automation lanes;
// see each voice's apply_cc for the exact cc -> field mapping.
//

use fundsp::prelude64::*;

// Implemented for every generated *View (see #[derive(Voice)]'s
// gen_view_impl) -- lets code that only needs get/set-by-name access to a
// voice's live params (persisting them to disk) work the same regardless of
// which concrete voice's View it's holding, without knowing that View's own
// field names.
pub trait ViewFields {
    fn fields (&self) -> Vec<(&'static str, f32)>;
    fn apply (&self, fields: &[(String, f32)]);
}

pub trait Voice: Send {
    // Slot this voice occupies in Engine::voices / Handles::voice_selected --
    // fixed per concrete type (see #[voice(index = ..)]).
    fn index (&self) -> usize;
    fn name (&self) -> &'static str;
    // One sample, mono freq in, (left, right) out. Always renders -- the
    // caller (Engine) is the one that knows whether this voice is selected.
    fn tick (&mut self, freq: f32) -> (f32, f32);
    fn set_sample_rate (&mut self, sample_rate: f64);
    // Called once per cpal callback chunk, before that block's tick()
    // calls -- extension point for a voice that needs block-driven
    // inference (a NamStage internally, say): fill a scratch buffer across
    // tick() calls, run it here.
    fn on_block_start (&mut self, _block_len: usize) {}
    // Called by Engine instead of tick() when this voice is NOT selected --
    // default no-op. Override only if a voice must keep book-keeping
    // running while gated out (GrowlVoice advances its NAM scratch cursor
    // here so a mid-block voice switch stays aligned).
    fn on_silence (&mut self) {}
    // Applies one MIDI CC message (value pre-normalized 0..1) to whichever
    // of this voice's params that cc number maps to; unmapped cc numbers
    // are ignored. See the CC registry above.
    fn apply_cc (&mut self, cc: u8, value: f32);
}

// The DSP half of a voice, hand-written by the author. #[derive(Voice)]
// generates the Voice::tick wrapper (thump application) and calls render()
// with a freq that's already thump-applied -- render() is the old per-sample
// tick body, minus the boilerplate prologue.
pub trait VoiceDsp {
    // `freq` is already thump-applied; `thump_mult` is the raw pitch
    // multiplier thump contributed this sample (1.0 at rest), handed over
    // for voices that surface it as telemetry (GrowlVoice's freq_mult_live).
    fn render (&mut self, freq: f32, thump_mult: f32) -> Frame<f32, U2>;
    // Block-driven inference hook (a NamStage internally, say) -- default
    // no-op; the generated Voice::on_block_start delegates here.
    fn on_block_start (&mut self, _block_len: usize) {}
    // Called by the generated Voice::on_silence -- default no-op. Override
    // only if a voice must keep book-keeping running while gated out
    // (GrowlVoice advances its NAM scratch cursor here so a mid-block voice
    // switch stays aligned).
    fn on_silence (&mut self) {}
    // Called at the end of the generated Voice::set_sample_rate (after it
    // forwards to every #[node] field + thump) -- default no-op. Override for
    // a voice that caches the sample rate in a plain field (SwarmVoice).
    fn on_set_sample_rate (&mut self, _sample_rate: f64) {}
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

