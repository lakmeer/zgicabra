
//
// Blank -- a no-op voice. Silences itself unconditionally, ignores every
// input. Fills a voice slot that has no patch behind it without disturbing
// VOICE_NAMES/voice_selected indexing for the other voices.
//

use fundsp::prelude64::*;

use super::voice::Voice;

#[derive(Clone)]
pub struct BlankVoice;

impl BlankVoice {
    pub fn new () -> BlankVoice { BlankVoice }
}

impl AudioNode for BlankVoice {
    const ID: u64 = 0x7A_60;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, _input: &Frame<f32, U2>) -> Frame<f32, U2> {
        Frame::from([0.0, 0.0])
    }
}

impl Voice for BlankVoice {
    const INDEX: usize = 3;
    fn name (&self) -> &'static str { "Blank" }
    fn set_signal (&mut self, _bend: f32, _filter: f32, _fuzz: f32, _width: f32, _thump: f32) {}
}
