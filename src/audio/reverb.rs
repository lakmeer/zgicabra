
//
// Reverb tail, the last stage in the chain. room_size/decay/damp are baked
// into fundsp's FDN at construction time -- not live, changing them needs a
// restart. `wet` is the live dry/wet balance; the caller handles bypass by
// skipping the call, since reverb_stereo mono-sums its input and driving
// wet=0 would still collapse stereo width.
//

use fundsp::prelude64::*;

pub struct ReverbFx {
    tail: Box<dyn AudioUnit>, // 2 in (L, R) / 2 out, built once from reverb_stereo
}

impl ReverbFx {
    pub fn new (room_size: f32, decay: f32, damp: f32) -> ReverbFx {
        ReverbFx { tail: Box::new(reverb_stereo(room_size, decay, damp)) }
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        self.tail.set_sample_rate(sample_rate);
    }

    // Genuinely stereo out: reverb_stereo produces a distinct L/R tail from
    // the mono-summed input.
    pub fn tick (&mut self, l: f32, r: f32, wet: f32) -> (f32, f32) {
        let x = (l + r) * 0.5;
        let wet = wet.clamp(0.0, 1.0);

        let mut tail = [0.0f32; 2];
        self.tail.tick(&[x, x], &mut tail);

        (x + (tail[0] - x) * wet, x + (tail[1] - x) * wet)
    }
}
