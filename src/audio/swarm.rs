
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use fundsp::prelude64::*;
use num_complex::Complex32;

use zgicabra_voice_macro::voice;

use crate::tools::linexp;
use super::signal::SharedSignal;
use super::voice::{Voice, VoiceDsp, ThumpMod, KnobPickup};
use super::nam::{NamModelCycler, NamModelSlot};
use super::nam_graph::nam_mid_side;
use super::nam_node::NAM_WINDOW;
use super::filter::MoogFilterFx;
use super::crusher::{crusher, LOW_MID_HZ, MID_HIGH_HZ};
use super::comb::{comb, Comb, MAX_DELAY_S};

const NUM_OSCS: usize = 5;

#[derive(Clone, Copy)]

enum OscShape { Tri, Saw, Square }

const OSC_SHAPES: [OscShape; NUM_OSCS] = [
    OscShape::Tri, OscShape::Tri, OscShape::Saw, OscShape::Saw, OscShape::Square,
];

const MIN_OSC_FREQ: f32 = 20.0;


fn lerp (a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

const LIMITER_ATTACK:  f32 = 0.003;
const LIMITER_RELEASE: f32 = 0.1;

const CRUSH_DEPTH:           f32 = 1.0;
const MOOG_RESONANCE: f32 = 0.08;

const PAN_NORM_HZ:          f32 = 20.0;
const DEFAULT_CHASE_FACTOR: f32 = 0.99;
const DEFAULT_RADIUS:       f32 = 90.0; // cents -- orbit radius on the freq axis, converted to Hz per-tick relative to the current origin frequency so the detune width stays perceptually consistent across pitch (see radius_hz in tick)
const DEFAULT_ORBIT_SPEED:  f32 = 2.25; // Hz -- rotations per second
const DEFAULT_COMB_TIME:    f32 = 0.01;  // seconds
const DEFAULT_COMB_FF:      f32 = 0.5;
const DEFAULT_COMB_FB:      f32 = 0.0;   // off by default -- feedback can self-resonate

fn model_index_by_name (names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(0)
}

#[voice(index = 3, label = "Swarm", new = manual, thump = manual)]
#[derive(Clone)]
pub struct SwarmVoice {

    angle: [f32; NUM_OSCS], // phase per oscillator, radians
    origin_freq: f32,

    #[node(each)] oscs:    [An<WaveSynth<U1>>; NUM_OSCS],
    #[node] chain_l: ChannelChain,
    #[node] chain_r: ChannelChain,
    #[node] nam: Box<dyn AudioUnit>,

    #[knob(range = 0.0..1.0)]                             pub chase_factor_input: Shared,
    #[knob(range = 0.0..200.0,  set = |v| v * 200.0)]     pub radius_input:       Shared,
    #[knob(range = 0.0..8.0,    set = |v| v * 8.0)]       pub orbit_speed_input:  Shared,
    #[knob(range = 0.001..MAX_DELAY_S, set = |v| 0.001 + v * (MAX_DELAY_S - 0.001))] pub comb_time_input: Shared,
    #[knob(range = -1.0..1.0,   set = |v| v * 2.0 - 1.0)] pub comb_ff_input:      Shared,
    #[knob(range = -0.95..0.95, set = |v| v * 1.9 - 0.95)] pub comb_fb_input:     Shared,

    #[live(range = 0.0..200.0)]  pub radius_live:       Shared,
    #[live(range = 0.0..8.0)]    pub orbit_speed_live:  Shared,
    #[live(range = 0.0..1.0)]    pub nam_hi_live:       Shared,
    #[live(range = 0.0..2000.0)] pub origin_live:       Shared,
    #[live(range = -1.0..1.0)]   pub comb_mix_live:         Shared,

    #[view] pub nam_hi: NamModelCycler,
    #[view] pub osc_freq_live: [Shared; NUM_OSCS],
    #[view] pub osc_pan_live:  [Shared; NUM_OSCS],

    sample_rate: f32,
    thump: ThumpMod,
    sig:   SharedSignal,
}

impl SwarmVoice {
    pub fn new (
        nam_models: Vec<Option<NamModelSlot>>,
        nam_names: Arc<Vec<String>>,
        thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared,
        signal: SharedSignal,
    ) -> SwarmVoice {
        let oscs: [An<WaveSynth<U1>>; NUM_OSCS] = std::array::from_fn(|i| match OSC_SHAPES[i] {
            OscShape::Tri    => triangle(),
            OscShape::Saw    => saw(),
            OscShape::Square => square(),
        });
        let angle: [f32; NUM_OSCS] = std::array::from_fn(|i| i as f32 * TAU / NUM_OSCS as f32);

        let hi_index = model_index_by_name(&nam_names, "sansamp");
        let nam_hi = NamModelCycler::new(shared(hi_index as f32), nam_names);

        let nam_blend   = shared(0.0);
        let nam_hi_live = shared(0.0);

        let slot_hi = nam_models.get(hi_index).and_then(Option::as_ref)
            .expect("swarm hi NAM model slot");

        let nam = Box::new(nam_mid_side(
            slot_hi,
            &shared(0.8), &nam_hi_live,
            NAM_WINDOW,
        ));

        let comb_time_input = shared(DEFAULT_COMB_TIME);
        let comb_ff_input   = shared(DEFAULT_COMB_FF);
        let comb_fb_input   = shared(DEFAULT_COMB_FB);
        let comb_mix_live       = shared(0.0);

        SwarmVoice {
            oscs,
            angle,
            origin_freq: 110.0,
            chain_l: ChannelChain::new(comb_time_input.clone(), comb_ff_input.clone(), comb_fb_input.clone(), comb_mix_live.clone()),
            chain_r: ChannelChain::new(comb_time_input.clone(), comb_ff_input.clone(), comb_fb_input.clone(), comb_mix_live.clone()),

            nam,

            chase_factor_input: shared(DEFAULT_CHASE_FACTOR),
            radius_input:       shared(DEFAULT_RADIUS),
            orbit_speed_input:  shared(DEFAULT_ORBIT_SPEED),
            comb_time_input, comb_ff_input, comb_fb_input, comb_mix_live,
            nam_hi,

            radius_live:       shared(0.0),
            orbit_speed_live:  shared(0.0),
            nam_hi_live,

            osc_freq_live: std::array::from_fn(|_| shared(0.0)),
            osc_pan_live:  std::array::from_fn(|_| shared(0.0)),
            origin_live:   shared(110.0),

            sample_rate: DEFAULT_SR as f32,

            selected_knob: shared(0.0),
            knob_pickup:   KnobPickup::new(),

            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            sig:   signal,
        }
    }
}

impl VoiceDsp for SwarmVoice {
    fn render (&mut self, freq: f32, _thump_mult: f32) -> Frame<f32, U2> {
        let chase_factor = self.chase_factor_input.value().clamp(0.0, 0.999_999) / 1000.0;
        self.origin_freq = lerp(self.origin_freq, freq, chase_factor);
        let origin_freq = self.origin_freq * self.thump.tick(self.sig.thump.value());

        let width_signal = self.sig.width.value().clamp(0.0, 1.0);
        let radius_cents = self.radius_input.value().max(0.0) * (1.0 + width_signal);
        let orbit_speed  = self.orbit_speed_input.value()     * (1.0 + width_signal);
        self.radius_live.set_value(radius_cents);
        self.orbit_speed_live.set_value(orbit_speed);

        let radius_hz = origin_freq * (2.0f32.powf(radius_cents / 1200.0) - 1.0);

        self.origin_live.set_value(origin_freq);

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for k in 0..NUM_OSCS {
            self.angle[k] = (self.angle[k] + orbit_speed * TAU / self.sample_rate).rem_euclid(TAU);

            let position = Complex32::new(origin_freq, 0.0) + Complex32::from_polar(radius_hz, self.angle[k]);
            let osc_freq = position.re.max(MIN_OSC_FREQ);
            let pan      = (position.im / PAN_NORM_HZ).clamp(-1.0, 1.0);

            self.osc_freq_live[k].set_value(osc_freq);
            self.osc_pan_live[k].set_value(pan);

            let dry = self.oscs[k].filter_mono(osc_freq);

            let angle_pan = (pan * 0.5 + 0.5) * (PI * 0.5);
            mix_l += dry * angle_pan.cos();
            mix_r += dry * angle_pan.sin();
        }

        let norm = 1.0 / (NUM_OSCS as f32).sqrt();
        mix_l *= norm;
        mix_r *= norm;

        let filter_cutoff = self.sig.filter.value().clamp(0.0, 1.0);

        let mut namd = [0.0f32; 2];
        self.nam.tick(&[self.chain_l.pre(mix_l), self.chain_r.pre(mix_r)], &mut namd);

        self.comb_mix_live.set_value(self.sig.vel.value());

        let note_env = self.sig.env.value();
        Frame::from([
            self.chain_l.post(namd[0], filter_cutoff) * note_env,
            self.chain_r.post(namd[1], filter_cutoff) * note_env,
        ])
    }

    fn on_set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }
}


// One each for L/R stereo

#[derive(Clone)]
struct ChannelChain {
    limiter: An<Limiter<U1>>,
    moog:    MoogFilterFx,
    crusher: Box<dyn AudioUnit>,
    comb:    An<Comb>,
    comb_time: Shared,
    comb_ff:   Shared,
    comb_fb:   Shared,
    comb_mix:  Shared,
}

impl ChannelChain {
    fn new (comb_time: Shared, comb_ff: Shared, comb_fb: Shared, comb_mix: Shared) -> ChannelChain {
        ChannelChain {
            limiter: limiter(LIMITER_ATTACK, LIMITER_RELEASE),
            moog:    MoogFilterFx::new(),
            crusher: Box::new(crusher(
                &shared(CRUSH_DEPTH),
                shared(LOW_MID_HZ), shared(MID_HIGH_HZ),
                shared(0.0), shared(0.0), shared(0.0),
            )),
            comb: comb(),
            comb_time, comb_ff, comb_fb, comb_mix,
        }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.limiter.set_sample_rate(sample_rate);
        self.moog.set_sample_rate(sample_rate);
        self.crusher.set_sample_rate(sample_rate);
        self.comb.set_sample_rate(sample_rate);
    }

    fn pre (&mut self, x: f32) -> f32 {
        self.limiter.filter_mono(x)
    }

    fn post (&mut self, x: f32, filter_cutoff: f32) -> f32 {
        let filtered = self.moog.tick(x, filter_cutoff, MOOG_RESONANCE);

        let wet = self.crusher.filter_mono(filtered);

        let combed = self.comb.tick(&Frame::from([
            wet,
            self.comb_time.value(),
            self.comb_ff.value(),
            self.comb_fb.value(),
        ]))[0];

        lerp(wet, combed, self.comb_mix.value())
    }
}

