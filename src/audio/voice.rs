
//
// Voice
//

use fundsp::prelude64::*;

pub trait ViewFields {
    fn fields (&self) -> Vec<(&'static str, f32)>;
    fn apply (&self, fields: &[(String, f32)]);
}

pub trait Voice: Send {
    fn index (&self) -> usize;
    fn name (&self) -> &'static str;
    fn tick (&mut self, freq: f32) -> (f32, f32);
    fn set_sample_rate (&mut self, sample_rate: f64);
    fn on_block_start (&mut self, _block_len: usize) {}
    fn on_silence (&mut self) {}

    fn knob_count (&self) -> usize;
    fn knob_name (&self, index: usize) -> &'static str;
    fn knob_range (&self, index: usize) -> (f32, f32);
    fn knob_value (&self, index: usize) -> f32;
    fn selected_knob (&self) -> usize;
    fn set_selected_knob (&mut self, index: usize);
    fn set_knob_value (&mut self, index: usize, raw: f32);
}

#[derive(Clone)]
pub struct KnobPickup {
    last_raw:  f32,
    picked_up: bool,
}

impl KnobPickup {
    pub fn new () -> KnobPickup {
        KnobPickup { last_raw: -1.0, picked_up: false }
    }

    pub fn reset (&mut self) {
        self.picked_up = false;
    }

    pub fn update (&mut self, raw: f32, target_norm: f32) -> Option<f32> {
        if !self.picked_up && self.last_raw >= 0.0 {
            let crossed = (self.last_raw <= target_norm && raw >= target_norm)
                       || (self.last_raw >= target_norm && raw <= target_norm);
            if crossed { self.picked_up = true; }
        }
        self.last_raw = raw;
        if self.picked_up { Some(raw) } else { None }
    }
}

pub trait VoiceDsp {
    fn render (&mut self, freq: f32, thump_mult: f32) -> Frame<f32, U2>;
    fn on_block_start (&mut self, _block_len: usize) {}
    fn on_silence (&mut self) {}
    fn on_set_sample_rate (&mut self, _sample_rate: f64) {}
}

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

