
//
// Crusher: OTT-style single-band simultaneous upward + downward compressor.
// Used inside voices (see reese.rs, swarm.rs). Mono in, mono out, with
// named params -- it was never composed with anything, so the old 7-in/2-out
// FxNode shape was padding.
// Pulls quiet signal up toward threshold_up and pushes loud signal down
// toward threshold_down at the same time, scaled by depth, blended against
// dry by the fixed `mix`. Gain is recomputed from a smoothed envelope every sample
// rather than smoothing gain separately, to avoid double-smoothing.
//

use fundsp::prelude64::*;

// Extra display-only ballistics for a GR meter -- separate from `follower`,
// which drives the actual gain math above and must stay untouched by
// anything meter-related. disp_env_db/disp_out_db re-smooth the
// (already-followed) envelope and its post-gain counterpart at
// meter-friendly attack/release rates; gr_peak_db is a held-and-decaying
// peak of their gap, exactly like a real GR meter's peak readout.
//
// These are written straight into caller-supplied Shared cells rather than
// exposed as getters, so the meters survive the node being moved into a
// graph (a getter is unreachable through Box<dyn AudioUnit>) and the caller
// no longer needs a relay line per meter. fundsp's monitor() can't do this
// job: Meter::Peak is a plain smoothed peak with no hold-and-decay, and
// gain reduction isn't a property of a single signal.
const METER_ATTACK_MS:        f32 = 3.0;
const METER_RELEASE_MS:       f32 = 400.0;
const GR_PEAK_HOLD_S:         f32 = 1.2;
const GR_PEAK_DECAY_DB_PER_S: f32 = 20.0;
const METER_FLOOR_DB:         f32 = -60.0;

fn meter_ballistic (current: f32, target: f32, dt_s: f32) -> f32 {
    let tau_ms = if target > current { METER_ATTACK_MS } else { METER_RELEASE_MS };
    let coeff = (-dt_s * 1000.0 / tau_ms).exp();
    target + (current - target) * coeff
}

#[derive(Clone)]
pub struct Crusher {
    ratio_down:   f32,
    threshold_up: f32,
    ratio_up:     f32,
    release:      f32,
    mix:          f32,
    follower: AFollow<f32>,

    sample_rate:      f32,
    disp_env_db:      f32,
    disp_out_db:      f32,
    gr_peak_db:       f32,
    gr_peak_hold_s:   f32,

    meter_env: Shared,
    meter_out: Shared,
    meter_gr:  Shared,
}

impl Crusher {
    // meter_env/meter_out/meter_gr are written every tick for ui/comp_meter.rs.
    pub fn new (
        ratio_down: f32, threshold_up: f32, ratio_up: f32, release: f32, mix: f32,
        meter_env: Shared, meter_out: Shared, meter_gr: Shared,
    ) -> Crusher {
        Crusher {
            ratio_down, threshold_up, ratio_up, release, mix,
            follower: AFollow::new(0.005, 0.15),
            sample_rate:    DEFAULT_SR as f32,
            disp_env_db:    METER_FLOOR_DB,
            disp_out_db:    METER_FLOOR_DB,
            gr_peak_db:     0.0,
            gr_peak_hold_s: 0.0,
            meter_env, meter_out, meter_gr,
        }
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        self.follower.set_sample_rate(sample_rate);
        self.sample_rate = sample_rate as f32;
    }

    pub fn tick (&mut self, x: f32, threshold_down: f32, attack: f32, depth: f32, makeup_db: f32) -> f32 {
        let attack = attack.max(0.0001);

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

        let wet = x * gain;
        let mono = x * (1.0 - self.mix) + wet * self.mix;

        let dt_s = 1.0 / self.sample_rate.max(1.0);
        self.disp_env_db = meter_ballistic(self.disp_env_db, env_db, dt_s);
        self.disp_out_db = meter_ballistic(self.disp_out_db, env_db + total_change_db, dt_s);

        let gr_now = (self.disp_env_db - self.disp_out_db).max(0.0);
        if gr_now >= self.gr_peak_db {
            self.gr_peak_db = gr_now;
            self.gr_peak_hold_s = GR_PEAK_HOLD_S;
        } else if self.gr_peak_hold_s > 0.0 {
            self.gr_peak_hold_s -= dt_s;
        } else {
            self.gr_peak_db = (self.gr_peak_db - GR_PEAK_DECAY_DB_PER_S * dt_s).max(gr_now);
        }

        self.meter_env.set_value(self.disp_env_db);
        self.meter_out.set_value(self.disp_out_db);
        self.meter_gr.set_value(self.gr_peak_db);

        mono
    }
}
