
use fundsp::prelude64::*;

const DRIVE: f32 = 4.0;

pub fn stutter () -> An<Unit<U1, U1>> {
    unit::<U1, U1>(Box::new(((triangle() >> shape_fn(|x| x.max(0.0))) * white()) >> shape_fn(|x| (x * DRIVE).tanh())))
}

// Just the clamped triangle half of stutter() on its own -- floor at 0.0 so
// only the positive half of the cycle passes, for use as an amplitude
// modulator elsewhere.
pub fn clamped_triangle () -> An<Unit<U1, U1>> {
    unit::<U1, U1>(Box::new(triangle() >> shape_fn(|x| x.max(0.0))))
}
