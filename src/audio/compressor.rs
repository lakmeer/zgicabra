
//
// Compressor: plain downward-only peak compressor, for the safety-limiter
// stage ("limiter" in mod.rs) -- distinct from Crusher's OTT-style
// simultaneous up+down squash (crusher.rs), which this does not reuse.
// Stereo-linked: one AFollow envelope tracks max(|l|,|r|) and the same
// gain reduction applies to both channels, so it doesn't shift the stereo
// image the way two independent per-channel followers would.
//

use fundsp::prelude64::*;

const RATIO:   f32 = 4.0;
const ATTACK:  f32 = 0.003;
const RELEASE: f32 = 0.1;

pub struct Compressor {
    follower: AFollow<f32>,
}

impl Compressor {
    pub fn new () -> Compressor {
        Compressor { follower: AFollow::new(ATTACK, RELEASE) }
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        self.follower.set_sample_rate(sample_rate);
    }

    pub fn tick (&mut self, l: f32, r: f32, thresh: f32) -> (f32, f32) {
        let env    = self.follower.filter_mono(l.abs().max(r.abs())).max(1e-6);
        let env_db = amp_db(env);
        let gain_db = if env_db > thresh { (thresh - env_db) * (1.0 - 1.0 / RATIO) } else { 0.0 };
        let gain = db_amp(gain_db);
        (l * gain, r * gain)
    }
}
