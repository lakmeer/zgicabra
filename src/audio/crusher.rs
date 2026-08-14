
//
// Crusher: OTT-style single-band simultaneous upward + downward compressor
// (FxNode, 7 in / 2 out). Not currently wired into Engine (see mod.rs).
// Pulls quiet signal up toward threshold_up and pushes loud signal down
// toward threshold_down at the same time, scaled by depth, blended dry/wet
// by `level`. Gain is recomputed from a smoothed envelope every sample
// rather than smoothing gain separately, to avoid double-smoothing.
//

use fundsp::prelude64::*;

#[derive(Clone)]
pub struct Crusher {
    ratio_down:   f32,
    threshold_up: f32,
    ratio_up:     f32,
    release:      f32,
    mix:          f32,
    follower: AFollow<f32>,
}

impl Crusher {
    pub fn new (ratio_down: f32, threshold_up: f32, ratio_up: f32, release: f32, mix: f32) -> Crusher {
        Crusher {
            ratio_down, threshold_up, ratio_up, release, mix,
            follower: AFollow::new(0.005, 0.15),
        }
    }
}

impl AudioNode for Crusher {
    const ID: u64 = 0x7A_21;
    type Inputs = U7;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U7>) -> Frame<f32, U2> {
        let x = (input[0] + input[1]) * 0.5;
        let level = input[2];

        let threshold_down = input[3];
        let attack          = input[4].max(0.0001);
        let depth            = input[5];
        let makeup_db         = input[6];

        self.follower.set_time(attack, self.release);

        let env    = self.follower.filter_mono(x.abs()).max(1e-6);
        let env_db = amp_db(env);

        // Downward: pull loud signal toward threshold_down.
        let gain_down_db = if env_db > threshold_down {
            (threshold_down - env_db) * (1.0 - 1.0 / self.ratio_down.max(1.0))
        } else { 0.0 };

        // Upward: push quiet signal toward threshold_up.
        let gain_up_db = if env_db < self.threshold_up {
            (self.threshold_up - env_db) * (1.0 - 1.0 / self.ratio_up.max(1.0))
        } else { 0.0 };

        let total_change_db = (gain_down_db + gain_up_db) * depth;
        let gain = db_amp(total_change_db + makeup_db);

        let wet  = x * gain;
        let dry_wet = x * (1.0 - self.mix) + wet * self.mix;
        let mono = x * (1.0 - level) + dry_wet * level;
        Frame::from([mono, mono])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.follower.set_sample_rate(sample_rate);
    }
}

impl super::fx_node::FxNode for Crusher {
    fn name (&self) -> &'static str { "Crusher" }
    fn param_names (&self) -> [&'static str; 4] { ["thresh", "attack", "depth", "boost"] }
}
