
//
// Crusher: OTT-style 3-band simultaneous upward + downward limiter.
// Used inside voices (see reese.rs, swarm.rs).
//
// Splits into low/mid/high bands at LOW_MID_HZ/MID_HIGH_HZ (Xfer OTT's own
// default crossover points), each band pulling its own quiet signal up
// toward threshold_up and pushing its own loud signal down toward
// threshold_down independently, then summing the three back together.
// `depth` is the single knob: it scales both ratio_down and ratio_up
// equally, from 1.0 (no compression, band passes untouched) up to their
// configured maximum -- so it acts as an intensity control on the limiting
// itself rather than a dry/wet blend. There is no dry path: each band's
// output is always fully wet, which is what makes this a limiter (loud/quiet
// always converge toward the thresholds) rather than a parallel-compression
// coloring tool.
//
// The crossover is a cascaded one-pole split (same complementary trick as
// nam_graph::crossover / nam.rs's xover_alpha: low = filtered state, high =
// input - low), so the three bands sum back to the input exactly whenever
// depth is 0 -- bypass stays bit-identical to dry, band split or not.
//

use fundsp::prelude64::*;

const LOW_MID_HZ:  f32 = 88.3;  // Xfer OTT's low/mid crossover default
const MID_HIGH_HZ: f32 = 2500.0; // Xfer OTT's mid/high crossover default

fn xover_alpha (cutoff_hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate.max(1.0)).exp()
}

// Extra display-only ballistics for a GR meter -- separate from the bands'
// own followers, which drive the actual gain math and must stay untouched
// by anything meter-related. disp_env_db/disp_out_db smooth the overall
// (pre-split) input and (post-sum) output at meter-friendly attack/release
// rates; gr_peak_db is a held-and-decaying peak of their gap, exactly like a
// real GR meter's peak readout.
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

// One band's own envelope follower plus the up/down gain math -- identical
// per band, only the input signal (and its envelope) differ.
#[derive(Clone)]
struct Band {
    follower: AFollow<f32>,
}

impl Band {
    fn new () -> Band {
        Band { follower: AFollow::new(0.005, 0.15) }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.follower.set_sample_rate(sample_rate);
    }

    #[allow(clippy::too_many_arguments)]
    fn process (
        &mut self, x: f32, attack: f32, release: f32,
        threshold_down: f32, threshold_up: f32,
        ratio_down: f32, ratio_up: f32,
        depth: f32, makeup_db: f32,
    ) -> f32 {
        self.follower.set_time(attack, release);

        let env    = self.follower.filter_mono(x.abs()).max(1e-6);
        let env_db = amp_db(env);

        // depth interpolates each ratio from 1.0 (bypassed) to its
        // configured maximum, scaling both stages by the same amount.
        let ratio_down = (1.0 + (ratio_down - 1.0) * depth).max(1.0);
        let ratio_up   = (1.0 + (ratio_up   - 1.0) * depth).max(1.0);

        // Downward: pull loud signal toward threshold_down.
        let gain_down_db = if env_db > threshold_down {
            (threshold_down - env_db) * (1.0 - 1.0 / ratio_down)
        } else { 0.0 };

        // Upward: push quiet signal toward threshold_up.
        let gain_up_db = if env_db < threshold_up {
            (threshold_up - env_db) * (1.0 - 1.0 / ratio_up)
        } else { 0.0 };

        let gain = db_amp(gain_down_db + gain_up_db + makeup_db);

        x * gain
    }
}

#[derive(Clone)]
pub struct Crusher {
    ratio_down:     f32,
    threshold_up:   f32,
    ratio_up:       f32,
    release:        f32,
    threshold_down: f32,

    band_low:  Band,
    band_mid:  Band,
    band_high: Band,

    // Cascaded one-pole crossover state: low_mid_lp splits low out of the
    // input, mid_high_lp then splits mid out of what's left (see module doc).
    low_mid_lp:  f32,
    mid_high_lp: f32,

    // Separate from the bands' followers -- these track the overall
    // pre-split/post-sum signal purely for meter display.
    meter_in_follower:  AFollow<f32>,
    meter_out_follower: AFollow<f32>,

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
        ratio_down: f32,
        threshold_up: f32,
        ratio_up: f32,
        release: f32,
        threshold_down: f32,
        meter_env: Shared,
        meter_out: Shared,
        meter_gr: Shared,
    ) -> Crusher {
        Crusher {
            ratio_down,
            threshold_up,
            ratio_up,
            release,
            threshold_down,
            band_low:  Band::new(),
            band_mid:  Band::new(),
            band_high: Band::new(),
            low_mid_lp:  0.0,
            mid_high_lp: 0.0,
            meter_in_follower:  AFollow::new(0.005, 0.15),
            meter_out_follower: AFollow::new(0.005, 0.15),
            sample_rate:    DEFAULT_SR as f32,
            disp_env_db:    METER_FLOOR_DB,
            disp_out_db:    METER_FLOOR_DB,
            gr_peak_db:     0.0,
            gr_peak_hold_s: 0.0,
            meter_env, meter_out, meter_gr,
        }
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        self.band_low.set_sample_rate(sample_rate);
        self.band_mid.set_sample_rate(sample_rate);
        self.band_high.set_sample_rate(sample_rate);
        self.meter_in_follower.set_sample_rate(sample_rate);
        self.meter_out_follower.set_sample_rate(sample_rate);
        self.sample_rate = sample_rate as f32;
    }

    pub fn tick (&mut self, x: f32, depth: f32, attack: f32, makeup_db: f32) -> f32 {
        let attack = attack.max(0.0001);
        let depth  = depth.clamp(0.0, 1.0);

        // 3-way split: low peels off first, then the remainder splits into
        // mid/high. low + mid + high == x exactly (see module doc).
        let a_lm = xover_alpha(LOW_MID_HZ, self.sample_rate);
        let a_mh = xover_alpha(MID_HIGH_HZ, self.sample_rate);

        self.low_mid_lp += a_lm * (x - self.low_mid_lp);
        let low  = self.low_mid_lp;
        let rest = x - low;

        self.mid_high_lp += a_mh * (rest - self.mid_high_lp);
        let mid  = self.mid_high_lp;
        let high = rest - mid;

        let out_low = self.band_low.process(
            low, attack, self.release, self.threshold_down, self.threshold_up,
            self.ratio_down, self.ratio_up, depth, makeup_db,
        );
        let out_mid = self.band_mid.process(
            mid, attack, self.release, self.threshold_down, self.threshold_up,
            self.ratio_down, self.ratio_up, depth, makeup_db,
        );
        let out_high = self.band_high.process(
            high, attack, self.release, self.threshold_down, self.threshold_up,
            self.ratio_down, self.ratio_up, depth, makeup_db,
        );

        let out = out_low + out_mid + out_high;

        self.meter_in_follower.set_time(attack, self.release);
        self.meter_out_follower.set_time(attack, self.release);
        let env_db     = amp_db(self.meter_in_follower.filter_mono(x.abs()).max(1e-6));
        let out_env_db = amp_db(self.meter_out_follower.filter_mono(out.abs()).max(1e-6));

        let dt_s = 1.0 / self.sample_rate.max(1.0);
        self.disp_env_db = meter_ballistic(self.disp_env_db, env_db, dt_s);
        self.disp_out_db = meter_ballistic(self.disp_out_db, out_env_db, dt_s);

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

        out
    }
}
