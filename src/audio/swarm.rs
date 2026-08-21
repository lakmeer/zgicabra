
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

use zgicabra_voice_macro::Voice;

use crate::tools::linexp;
use crate::zgicabra::SignalState;
use super::voice::{Voice, VoiceDsp, ThumpMod};
use super::nam::{NamModelCycler, NamModelSlot};
use super::nam_graph::nam_mid_side;
use super::nam_node::NAM_WINDOW;
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
// TODO: Map to signal width

const PAN_NORM_HZ:          f32 = 20.0;
const DEFAULT_CHASE_FACTOR: f32 = 0.99;
const DEFAULT_RADIUS:       f32 = 90.0; // cents -- orbit radius on the freq axis, converted to Hz per-tick relative to the current origin frequency so the detune width stays perceptually consistent across pitch (see radius_hz in tick)
const DEFAULT_ORBIT_SPEED:  f32 = 2.25; // Hz -- rotations per second
const DEFAULT_PHASER_DEPTH: f32 = 0.4;
const DEFAULT_XOVER_FREQ:   f32 = 400.0; // Hz, splits the swarm mix before the two NAM stages

fn model_index_by_name (names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(0)
}

// One output channel's per-sample stages, split around the NAM graph:
// `pre` is everything upstream of it (the limiter), `post` everything
// downstream (moog filter -> crusher). The crossover, mid/side collapse and
// inference that used to sit between them -- along with the one-block
// latency ring that made a block kernel reachable from a per-sample tick --
// are all nam_graph::nam_mid_side now.
#[derive(Clone)]
struct ChannelChain {
    limiter: Compressor,
    moog:    MoogFilterFx,
    crusher: Crusher,
}

impl ChannelChain {
    fn new () -> ChannelChain {
        ChannelChain {
            limiter: Compressor::new(),
            moog:    MoogFilterFx::new(),
            // Swarm has no compressor panel, so the meter cells go nowhere.
            crusher: Crusher::new(
                CRUSH_RATIO_DOWN, CRUSH_THRESHOLD_UP, CRUSH_RATIO_UP, CRUSH_RELEASE, CRUSH_MIX,
                shared(0.0), shared(0.0), shared(0.0),
            ),
        }
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        self.limiter.set_sample_rate(sample_rate);
        self.moog.set_sample_rate(sample_rate);
        self.crusher.set_sample_rate(sample_rate);
    }

    // Squash hard into the crossover -- see LIMITER_THRESH_DB.
    fn pre (&mut self, x: f32) -> f32 {
        self.limiter.tick(x, x, LIMITER_THRESH_DB).0
    }

    fn post (&mut self, x: f32, filter_cutoff: f32) -> f32 {
        let filtered = self.moog.tick(x, filter_cutoff, MOOG_RESONANCE);
        self.crusher.tick(filtered, CRUSH_THRESH_DOWN, CRUSH_ATTACK, CRUSH_DEPTH, CRUSH_MAKEUP_DB)
    }
}

#[derive(Clone, Voice)]
#[voice(index = 3, id = 0x7A_50, label = "Swarm", new = manual, thump = manual)]
pub struct SwarmVoice {
    #[node(each)] oscs:    [An<WaveSynth<U1>>; NUM_OSCS],
    #[node(each)] phasers: [Phaser; NUM_OSCS],
    angle:   [f32; NUM_OSCS], // running orbit phase per oscillator, radians

    origin_freq: f32, // chased origin, Hz -- thump = manual: applied to this, not the raw freq

    #[node] chain_l: ChannelChain,
    #[node] chain_r: ChannelChain,

    // The whole mid/side NAM stage: crossover, mid/side collapse, one model
    // per band on the mid signal, rebuild, band sum. Boxed because the
    // combinator type is unnameable and impl Trait can't be a field type;
    // Box<dyn AudioUnit> is Clone via dyn_clone, so #[derive(Clone)] still
    // works. See nam_graph.rs.
    #[node] nam: Box<dyn AudioUnit>,
    // Dry/wet, written from sig.fuzz each sample -- the graph reads it.
    nam_blend: Shared,

    #[input(cc = "1", range = 0.5..1.0,    set = |v| 0.5 + v * 0.5)] pub chase_factor_input: Shared,
    #[input(cc = "2", range = 0.0..200.0,  set = |v| v * 200.0)]     pub radius_input:       Shared,
    #[input(cc = "3", range = 0.0..2.0,    set = |v| v * 2.0)]       pub orbit_speed_input:  Shared,
    #[input(cc = "4", range = 0.0..1.0,    set = |v| v)]             pub phaser_depth_input: Shared,
    #[input(cc = "5", range = 0.0..2000.0, set = |v| v * 2000.0)]    pub xover_freq_input:   Shared,
    #[view] pub nam_lo: NamModelCycler,
    #[view] pub nam_hi: NamModelCycler,

    #[live(range = 0.0..200.0)] pub radius_live:       Shared,
    #[live(range = 0.0..2.0)]   pub orbit_speed_live:  Shared,
    #[live(range = 0.0..1.0)]   pub phaser_depth_live: Shared,

    // Post-blend peak of each band, written by monitor() nodes inside the
    // NAM graph rather than by hand here.
    #[live(range = 0.0..1.0)] pub nam_lo_live: Shared,
    #[live(range = 0.0..1.0)] pub nam_hi_live: Shared,

    // Live per-oscillator freq/pan, written every tick -- read-only from the
    // UI side for the swarm scope (see ui.rs's draw_swarm_panel). Arrays, so
    // #[view] (passthrough) rather than #[live].
    #[view] pub osc_freq_live: [Shared; NUM_OSCS],
    #[view] pub osc_pan_live:  [Shared; NUM_OSCS],
    #[live(range = 0.0..2000.0)] pub origin_live: Shared,

    sample_rate: f32,

    thump: ThumpMod,
    sig:   SignalState,
}

// SwarmView + view()/fields()/apply()/UI_RANGES + the AudioNode/Voice impls
// are generated by #[derive(Voice)]. new() stays hand-written (new = manual):
// it takes the NAM model list, resolves default model indices by name, and
// sizes the mid/side scratch buffers before the struct literal.
impl SwarmVoice {
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

        let lo_index = model_index_by_name(&nam_names, "wetbass");
        let hi_index = model_index_by_name(&nam_names, "sansamp");
        let nam_lo = NamModelCycler::new(shared(lo_index as f32), nam_names.clone());
        let nam_hi = NamModelCycler::new(shared(hi_index as f32), nam_names);

        // Each band gets its own model instance, so neither band's WaveNet
        // dilation state can bleed into the other's even if both cyclers land
        // on the same model -- which the old shared Vec<Arc<Model>> allowed.
        //
        // Note the tradeoff: the model choice is now baked into the graph at
        // construction, where NamStage read it from a Shared every block.
        // Nothing actually drove that Shared (NamModelCycler::cycle has no
        // caller -- see nam.rs), so this loses no working behaviour, but
        // making model choice live again would mean Net::crossfade rather than
        // a Shared write. That is the cost of expressing the path as a graph.
        let nam_blend        = shared(0.0);
        let xover_freq_input = shared(DEFAULT_XOVER_FREQ);
        let nam_lo_live = shared(0.0);
        let nam_hi_live = shared(0.0);
        let slot_lo = nam_models.get(lo_index).and_then(Option::as_ref)
            .expect("swarm lo NAM model slot");
        let slot_hi = nam_models.get(hi_index).and_then(Option::as_ref)
            .expect("swarm hi NAM model slot");
        let nam = Box::new(nam_mid_side(
            slot_lo, slot_hi,
            &nam_blend, &xover_freq_input,
            &nam_lo_live, &nam_hi_live,
            NAM_WINDOW,
        ));

        SwarmVoice {
            oscs,
            phasers: std::array::from_fn(|_| Phaser::new()),
            angle,
            origin_freq: 110.0,
            chain_l: ChannelChain::new(),
            chain_r: ChannelChain::new(),

            nam,
            nam_blend,

            chase_factor_input: shared(DEFAULT_CHASE_FACTOR),
            radius_input:       shared(DEFAULT_RADIUS),
            orbit_speed_input:  shared(DEFAULT_ORBIT_SPEED),
            phaser_depth_input: shared(DEFAULT_PHASER_DEPTH),
            xover_freq_input,
            nam_lo, nam_hi,

            radius_live:       shared(0.0),
            orbit_speed_live:  shared(0.0),
            phaser_depth_live: shared(0.0),
            nam_lo_live,
            nam_hi_live,

            osc_freq_live: std::array::from_fn(|_| shared(0.0)),
            osc_pan_live:  std::array::from_fn(|_| shared(0.0)),
            origin_live:   shared(110.0),

            sample_rate: DEFAULT_SR as f32,
            thump: ThumpMod::new(thump_trigger, thump_peak, thump_decay),
            sig:   SignalState::new(),
        }
    }
}

impl VoiceDsp for SwarmVoice {
    // thump = manual: SwarmVoice chases an origin freq from the raw input,
    // then applies thump to that origin (not the incoming freq), so it does
    // its own thump.tick here -- the generated tick hands over the raw freq.
    fn render (&mut self, freq: f32, _thump_mult: f32) -> Frame<f32, U2> {
        let chase_factor = self.chase_factor_input.value().clamp(0.0, 0.999_999);
        self.origin_freq = lerp(self.origin_freq, freq, chase_factor);
        let origin_freq = self.origin_freq * self.thump.tick(self.sig.thump);

        let width_signal = self.sig.width.clamp(0.0, 1.0);
        let radius_cents = self.radius_input.value().max(0.0) * (1.0 + width_signal);
        let orbit_speed  = self.orbit_speed_input.value()     * (1.0 + width_signal);
        let phaser_depth = (self.phaser_depth_input.value() + width_signal + self.sig.fuzz).clamp(0.0, 1.0);
        self.radius_live.set_value(radius_cents);
        self.orbit_speed_live.set_value(orbit_speed);
        self.phaser_depth_live.set_value(phaser_depth);

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

            self.osc_freq_live[k].set_value(osc_freq);
            self.osc_pan_live[k].set_value(pan);

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

        let filter_cutoff = self.sig.filter.clamp(0.0, 1.0);

        // The graph reads blend and crossover cutoff from Shared cells, so
        // the only thing to hand it is audio. Crossover cutoff is already a
        // Shared (xover_freq_input); blend has to be published from the
        // per-block performance signal.
        self.nam_blend.set_value(self.sig.fuzz.clamp(0.0, 1.0));

        let mut namd = [0.0f32; 2];
        self.nam.tick(&[self.chain_l.pre(mix_l), self.chain_r.pre(mix_r)], &mut namd);

        Frame::from([
            self.chain_l.post(namd[0], filter_cutoff),
            self.chain_r.post(namd[1], filter_cutoff),
        ])
    }

    fn on_set_sample_rate (&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
    }

}
