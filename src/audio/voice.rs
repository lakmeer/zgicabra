
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
use super::growl::WavetableGen;
use super::gorgle::GorgleGen;

pub trait Voice: AudioNode<Inputs = U2, Outputs = U2> {
    const INDEX: usize;
    fn name (&self) -> &'static str;
    // Read-only access to the live signal (thump, bend/pitch, velocity,
    // etc) for whatever internal modulation a voice wants -- both voices
    // below no-op this today.
    fn set_signal (&mut self, signal: &SignalState);
    // Extension point for a future voice wrapping a NamStage internally:
    // NamStage::process_block needs a real block, so such a voice would
    // fill a scratch buffer across its own tick() calls and run inference
    // here, mirroring Engine's pre_nam/run_nam/post_nam split for the
    // global amp stage. Called once per cpal callback chunk on every voice.
    fn on_block_start (&mut self, _block_len: usize) {}
}

// (name, value) pairs for every param a voice exposes -- the shape
// snapshot.rs needs to save/load one voice's params as flat text.
pub trait VoiceParams: Default {
    fn voice_name () -> &'static str;
    fn fields (&self) -> Vec<(&'static str, f32)>;
    fn from_fields (fields: &[(String, f32)]) -> Self;
}

//
// Growl -- wraps WavetableGen (growl.vital, unmodified) with a live Shared
// per param, same as everything else in this engine.
//

#[derive(Clone, Copy)]
pub struct GrowlParams {
    pub bass_drive: f32,
    pub filter:     f32,
    pub space:      f32,
    pub warp:       f32,
}

impl Default for GrowlParams {
    fn default () -> GrowlParams {
        GrowlParams { bass_drive: 0.8, filter: 0.9, space: 0.25, warp: 0.3 }
    }
}

impl VoiceParams for GrowlParams {
    fn voice_name () -> &'static str { "growl" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("bass_drive", self.bass_drive),
            ("filter",     self.filter),
            ("space",      self.space),
            ("warp",       self.warp),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> GrowlParams {
        let mut params = GrowlParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "bass_drive" => params.bass_drive = *value,
                "filter"     => params.filter     = *value,
                "space"      => params.space      = *value,
                "warp"       => params.warp       = *value,
                _ => {},
            }
        }
        params
    }
}

// GUI/AudioHandles-facing handle: just the live Shared cells, no DSP state --
// cheap to clone (Arc bump), safe to hand to the GUI thread.
#[derive(Clone)]
pub struct GrowlHandle {
    pub bass_drive: Shared,
    pub filter:     Shared,
    pub space:      Shared,
    pub warp:       Shared,
}

impl GrowlHandle {
    pub fn new (params: &GrowlParams) -> GrowlHandle {
        GrowlHandle {
            bass_drive: shared(params.bass_drive),
            filter:     shared(params.filter),
            space:      shared(params.space),
            warp:       shared(params.warp),
        }
    }

    pub fn params (&self) -> GrowlParams {
        GrowlParams {
            bass_drive: self.bass_drive.value(),
            filter:     self.filter.value(),
            space:      self.space.value(),
            warp:       self.warp.value(),
        }
    }

    pub fn load (&self, params: &GrowlParams) {
        self.bass_drive.set_value(params.bass_drive);
        self.filter.set_value(params.filter);
        self.space.set_value(params.space);
        self.warp.set_value(params.warp);
    }
}

// Audio-thread owner: the real WavetableGen plus the same Shared cells the
// handle above holds (cloned in, same underlying Arc -- edits sync).
// AudioNode requires Self: Clone -- WavetableGen's own Clone impl resets to
// a fresh, un-warmed-up instance (see wavetable_gen.rs), same as it always
// has; the handle's Shared cells clone cheap (Arc bump) and stay live.
#[derive(Clone)]
pub struct GrowlVoice {
    inner:  WavetableGen,
    handle: GrowlHandle,
}

impl GrowlVoice {
    pub fn new (handle: GrowlHandle) -> GrowlVoice {
        GrowlVoice { inner: WavetableGen::new(), handle }
    }
}

impl AudioNode for GrowlVoice {
    const ID: u64 = 0x7A_40;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        self.inner.tick(&Frame::from([
            freq, 1.0,
            self.handle.bass_drive.value(), self.handle.filter.value(),
            self.handle.space.value(), self.handle.warp.value(),
        ]))
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.inner.set_sample_rate(sample_rate);
    }
}

impl Voice for GrowlVoice {
    const INDEX: usize = 0;
    fn name (&self) -> &'static str { "Growl" }
    fn set_signal (&mut self, _signal: &SignalState) {}
}

//
// Basic -- 4 fundsp builtin oscillators (sin/tri/square/saw), independently
// level-mixed. A test voice, not a real patch.
//

#[derive(Clone, Copy)]
pub struct BasicParams {
    pub sin_level:    f32,
    pub tri_level:    f32,
    pub square_level: f32,
    pub saw_level:    f32,
}

impl Default for BasicParams {
    fn default () -> BasicParams {
        BasicParams { sin_level: 0.25, tri_level: 0.25, square_level: 0.25, saw_level: 0.25 }
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
}

impl BasicHandle {
    pub fn new (params: &BasicParams) -> BasicHandle {
        BasicHandle {
            sin_level:    shared(params.sin_level),
            tri_level:    shared(params.tri_level),
            square_level: shared(params.square_level),
            saw_level:    shared(params.saw_level),
        }
    }

    pub fn params (&self) -> BasicParams {
        BasicParams {
            sin_level:    self.sin_level.value(),
            tri_level:    self.tri_level.value(),
            square_level: self.square_level.value(),
            saw_level:    self.saw_level.value(),
        }
    }

    pub fn load (&self, params: &BasicParams) {
        self.sin_level.set_value(params.sin_level);
        self.tri_level.set_value(params.tri_level);
        self.square_level.set_value(params.square_level);
        self.saw_level.set_value(params.saw_level);
    }
}

#[derive(Clone)]
pub struct BasicVoice {
    sin:    An<Sine<f64>>,
    tri:    An<WaveSynth<U1>>,
    square: An<WaveSynth<U1>>,
    saw:    An<WaveSynth<U1>>,
    handle: BasicHandle,
}

impl BasicVoice {
    pub fn new (handle: BasicHandle) -> BasicVoice {
        BasicVoice { sin: sine(), tri: triangle(), square: square(), saw: saw(), handle }
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

        let mono = self.sin.filter_mono(freq)    * self.handle.sin_level.value()
                 + self.tri.filter_mono(freq)    * self.handle.tri_level.value()
                 + self.square.filter_mono(freq) * self.handle.square_level.value()
                 + self.saw.filter_mono(freq)    * self.handle.saw_level.value();
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sin.set_sample_rate(sample_rate);
        self.tri.set_sample_rate(sample_rate);
        self.square.set_sample_rate(sample_rate);
        self.saw.set_sample_rate(sample_rate);
    }
}

impl Voice for BasicVoice {
    const INDEX: usize = 1;
    fn name (&self) -> &'static str { "Basic" }
    fn set_signal (&mut self, _signal: &SignalState) {}
}

//
// Gorgle -- wraps GorgleGen (gorgle.vital, unmodified) with a live Shared
// per param, same as Growl above.
//

#[derive(Clone, Copy)]
pub struct GorgleParams {
    pub wobble:   f32,
    pub ambience: f32,
    pub girgle:   f32,
    pub grind:    f32,
}

impl Default for GorgleParams {
    fn default () -> GorgleParams {
        GorgleParams { wobble: 0.3, ambience: 0.4, girgle: 0.3, grind: 0.25 }
    }
}

impl VoiceParams for GorgleParams {
    fn voice_name () -> &'static str { "gorgle" }

    fn fields (&self) -> Vec<(&'static str, f32)> {
        vec![
            ("wobble",   self.wobble),
            ("ambience", self.ambience),
            ("girgle",   self.girgle),
            ("grind",    self.grind),
        ]
    }

    fn from_fields (fields: &[(String, f32)]) -> GorgleParams {
        let mut params = GorgleParams::default();
        for (name, value) in fields {
            match name.as_str() {
                "wobble"   => params.wobble   = *value,
                "ambience" => params.ambience = *value,
                "girgle"   => params.girgle   = *value,
                "grind"    => params.grind    = *value,
                _ => {},
            }
        }
        params
    }
}

#[derive(Clone)]
pub struct GorgleHandle {
    pub wobble:   Shared,
    pub ambience: Shared,
    pub girgle:   Shared,
    pub grind:    Shared,
}

impl GorgleHandle {
    pub fn new (params: &GorgleParams) -> GorgleHandle {
        GorgleHandle {
            wobble:   shared(params.wobble),
            ambience: shared(params.ambience),
            girgle:   shared(params.girgle),
            grind:    shared(params.grind),
        }
    }

    pub fn params (&self) -> GorgleParams {
        GorgleParams {
            wobble:   self.wobble.value(),
            ambience: self.ambience.value(),
            girgle:   self.girgle.value(),
            grind:    self.grind.value(),
        }
    }

    pub fn load (&self, params: &GorgleParams) {
        self.wobble.set_value(params.wobble);
        self.ambience.set_value(params.ambience);
        self.girgle.set_value(params.girgle);
        self.grind.set_value(params.grind);
    }
}

#[derive(Clone)]
pub struct GorgleVoice {
    inner:  GorgleGen,
    handle: GorgleHandle,
}

impl GorgleVoice {
    pub fn new (handle: GorgleHandle) -> GorgleVoice {
        GorgleVoice { inner: GorgleGen::new(), handle }
    }
}

impl AudioNode for GorgleVoice {
    const ID: u64 = 0x7A_51;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        self.inner.tick(&Frame::from([
            freq, 1.0,
            self.handle.wobble.value(), self.handle.ambience.value(),
            self.handle.girgle.value(), self.handle.grind.value(),
        ]))
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.inner.set_sample_rate(sample_rate);
    }
}

impl Voice for GorgleVoice {
    const INDEX: usize = 2;
    fn name (&self) -> &'static str { "Gorgle" }
    fn set_signal (&mut self, _signal: &SignalState) {}
}
