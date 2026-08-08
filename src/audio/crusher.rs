
//
// Crusher: OTT-style single-band simultaneous upward + downward compressor,
// as a self-contained fundsp AudioNode (1 in, 1 out). Pulls quiet signal up
// toward threshold_up and pushes loud signal down toward threshold_down at
// the same time, both scaled by `depth`, then blended dry/wet by `mix`.
//
// Envelope is smoothed via fundsp's own AFollow (same follower Limiter uses
// internally), gain is recomputed from that smoothed envelope every sample
// rather than smoothing the gain separately -- avoids double-smoothing.
//

use fundsp::prelude64::*;

use super::{ParamSpec, param_factor};
use crate::zgicabra::SignalState;

#[derive(Clone)]
pub struct Crusher {
    threshold_down: ParamSpec, ratio_down: ParamSpec,
    threshold_up:   ParamSpec, ratio_up:   ParamSpec,
    attack: ParamSpec, release: ParamSpec,
    depth:  ParamSpec, makeup_gain: ParamSpec, mix: ParamSpec,
    follower: AFollow<f32>,
}

impl Crusher {
    pub fn new (threshold_down: ParamSpec, ratio_down: ParamSpec,
                threshold_up: ParamSpec, ratio_up: ParamSpec,
                attack: ParamSpec, release: ParamSpec,
                depth: ParamSpec, makeup_gain: ParamSpec, mix: ParamSpec) -> Crusher {
        Crusher {
            threshold_down, ratio_down, threshold_up, ratio_up,
            attack, release, depth, makeup_gain, mix,
            follower: AFollow::new(0.005, 0.15),
        }
    }
}

impl AudioNode for Crusher {
    const ID: u64 = 0x7A_21;
    type Inputs = U1;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let x = input[0];
        let rest = SignalState::new();

        let attack  = param_factor(&self.attack, &rest).max(0.0001);
        let release = param_factor(&self.release, &rest).max(0.0001);
        self.follower.set_time(attack, release);

        let env    = self.follower.filter_mono(x.abs()).max(1e-6);
        let env_db = amp_db(env);

        let threshold_down = param_factor(&self.threshold_down, &rest);
        let ratio_down      = param_factor(&self.ratio_down, &rest).max(1.0);
        let threshold_up    = param_factor(&self.threshold_up, &rest);
        let ratio_up        = param_factor(&self.ratio_up, &rest).max(1.0);
        let depth           = param_factor(&self.depth, &rest);
        let makeup_db       = param_factor(&self.makeup_gain, &rest);
        let mix             = param_factor(&self.mix, &rest).clamp(0.0, 1.0);

        // Downward: pull loud signal toward threshold_down.
        let gain_down_db = if env_db > threshold_down {
            (threshold_down - env_db) * (1.0 - 1.0 / ratio_down)
        } else { 0.0 };

        // Upward: push quiet signal toward threshold_up.
        let gain_up_db = if env_db < threshold_up {
            (threshold_up - env_db) * (1.0 - 1.0 / ratio_up)
        } else { 0.0 };

        let total_change_db = (gain_down_db + gain_up_db) * depth;
        let gain = db_amp(total_change_db + makeup_db);

        let wet = x * gain;
        Frame::from([x * (1.0 - mix) + wet * mix])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.follower.set_sample_rate(sample_rate);
    }
}
