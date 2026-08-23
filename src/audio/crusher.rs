
//
// Crusher: OTT-style 3-band simultaneous upward + downward limiter.
// Used inside voices (see reese.rs, swarm.rs).
//

use fundsp::prelude64::*;

const LOW_MID_HZ:  f32 = 88.3;   // Xfer OTT's low/mid crossover default
const MID_HIGH_HZ: f32 = 2500.0; // Xfer OTT's mid/high crossover default

pub const CRUSH_THRESHOLD:  f32 = -12.0; // dB, fixed
const CRUSH_RELEASE:        f32 = 0.12;
const CRUSH_ATTACK:         f32 = 0.001;
const CRUSH_RATIO_DOWN:     f32 = 2.0;
const CRUSH_RATIO_UP:       f32 = 4.0;
const CRUSH_MAKEUP_DB:      f32 = 9.0;

const METER_ATTACK_MS:        f32 = 3.0;
const METER_RELEASE_MS:       f32 = 40.0;
const GR_PEAK_HOLD_S:         f32 = 1.2;
const GR_PEAK_DECAY_DB_PER_S: f32 = 20.0;
const METER_FLOOR_DB:         f32 = -60.0;

fn meter_ballistic (current: f32, target: f32, dt_s: f32) -> f32 {
    let tau_ms = if target > current { METER_ATTACK_MS } else { METER_RELEASE_MS };
    let coeff = (-dt_s * 1000.0 / tau_ms).exp();
    target + (current - target) * coeff
}

#[derive(Clone)]
struct GrMeter {
    in_follower:  AFollow<f32>,
    out_follower: AFollow<f32>,
    attack:  f32,
    release: f32,

    sample_rate:    f32,
    disp_env_db:    f32,
    disp_out_db:    f32,
    gr_peak_db:     f32,
    gr_peak_hold_s: f32,

    meter_env: Shared,
    meter_out: Shared,
    meter_gr:  Shared,
}

impl GrMeter {
    fn new (meter_env: Shared, meter_out: Shared, meter_gr: Shared) -> GrMeter {
        GrMeter {
            in_follower:  AFollow::new(0.005, 0.15),
            out_follower: AFollow::new(0.005, 0.15),
            attack:         CRUSH_ATTACK,
            release:        CRUSH_RELEASE,
            sample_rate:    DEFAULT_SR as f32,
            disp_env_db:    METER_FLOOR_DB,
            disp_out_db:    METER_FLOOR_DB,
            gr_peak_db:     0.0,
            gr_peak_hold_s: 0.0,
            meter_env, meter_out, meter_gr,
        }
    }
}

impl AudioNode for GrMeter {
    const ID: u64 = 0x7A_C1;
    type Inputs  = U2;
    type Outputs = U1;

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.in_follower.set_sample_rate(sample_rate);
        self.out_follower.set_sample_rate(sample_rate);
        self.sample_rate = sample_rate as f32;
    }

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U1> {
        let (wet_in, dry_in) = (input[0], input[1]);

        self.in_follower.set_time(self.attack, self.release);
        self.out_follower.set_time(self.attack, self.release);
        let env_db     = amp_db(self.in_follower.filter_mono(dry_in.abs()).max(1e-6));
        let out_env_db = amp_db(self.out_follower.filter_mono(wet_in.abs()).max(1e-6));

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

        Frame::from([wet_in])
    }
}

// One band's envelope follower plus the up/down gain math
fn crush_band (
    depth: &Shared,
) -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
    let depth = depth.clone();

    // Branch the signal: pass() carries x through untouched, the other arm
    // rectifies and follows it into an envelope. Recombined as [x, env].
    (pass() ^ (map(|f: &Frame<f32, U1>| f[0].abs().max(1e-6)) >> afollow(CRUSH_ATTACK, CRUSH_RELEASE)))
        >> map(move |f: &Frame<f32, U2>| {
            let (x, env) = (f[0], f[1]);
            let env_db = amp_db(env);

            // depth interpolates each ratio from 1.0 (bypassed) to its
            // configured maximum, scaling both stages by the same amount.
            let d  = depth.value().clamp(0.0, 1.0);
            let rd = (1.0 + (CRUSH_RATIO_DOWN - 1.0) * d).max(1.0);
            let ru = (1.0 + (CRUSH_RATIO_UP   - 1.0) * d).max(1.0);

            // Downward: pull loud signal toward threshold_down.
            let gain_down_db = if env_db > CRUSH_THRESHOLD {
                (CRUSH_THRESHOLD - env_db) * (1.0 - 1.0 / rd)
            } else { 0.0 };

            // Upward: push quiet signal toward threshold.
            let gain_up_db = if env_db < CRUSH_THRESHOLD {
                (CRUSH_THRESHOLD - env_db) * (1.0 - 1.0 / ru)
            } else { 0.0 };

            x * db_amp(gain_down_db + gain_up_db + CRUSH_MAKEUP_DB)
        })
}

// The full graph: 1 in, 1 out. meter_env/meter_out/meter_gr are written
// every tick for ui/comp_meter.rs. depth is read live every tick from a
// Shared -- pass one driven by a CC/live macro, or `&shared(1.0)` for a
// fixed-depth caller.
//
// Drop this straight into a `>>`/`|`/`^` expression, or box it into a
// `Box<dyn AudioUnit>` field to store long-term -- same pattern as
// nam_graph's nodes (see growl.rs's `nam` field).
#[allow(clippy::too_many_arguments)]
pub fn crusher (
    depth: &Shared,
    meter_env: Shared,
    meter_out: Shared,
    meter_gr: Shared,
) -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
    let center_hz: f32 = (LOW_MID_HZ * MID_HIGH_HZ).sqrt();
    let filter_q:  f32 = center_hz / (MID_HIGH_HZ - LOW_MID_HZ);

    (
      (   (lowpass_hz(LOW_MID_HZ,   filter_q) >> crush_band(depth))
        & (bandpass_hz(center_hz,   filter_q) >> crush_band(depth))
        & (highpass_hz(MID_HIGH_HZ, filter_q) >> crush_band(depth))
      ) ^ pass()
    )
    >> An(GrMeter::new(meter_env, meter_out, meter_gr))
}
