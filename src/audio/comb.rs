
//
// Comb filter: single delay line combining feedforward and feedback taps
// (the Schroeder structure), general enough to cover both the FIR notch
// form and the IIR resonant form depending on the gains passed in.
//
// v[n] = x[n] + fb * v[n-M]
// y[n] = v[n] + ff * v[n-M]
//

use fundsp::prelude64::*;

pub const MAX_DELAY_S: f32 = 0.05;

#[derive(Clone)]
struct Comb {
    buffer: Vec<f32>,
    pos: usize,
    sample_rate: f32,
}

impl Comb {
    fn new () -> Comb {
        Comb { buffer: vec![0.0; 1], pos: 0, sample_rate: DEFAULT_SR as f32 }
    }
}

impl AudioNode for Comb {
    const ID: u64 = 9003;
    type Inputs  = U4;
    type Outputs = U1;

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        self.buffer = vec![0.0; (MAX_DELAY_S * self.sample_rate).ceil() as usize + 1];
        self.pos = 0;
    }

    fn reset (&mut self) {
        self.buffer.iter_mut().for_each(|s| *s = 0.0);
        self.pos = 0;
    }

    fn tick (&mut self, input: &Frame<f32, U4>) -> Frame<f32, U1> {
        let (x, delay_s, ff, fb) = (input[0], input[1], input[2], input[3]);
        let len = self.buffer.len();

        let delay = (delay_s.max(0.0) * self.sample_rate).min((len - 1) as f32);
        let read  = (self.pos as f32 - delay).rem_euclid(len as f32);
        let (i0, frac) = (read as usize, read.fract());
        let i1 = (i0 + 1) % len;
        let tap = self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac;

        let v = x + fb * tap;
        self.buffer[self.pos] = v;
        self.pos = (self.pos + 1) % len;

        Frame::from([v + ff * tap])
    }
}

//pub fn comb () -> An<impl AudioNode<Inputs = U4, Outputs = U1>> {
pub fn comb () -> An<impl AudioNode<Inputs = U4, Outputs = U1>> {
    An(Comb::new())
}

