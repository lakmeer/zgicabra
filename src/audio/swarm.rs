
//
// Swarm -- 5 oscillators (2 tri, 2 saw, 1 square), each orbiting a shared
// origin point in a 2D plane where x = frequency (Hz) offset and y = pan
// (see num_complex usage below). The origin itself chases the input note's
// frequency with a fast-but-imperfect lerp, so the whole swarm glides
// rather than snapping. Each oscillator's orbit position feeds its own
// frequency offset, stereo pan, and its own hand-rolled phaser (rate
// tracks that oscillator's own orbiting frequency -- see Phaser below).
//
// The summed swarm then splits into two fully independent channel chains
// (limiter -> crossover -> low/high NAM -> moog filter -> crusher), one
// per output channel, mirroring Engine's amp_l/amp_r precedent: NAM
// WaveNet models carry internal dilation state, so sharing one model
// instance across L and R would let one channel's audio bleed into the
// other's model state. Each of the 4 NAM slots below (L-low, L-high,
// R-low, R-high) is therefore its own independently loaded model bank.
//

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use fundsp::prelude64::*;
use num_complex::Complex32;

use crate::tools::linexp;
use super::voice::{Voice, ThumpMod};
use super::nam::{NamStage, NamModelCycler, NamModelSlot, NAM_BLOCK_CAP};
use super::filter::MoogFilterFx;
use super::crusher::Crusher;
use super::compressor::Compressor;

const NUM_OSCS: usize = 5;

#[derive(Clone, Copy)]
enum OscShape { Tri, Saw, Square }
const OSC_SHAPES: [OscShape; NUM_OSCS] = [OscShape::Tri, OscShape::Tri, OscShape::Saw, OscShape::Saw, OscShape::Square];

const MIN_OSC_FREQ: f32 = 20.0;

const PHASER_STAGES: usize = 4;
const PHASER_FC_LO: f32 = 200.0;
const PHASER_FC_HI: f32 = 3000.0;
const PHASER_FEEDBACK: f32 = 0.6;

#[derive(Clone)]
struct Phaser {
    stages: [f32; PHASER_STAGES],
    last_out: f32,
    lfo_phase: f32,
    sample_rate: f32,
}

impl Phaser {
    fn new () -> Phaser {
        Phaser { stages: [0.0; PHASER_STAGES], last_out: 0.0, lfo_phase: 0.0, sample_rate: DEFAULT_SR as f32 }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }

    // rate_hz: this oscillator's own LFO sweep rate. depth: 0..1 sweep amount.
    fn tick (&mut self, x: f32, rate_hz: f32, depth: f32) -> f32 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = (self.lfo_phase * TAU).sin(); // -1..1
        let mod_amt = (0.5 + 0.5 * lfo * depth.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        let fc = linexp(0.0, 1.0, PHASER_FC_LO, PHASER_FC_HI, mod_amt);

        let wc = (PI * fc / self.sample_rate).tan();
        let a = ((wc - 1.0) / (wc + 1.0)).clamp(-0.999, 0.999);

        let mut s = x + self.last_out * PHASER_FEEDBACK;
        for stage in self.stages.iter_mut() {
            let y = -a * s + *stage;
            *stage = s + a * y;
            s = y;
        }
        self.last_out = s;
        x + s // classic phaser: sum of dry and the allpass-chained signal
    }
}

// One-pole (6dB/oct) lowpass coefficient, same shape as NamStage::xover_alpha
// -- duplicated locally since that one's private to nam.rs and this is only
// a couple lines of math.
fn xover_alpha (fc: f32, sample_rate: f32) -> f32 {
    1.0 - (-2.0 * PI * fc / sample_rate).exp()
}

fn lerp (a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

const LIMITER_THRESH_DB: f32 = -24.0; // "low threshold" -- squashes hard to normalize into the crossover

// Fixed Crusher/moog character -- not GUI-exposed, the spec only calls out
// crossover_freq/filter as live-tunable for this stage.
const CRUSH_RATIO_DOWN:   f32 = 3.0;
const CRUSH_THRESHOLD_UP: f32 = -30.0;
const CRUSH_RATIO_UP:     f32 = 2.0;
const CRUSH_RELEASE:      f32 = 0.12;
const CRUSH_MIX:          f32 = 0.5;
const CRUSH_THRESH_DOWN:  f32 = -12.0;
const CRUSH_ATTACK:       f32 = 0.01;
const CRUSH_DEPTH:        f32 = 1.0;
const CRUSH_MAKEUP_DB:    f32 = 0.0;

const MOOG_RESONANCE: f32 = 0.3; // 0..1, no GUI knob specified for this

const PAN_NORM_HZ:          f32 = 20.0;
const DEFAULT_CHASE_FACTOR: f32 = 0.99;
const DEFAULT_RADIUS:       f32 = 6.0;  // Hz -- orbit radius on both the freq and (scaled) pan axes
const DEFAULT_ORBIT_SPEED:  f32 = 2.25; // Hz -- rotations per second
const DEFAULT_PHASER_DEPTH: f32 = 0.4;
const DEFAULT_XOVER_FREQ:   f32 = 400.0; // Hz, splits the swarm mix before the two NAM stages

fn model_index_by_name (names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(0)
}

// One output channel's post-swarm chain: limiter -> xover -> (low NAM,
// high NAM) -> moog filter -> crusher. NAM inference is block-rate (see
// SwarmVoice::on_block_start), so this only owns the per-sample stages plus
// the one-block-latency raw/wet scratch ring for its two NAM bands.
#[derive(Clone)]
struct ChannelChain {
    limiter:  Compressor,
    xover_lp: f32,
    nam_lo:   NamStage,
    nam_hi:   NamStage,
    moog:     MoogFilterFx,
    crusher:  Crusher,
    raw_lo:   Vec<f32>,
    raw_hi:   Vec<f32>,
    pos:      usize,
}

impl ChannelChain {
    fn new (nam_models: Vec<Option<NamModelSlot>>, nam_lo_selected: Shared, nam_hi_selected: Shared) -> ChannelChain {
        ChannelChain {
            limiter: Compressor::new(),
            xover_lp: 0.0,
            nam_lo:  NamStage::new(nam_models.clone(), nam_lo_selected),
            nam_hi:  NamStage::new(nam_models, nam_hi_selected),
            moog:    MoogFilterFx::new(),
            crusher: Crusher::new(CRUSH_RATIO_DOWN, CRUSH_THRESHOLD_UP, CRUSH_RATIO_UP, CRUSH_RELEASE, CRUSH_MIX),
            raw_lo: vec![0.0; NAM_BLOCK_CAP], raw_hi: vec![0.0; NAM_BLOCK_CAP],
            pos: 0,
        }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.limiter.set_sample_rate(sample_rate);
        self.nam_lo.set_sample_rate(sample_rate);
        self.nam_hi.set_sample_rate(sample_rate);
        self.moog.set_sample_rate(sample_rate);
        self.crusher.set_sample_rate(sample_rate);
    }

    fn tick (&mut self, x: f32, sample_rate: f32, filter_cutoff: f32, xover_hz: f32) -> f32 {
        let (limited, _) = self.limiter.tick(x, x, LIMITER_THRESH_DB);

        let alpha = xover_alpha(xover_hz, sample_rate);
        self.xover_lp += alpha * (limited - self.xover_lp);
        let low  = self.xover_lp;
        let high = limited - self.xover_lp;

        let wet_low  = self.raw_lo.get(self.pos).copied().unwrap_or(0.0);
        let wet_high = self.raw_hi.get(self.pos).copied().unwrap_or(0.0);
        if let Some(cell) = self.raw_lo.get_mut(self.pos) { *cell = low; }
        if let Some(cell) = self.raw_hi.get_mut(self.pos) { *cell = high; }
        self.pos += 1;

        let combined = wet_low + wet_high;
        let filtered = self.moog.tick(&Frame::from([combined, combined, 1.0, filter_cutoff, MOOG_RESONANCE, 0.0, 0.0]))[0];
        self.crusher.tick(&Frame::from([filtered, filtered, 1.0, CRUSH_THRESH_DOWN, CRUSH_ATTACK, CRUSH_DEPTH, CRUSH_MAKEUP_DB]))[0]
    }

    fn on_block_start (&mut self, block_len: usize, fuzz_signal: f32) {
        let n = std::cmp::min(block_len, self.raw_lo.len());
        // to save cpu for now
        //self.nam_lo.process_block(&mut self.raw_lo[..n], 1.0, fuzz_signal.clamp(0.0, 1.0), 1.0, 0.0);
        //self.nam_hi.process_block(&mut self.raw_hi[..n], 1.0, fuzz_signal.clamp(0.0, 1.0), 1.0, 0.0);
        self.pos = 0;
    }
}

#[derive(Clone)]
pub struct SwarmVoice {
    oscs:    [An<WaveSynth<U1>>; NUM_OSCS],
    phasers: [Phaser; NUM_OSCS],
    angle:   [f32; NUM_OSCS], // running orbit phase per oscillator, radians

    origin_freq: f32, // chased origin, Hz -- see ThumpMod::tick's own doc for why thump applies after

    chain_l: ChannelChain,
    chain_r: ChannelChain,

    pub chase_factor: Shared,
    pub radius:       Shared,
    pub orbit_speed:  Shared,
    pub phaser_depth: Shared,
    pub xover_freq:   Shared,
    pub nam_lo: NamModelCycler,
    pub nam_hi: NamModelCycler,

    sample_rate: f32,

    thump: ThumpMod,
    thump_signal:  f32,
    filter_signal: f32,
    fuzz_signal:   f32,
    width_signal:  f32,
}

// Read-only-from-outside view onto SwarmVoice's Shared cells -- see
// GrowlView's doc in growl.rs for why this exists. nam_lo/nam_hi are
// NamModelCycler, already a cheap-clone Shared+Arc<Vec<String>> bundle.
#[derive(Clone)]
pub struct SwarmView {
    pub chase_factor: Shared,
    pub radius:       Shared,
    pub orbit_speed:  Shared,
    pub phaser_depth: Shared,
    pub xover_freq:   Shared,
    pub nam_lo: NamModelCycler,
    pub nam_hi: NamModelCycler,
}

impl SwarmVoice {
    pub fn view (&self) -> SwarmView {
        SwarmView {
            chase_factor: self.chase_factor.clone(),
            radius:       self.radius.clone(),
            orbit_speed:  self.orbit_speed.clone(),
            phaser_depth: self.phaser_depth.clone(),
            xover_freq:   self.xover_freq.clone(),
            nam_lo: self.nam_lo.clone(),
            nam_hi: self.nam_hi.clone(),
        }
    }

    pub fn new (
        nam_models: Vec<Option<NamModelSlot>>,
        nam_names: Arc<Vec<String>>,
        thump_trigger: Shared, thump_peak: Shared, thump_decay: Shared,
    ) -> SwarmVoice {
        let oscs: [An<WaveSynth<U1>>; NUM_OSCS] = std::array::from_fn(|i| match OSC_SHAPES[i] {
            OscShape::Tri    => triangle(),
            OscShape::Saw    => saw(),
            OscShape::Square => square(),
        });
        let angle: [f32; NUM_OSCS] = std::array::from_fn(|i| i as f32 * TAU / NUM_OSCS as f32);

        let lo_index = model_index_by_name(&nam_names, "wetbass") as f32;
        let hi_index = model_index_by_name(&nam_names, "sansamp") as f32;
        let nam_lo = NamModelCycler::new(shared(lo_index), nam_names.clone());
        let nam_hi = NamModelCycler::new(shared(hi_index), nam_names);

        SwarmVoice {
            oscs,
            phasers: std::array::from_fn(|_| Phaser::new()),
            angle,
            origin_freq: 110.0,
            chain_l: ChannelChain::new(nam_models.clone(), nam_lo.shared(), nam_hi.shared()),
            chain_r: ChannelChain::new(nam_models, nam_lo.shared(), nam_hi.shared()),

            chase_factor: shared(DEFAULT_CHASE_FACTOR),
            radius:       shared(DEFAULT_RADIUS),
            orbit_speed:  shared(DEFAULT_ORBIT_SPEED),
            phaser_depth: shared(DEFAULT_PHASER_DEPTH),
            xover_freq:   shared(DEFAULT_XOVER_FREQ),
            nam_lo, nam_hi,

            sample_rate: DEFAULT_SR as f32,
            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            thump_signal: 0.0, filter_signal: 0.0, fuzz_signal: 0.0, width_signal: 0.0,
        }
    }
}

impl AudioNode for SwarmVoice {
    const ID: u64 = 0x7A_50;
    type Inputs = U2;
    type Outputs = U2;

    fn tick (&mut self, input: &Frame<f32, U2>) -> Frame<f32, U2> {
        let freq     = input[0];
        let selected = input[1] as usize;
        if selected != Self::INDEX { return Frame::from([0.0, 0.0]); }

        let chase_factor = self.chase_factor.value().clamp(0.0, 0.999_999);
        self.origin_freq = lerp(self.origin_freq, freq, chase_factor);
        let origin_freq = self.origin_freq * self.thump.tick(self.thump_signal);

        let width_signal = self.width_signal.clamp(0.0, 1.0);
        let radius       = self.radius.value().max(0.0)      * (1.0 + width_signal);
        let orbit_speed  = self.orbit_speed.value()          * (1.0 + width_signal);
        let phaser_depth = (self.phaser_depth.value() + width_signal + self.fuzz_signal).clamp(0.0, 1.0);

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for k in 0..NUM_OSCS {
            self.angle[k] = (self.angle[k] + orbit_speed * TAU / self.sample_rate).rem_euclid(TAU);

            let position = Complex32::new(origin_freq, 0.0) + Complex32::from_polar(radius, self.angle[k]);
            let osc_freq = position.re.max(MIN_OSC_FREQ);
            let pan      = (position.im / PAN_NORM_HZ).clamp(-1.0, 1.0);

            let dry = self.oscs[k].filter_mono(osc_freq);
            let phaser_rate = (osc_freq / 100.0).clamp(0.05, 8.0);
            let wet = self.phasers[k].tick(dry, phaser_rate, phaser_depth);

            // Equal-power pan, same law as fundsp's own panner().
            let angle_pan = (pan * 0.5 + 0.5) * (PI * 0.5);
            mix_l += wet * angle_pan.cos();
            mix_r += wet * angle_pan.sin();
        }

        let norm = 1.0 / (NUM_OSCS as f32).sqrt();
        mix_l *= norm;
        mix_r *= norm;

        let filter_cutoff = self.filter_signal.clamp(0.0, 1.0);
        let xover_hz  = self.xover_freq.value().max(1.0);

        let out_l = self.chain_l.tick(mix_l, self.sample_rate, filter_cutoff, xover_hz);
        let out_r = self.chain_r.tick(mix_r, self.sample_rate, filter_cutoff, xover_hz);

        Frame::from([out_l, out_r])
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        for osc in self.oscs.iter_mut() { osc.set_sample_rate(sample_rate); }
        for phaser in self.phasers.iter_mut() { phaser.set_sample_rate(sample_rate); }
        self.chain_l.set_sample_rate(sample_rate);
        self.chain_r.set_sample_rate(sample_rate);
        self.thump.set_sample_rate(sample_rate);
    }
}

impl Voice for SwarmVoice {
    const INDEX: usize = 3;
    fn name (&self) -> &'static str { "Swarm" }

    fn set_signal (&mut self, _bend: f32, filter: f32, fuzz: f32, width: f32, thump: f32) {
        self.thump_signal  = thump;
        self.filter_signal = filter;
        self.fuzz_signal   = fuzz;
        self.width_signal  = width;
    }

    fn on_block_start (&mut self, block_len: usize) {
        self.chain_l.on_block_start(block_len, self.fuzz_signal);
        self.chain_r.on_block_start(block_len, self.fuzz_signal);
    }

    // CC 50-54, 0..1 normalized input scaled to each param's own range.
    fn apply_cc (&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            50 => self.chase_factor.set_value(0.5 + value * 0.5),
            51 => self.radius.set_value(value * 200.0),
            52 => self.orbit_speed.set_value(value * 2.0),
            53 => self.phaser_depth.set_value(value),
            54 => self.xover_freq.set_value(value * 2000.0),
            _ => {},
        }
    }
}
