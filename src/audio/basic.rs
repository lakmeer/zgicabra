
//
// Basic -- 4 fundsp builtin oscillators (sin/tri/square/saw), independently
// level-mixed. A test voice, not a real patch.
//

use fundsp::prelude64::*;

use crate::zgicabra::SignalState;
use super::voice::{Voice, VoiceParams, ThumpMod};

#[derive(Clone, Copy)]
pub struct BasicParams {
    pub sin_level:    f32,
    pub tri_level:    f32,
    pub square_level: f32,
    pub saw_level:    f32,
    pub saturation:   f32,
}

impl Default for BasicParams {
    fn default () -> BasicParams {
        BasicParams { sin_level: 0.25, tri_level: 0.25, square_level: 0.25, saw_level: 0.25, saturation: 1.0 }
    }
}

impl VoiceParams for BasicParams {
    fn voice_name () -> &'static str { "basic" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("sin_level",    self.sin_level),
            ("tri_level",    self.tri_level),
            ("square_level", self.square_level),
            ("saw_level",    self.saw_level),
            ("saturation",   self.saturation),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> BasicParams {
        let mut params = BasicParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "sin_level"    => params.sin_level    = *value,
                "tri_level"    => params.tri_level    = *value,
                "square_level" => params.square_level = *value,
                "saw_level"    => params.saw_level    = *value,
                "saturation"   => params.saturation   = *value,
                _ => {},
            }
        }
        params
    }
}

#[derive(Clone)]
pub struct BasicHandle {
    pub sin_level:    Shared,
    pub tri_level:    Shared,
    pub square_level: Shared,
    pub saw_level:    Shared,
    pub saturation:   Shared,
}

impl BasicHandle {
    pub fn new (params: &BasicParams) -> BasicHandle {
        BasicHandle {
            sin_level:    shared(params.sin_level),
            tri_level:    shared(params.tri_level),
            square_level: shared(params.square_level),
            saw_level:    shared(params.saw_level),
            saturation:   shared(params.saturation),
        }
    }

    pub fn params (&self) -> BasicParams {
        BasicParams {
            sin_level:    self.sin_level.value(),
            tri_level:    self.tri_level.value(),
            square_level: self.square_level.value(),
            saw_level:    self.saw_level.value(),
            saturation:   self.saturation.value(),
        }
    }

    pub fn load (&self, params: &BasicParams) {
        self.sin_level.set_value(params.sin_level);
        self.tri_level.set_value(params.tri_level);
        self.square_level.set_value(params.square_level);
        self.saw_level.set_value(params.saw_level);
        self.saturation.set_value(params.saturation);
    }
}

#[derive(Clone)]
pub struct BasicVoice {
    sin:    An<Sine<f64>>,
    tri:    An<WaveSynth<U1>>,
    square: An<WaveSynth<U1>>,
    saw:    An<WaveSynth<U1>>,
    handle: BasicHandle,

    thump:        ThumpMod,
    thump_signal: f32,
}

impl BasicVoice {
    pub fn new (handle: BasicHandle, thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared) -> BasicVoice {
        BasicVoice {
            sin: sine(), tri: triangle(), square: square(), saw: saw(), handle,
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

        let mono = self.sin.filter_mono(freq)    * self.handle.sin_level.value()
                 + self.tri.filter_mono(freq)    * self.handle.tri_level.value()
                 + self.square.filter_mono(freq) * self.handle.square_level.value()
                 + self.saw.filter_mono(freq)    * self.handle.saw_level.value();
        let mono = (mono * self.handle.saturation.value()).tanh();
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
}
