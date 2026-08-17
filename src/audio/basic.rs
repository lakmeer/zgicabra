
//
// Basic -- 4 fundsp builtin oscillators (sin/tri/square/saw), independently
// level-mixed. A test voice, not a real patch.
//

use fundsp::prelude64::*;

use crate::zgicabra::SignalState;
use super::voice::{Voice, ThumpMod};

// Fixed defaults, formerly BasicParams::default() -- seeded directly into
// the Shared cells below now that there's no separate snapshot/handle shape.
const DEFAULT_SIN_LEVEL:    f32 = 0.25;
const DEFAULT_TRI_LEVEL:    f32 = 0.25;
const DEFAULT_SQUARE_LEVEL: f32 = 0.25;
const DEFAULT_SAW_LEVEL:    f32 = 0.25;
const DEFAULT_SATURATION:   f32 = 1.0;

#[derive(Clone)]
pub struct BasicVoice {
    sin:    An<Sine<f64>>,
    tri:    An<WaveSynth<U1>>,
    square: An<WaveSynth<U1>>,
    saw:    An<WaveSynth<U1>>,

    pub sin_level:    Shared,
    pub tri_level:    Shared,
    pub square_level: Shared,
    pub saw_level:    Shared,
    pub saturation:   Shared,

    thump:        ThumpMod,
    thump_signal: f32,
}

// Read-only-from-outside view onto BasicVoice's Shared cells -- see
// GrowlView's doc in growl.rs for why this exists.
#[derive(Clone)]
pub struct BasicView {
    pub sin_level:    Shared,
    pub tri_level:    Shared,
    pub square_level: Shared,
    pub saw_level:    Shared,
    pub saturation:   Shared,
}

impl BasicView {
    pub fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("sin_level",    self.sin_level.value()),
            ("tri_level",    self.tri_level.value()),
            ("square_level", self.square_level.value()),
            ("saw_level",    self.saw_level.value()),
            ("saturation",   self.saturation.value()),
        ]
    }

    pub fn apply (&self, fields: &[(String, f32)]) {
        for (name, value) in fields {
            match name.as_str() {
                "sin_level"    => self.sin_level.set_value(*value),
                "tri_level"    => self.tri_level.set_value(*value),
                "square_level" => self.square_level.set_value(*value),
                "saw_level"    => self.saw_level.set_value(*value),
                "saturation"   => self.saturation.set_value(*value),
                _ => {},
            }
        }
    }
}

impl BasicVoice {
    pub fn view (&self) -> BasicView {
        BasicView {
            sin_level:    self.sin_level.clone(),
            tri_level:    self.tri_level.clone(),
            square_level: self.square_level.clone(),
            saw_level:    self.saw_level.clone(),
            saturation:   self.saturation.clone(),
        }
    }

    pub fn new (thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> BasicVoice {
        BasicVoice {
            sin: sine(), tri: triangle(), square: square(), saw: saw(),

            sin_level:    shared(DEFAULT_SIN_LEVEL),
            tri_level:    shared(DEFAULT_TRI_LEVEL),
            square_level: shared(DEFAULT_SQUARE_LEVEL),
            saw_level:    shared(DEFAULT_SAW_LEVEL),
            saturation:   shared(DEFAULT_SATURATION),

            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay), thump_signal: 0.0,
        }
    }
}

impl AudioNode for BasicVoice {
    const ID: u64 = 0x7A_41;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        let freq = freq * self.thump.tick(self.thump_signal);

        let mono = self.sin.filter_mono(freq)    * self.sin_level.value()
                 + self.tri.filter_mono(freq)    * self.tri_level.value()
                 + self.square.filter_mono(freq) * self.square_level.value()
                 + self.saw.filter_mono(freq)    * self.saw_level.value();
        let mono = (mono * self.saturation.value()).tanh();
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sin.set_sample_rate(sample_rate);
        self.tri.set_sample_rate(sample_rate);
        self.square.set_sample_rate(sample_rate);
        self.saw.set_sample_rate(sample_rate);
        self.thump.set_sample_rate(sample_rate);
    }
}

impl Voice for BasicVoice {
    const INDEX: usize = 2;
    fn name (&self) -> &'static str { "Basic" }
    fn set_signal (&mut self, _bend: f32, _filter: f32, _fuzz: f32, _width: f32, thump: f32) {
        self.thump_signal = thump;
    }

    // CC 40-44, 0..1 normalized input scaled to each param's own range.
    // CC2-6 is the
    // same set of knobs (CC1 being reserved for the global Mod Wheel ->
    // filter mapping, see hydra/midi.rs) so a controller with only 8
    // physical knobs can still reach them live.
    fn apply_cc (&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            40 | 2 => self.sin_level.set_value(value),
            41 | 3 => self.tri_level.set_value(value),
            42 | 4 => self.square_level.set_value(value),
            43 | 5 => self.saw_level.set_value(value),
            44 | 6 => self.saturation.set_value(1.0 + value * 9.0),
            _ => {},
        }
    }
}
