
//
// Stutter generator: triangle sub-oscillator ring-modulated by white noise,
// so the noise is gated on and off with pitch instead of being a
// fixed-frequency hiss. Currently unused -- kept as a graph expression
// because that is now what a generator looks like here.
//
// 1 in (freq) -> 1 out (mono). Level is the caller's business.
//

use fundsp::prelude64::*;

const DRIVE: f32 = 4.0;

pub fn stutter () -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
    (triangle() * white()) >> shape_fn(|x| (x * DRIVE).tanh())
}
