
//
// Swarm -- 5 oscillators (2 tri, 2 saw, 1 square), each orbiting a shared
// origin point in a 2D plane where x = frequency (Hz) offset and y = pan
// (see num_complex usage below). The origin itself chases the input note's
// frequency with a fast-but-imperfect lerp, so the whole swarm glides
// rather than snapping. Each oscillator's orbit position feeds its own
// frequency offset, stereo pan, and its own hand-rolled phaser (rate
// tracks that oscillator's own orbiting frequency -- see Phaser below).
//
// The summed swarm splits into two per-channel chains (limiter ->
// crossover -> moog filter -> crusher), one per output channel. The NAM
// stage in between is mid/side, not per-channel: L/R low bands collapse to
// mid_lo/side_lo (same for high), one NamStage runs on each of mid_lo and
// mid_hi, and L/R are rebuilt as mid+side / mid-side afterward. This halves
// the WaveNet inference cost (2 model runs/block instead of 4) and, unlike
// giving each channel its own NamStage pointed at the same underlying
// model, can't let one channel's dilation state bleed into the other's --
// there's only one instance of each selected model, and it only ever sees
// one (merged) signal. Side channels (side_lo/side_hi) stay dry, which is
// where any stereo width the model would have added is traded away.
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

// 0..1, no GUI knob specified for this. MoogFilterFx (filter.rs) remaps this
// into a raw Moog Q of 0.1..4.0 -- simulating that ladder's own feedback
// equation shows it self-oscillates (sustained output with silent input)
// once raw Q crosses ~0.6-1.0 depending on cutoff, i.e. past input~0.13-0.23.
// 0.3 here maps to raw Q 1.27, comfortably past that onset at every cutoff,
// which is the whine -- not the filter reacting to the swarm, oscillating
// on its own. Kept well under the onset margin instead.
const MOOG_RESONANCE: f32 = 0.08;

const PAN_NORM_HZ:          f32 = 20.0;
const DEFAULT_CHASE_FACTOR: f32 = 0.99;
const DEFAULT_RADIUS:       f32 = 90.0; // cents -- orbit radius on the freq axis, converted to Hz per-tick relative to the current origin frequency so the detune width stays perceptually consistent across pitch (see radius_hz in tick)
const DEFAULT_ORBIT_SPEED:  f32 = 2.25; // Hz -- rotations per second
const DEFAULT_PHASER_DEPTH: f32 = 0.4;
const DEFAULT_XOVER_FREQ:   f32 = 400.0; // Hz, splits the swarm mix before the two NAM stages

fn model_index_by_name (names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(0)
}

// One output channel's post-swarm chain: limiter -> xover -> moog filter ->
// crusher. NAM inference happens once per block, mid/side across both
// channels (see SwarmVoice::on_block_start) -- this struct only owns the
// per-sample stages plus the one-block-latency raw/wet scratch ring that
// the swarm-level NAM pass reads and rewrites in place between blocks.
#[derive(Clone)]
struct ChannelChain {
    limiter:  Compressor,
    xover_lp: f32,
    moog:     MoogFilterFx,
    crusher:  Crusher,
    raw_lo:   Vec<f32>,
    raw_hi:   Vec<f32>,
    pos:      usize,
}

impl ChannelChain {
    fn new () -> ChannelChain {
        ChannelChain {
            limiter: Compressor::new(),
            xover_lp: 0.0,
            moog:    MoogFilterFx::new(),
            crusher: Crusher::new(CRUSH_RATIO_DOWN, CRUSH_THRESHOLD_UP, CRUSH_RATIO_UP, CRUSH_RELEASE, CRUSH_MIX),
            raw_lo: vec![0.0; NAM_BLOCK_CAP], raw_hi: vec![0.0; NAM_BLOCK_CAP],
            pos: 0,
        }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.limiter.set_sample_rate(sample_rate);
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

    fn on_block_start (&mut self) {
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

    // Mid/side NAM stages -- one instance per band, shared across both
    // channels (see file doc comment). mid_lo/side_lo/mid_hi/side_hi are
    // scratch for the merge/split around them, sized once at construction,
    // never reallocated on the audio thread.
    nam_lo_stage: NamStage,
    nam_hi_stage: NamStage,
    mid_lo:  Vec<f32>,
    side_lo: Vec<f32>,
    mid_hi:  Vec<f32>,
    side_hi: Vec<f32>,

    pub chase_factor: Shared,
    pub radius:       Shared,
    pub orbit_speed:  Shared,
    pub phaser_depth: Shared,
    pub xover_freq:   Shared,
    pub nam_lo: NamModelCycler,
    pub nam_hi: NamModelCycler,

    // Live per-oscillator freq/pan, written every tick -- read-only from the
    // UI side for the swarm scope (see ui.rs's draw_swarm_panel).
    pub osc_freq:    [Shared; NUM_OSCS],
    pub osc_pan:     [Shared; NUM_OSCS],
    pub origin_live: Shared,

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

    pub osc_freq:    [Shared; NUM_OSCS],
    pub osc_pan:     [Shared; NUM_OSCS],
    pub origin_live: Shared,
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
            osc_freq:    self.osc_freq.clone(),
            osc_pan:     self.osc_pan.clone(),
            origin_live: self.origin_live.clone(),
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
            chain_l: ChannelChain::new(),
            chain_r: ChannelChain::new(),

            nam_lo_stage: NamStage::new(nam_models.clone(), nam_lo.shared()),
            nam_hi_stage: NamStage::new(nam_models, nam_hi.shared()),
            mid_lo:  vec![0.0; NAM_BLOCK_CAP],
            side_lo: vec![0.0; NAM_BLOCK_CAP],
            mid_hi:  vec![0.0; NAM_BLOCK_CAP],
            side_hi: vec![0.0; NAM_BLOCK_CAP],

            chase_factor: shared(DEFAULT_CHASE_FACTOR),
            radius:       shared(DEFAULT_RADIUS),
            orbit_speed:  shared(DEFAULT_ORBIT_SPEED),
            phaser_depth: shared(DEFAULT_PHASER_DEPTH),
            xover_freq:   shared(DEFAULT_XOVER_FREQ),
            nam_lo, nam_hi,

            osc_freq:    std::array::from_fn(|_| shared(0.0)),
            osc_pan:     std::array::from_fn(|_| shared(0.0)),
            origin_live: shared(110.0),

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
        let radius_cents = self.radius.value().max(0.0)      * (1.0 + width_signal);
        let orbit_speed  = self.orbit_speed.value()          * (1.0 + width_signal);
        let phaser_depth = (self.phaser_depth.value() + width_signal + self.fuzz_signal).clamp(0.0, 1.0);

        // cents -> Hz radius against the current origin, so a fixed cents
        // width reads the same at any pitch instead of shrinking as origin
        // rises (which is what a fixed-Hz radius did).
        let radius_hz = origin_freq * (2.0f32.powf(radius_cents / 1200.0) - 1.0);

        self.origin_live.set_value(origin_freq);

        let mut mix_l = 0.0f32;
        let mut mix_r = 0.0f32;

        for k in 0..NUM_OSCS {
            self.angle[k] = (self.angle[k] + orbit_speed * TAU / self.sample_rate).rem_euclid(TAU);

            let position = Complex32::new(origin_freq, 0.0) + Complex32::from_polar(radius_hz, self.angle[k]);
            let osc_freq = position.re.max(MIN_OSC_FREQ);
            let pan      = (position.im / PAN_NORM_HZ).clamp(-1.0, 1.0);

            self.osc_freq[k].set_value(osc_freq);
            self.osc_pan[k].set_value(pan);

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
        self.nam_lo_stage.set_sample_rate(sample_rate);
        self.nam_hi_stage.set_sample_rate(sample_rate);
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

    // Merge last block's per-channel low/high dry buffers to mid/side, run
    // one NAM instance per band on the mid signal only (side stays dry),
    // then rebuild L/R in place before this block's tick() calls start
    // reading them as wet. See file doc comment for why mid/side instead of
    // one NamStage per channel.
    fn on_block_start (&mut self, block_len: usize) {
        let n = std::cmp::min(block_len, NAM_BLOCK_CAP);

        for i in 0..n {
            let (l, r) = (self.chain_l.raw_lo[i], self.chain_r.raw_lo[i]);
            self.mid_lo[i]  = (l + r) * 0.5;
            self.side_lo[i] = (l - r) * 0.5;

            let (l, r) = (self.chain_l.raw_hi[i], self.chain_r.raw_hi[i]);
            self.mid_hi[i]  = (l + r) * 0.5;
            self.side_hi[i] = (l - r) * 0.5;
        }

        let blend = self.fuzz_signal.clamp(0.0, 1.0);
        self.nam_lo_stage.process_block(&mut self.mid_lo[..n], 1.0, blend, 1.0, 0.0);
        self.nam_hi_stage.process_block(&mut self.mid_hi[..n], 1.0, blend, 1.0, 0.0);

        for i in 0..n {
            self.chain_l.raw_lo[i] = self.mid_lo[i] + self.side_lo[i];
            self.chain_r.raw_lo[i] = self.mid_lo[i] - self.side_lo[i];
            self.chain_l.raw_hi[i] = self.mid_hi[i] + self.side_hi[i];
            self.chain_r.raw_hi[i] = self.mid_hi[i] - self.side_hi[i];
        }

        self.chain_l.on_block_start();
        self.chain_r.on_block_start();
    }

    // CC 50-54, 0..1 normalized input scaled to each param's own range.
    // CC2-6 is the same set of knobs (CC1 being reserved for the global Mod
    // Wheel -> filter mapping, see hydra/midi.rs) so a controller with only
    // 8 physical knobs can still reach them live.
    fn apply_cc (&mut self, cc: u8, value: f32) {
        let value = value.clamp(0.0, 1.0);
        match cc {
            50 | 2 => self.chase_factor.set_value(0.5 + value * 0.5),
            51 | 3 => self.radius.set_value(value * 200.0),
            52 | 4 => self.orbit_speed.set_value(value * 2.0),
            53 | 5 => self.phaser_depth.set_value(value),
            54 | 6 => self.xover_freq.set_value(value * 2000.0),
            _ => {},
        }
    }
}
