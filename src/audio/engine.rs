
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use fundsp::prelude64::*;

use super::signal::SharedSignal;
use super::voice::Voice;
use super::growl::GrowlVoice;
use super::swarm::SwarmVoice;
use super::reese::ReeseVoice;
use super::basic::BasicVoice;
use super::cc_input::CcInput;
use super::nam;

pub use super::growl::GrowlView;
pub use super::reese::ReeseView;
pub use super::basic::BasicView;
pub use super::swarm::SwarmView;

use super::nam::NAM_BLOCK_CAP;
use super::{ENVELOPE_ATTACK, ENVELOPE_RELEASE};

pub const VOICE_COUNT: usize = 4;

const LIMITER_ATTACK:  f32 = 0.003;
const LIMITER_RELEASE: f32 = 0.1;


//
// Audio Engine
//

pub struct Engine {
    pub signal: SharedSignal,
    pub freq: Shared,
    pub gate: Shared,

    pub main_sub_tri: An<WaveSynth<U1>>,
    pub dry_sub:      An<Sine<f64>>,
    pub envelope:     Box<dyn AudioUnit>,

    // Homogeneous -- every concrete voice is boxed behind the same
    // object-safe Voice trait (see voice.rs), so this array needs no
    // per-concrete-type dispatch. Whether a given voice is selected is
    // decided here in Engine (tick_pre_nam), not inside the voice itself.
    pub voices: [Box<dyn Voice>; VOICE_COUNT],
    // Built once at construction, before the concrete voices above get
    // boxed -- Voice::view() isn't part of the trait (each voice has its
    // own concrete View type), so this is the only place that can still see
    // the concrete types.
    pub voice_views: (ReeseView, GrowlView, BasicView, SwarmView),
    pub voice_selected: Shared,

    pub voice_dirty: [Arc<AtomicBool>; VOICE_COUNT],

    pub main_sub_lvl:  Shared,
    pub dry_sub_lvl:   Shared,

    pub amp_l: nam::NamStage,
    pub amp_r: nam::NamStage,
    pub amp_bypass:    Shared,
    pub amp_boost:     Shared,
    pub amp_blend:     Shared,
    pub amp_crossover: Shared,

    pub reverb: Box<dyn AudioUnit>, // 2 in (L, R) / 2 out, built from reverb_stereo
    pub reverb_bypass: Shared,
    pub reverb_dry:    Shared,

    pub limiter: An<Limiter<U2>>,
    pub limiter_bypass: Shared,
    pub limiter_thresh: Shared,

    pub master_vol: Shared,

    pub cc_input: CcInput,
}

impl Engine {
    pub fn new (
        signal: SharedSignal,
        freq: Shared,
        gate: Shared,

        thump_trigger: Shared,
        thump_peak: Shared,
        thump_decay: Shared,

        voice_selected: Shared,

        main_sub_lvl: Shared,
        dry_sub_lvl: Shared,

        nam_models: Vec<Option<nam::NamModelSlot>>,
        nam_names: Arc<Vec<String>>,
        amp_model_l: nam::NamModelSlot,
        amp_model_r: nam::NamModelSlot,

        amp_bypass: Shared,
        amp_boost: Shared,
        amp_blend: Shared,
        amp_crossover: Shared,

        reverb_bypass: Shared,
        reverb_dry: Shared,
        reverb_decay: f32,
        reverb_damp: f32,
        reverb_size: f32,

        limiter_bypass: Shared,
        limiter_thresh: Shared,

        master_vol: Shared,

    ) -> Engine {
        // Each NamStage holds exactly one fixed model -- no Bypass slot, no cycling.
        let amp_l = nam::NamStage::new(vec![Some(amp_model_l)], shared(0.0));
        let amp_r = nam::NamStage::new(vec![Some(amp_model_r)], shared(0.0));

        let reese = ReeseVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone(), signal.clone());
        let growl = GrowlVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone(), signal.clone());
        let basic = BasicVoice::new(thump_trigger.clone(), thump_peak.clone(), thump_decay.clone(), signal.clone());
        let swarm = SwarmVoice::new(nam_models, nam_names, thump_trigger.clone(), thump_peak.clone(), thump_decay.clone(), signal.clone());
        let voice_views = (reese.view(), growl.view(), basic.view(), swarm.view());

        Engine {
            freq, gate,

            signal: signal.clone(),

            main_sub_tri: triangle(),
            dry_sub:      sine(),
            envelope: Box::new(adsr_live(ENVELOPE_ATTACK, 0.0, 1.0, ENVELOPE_RELEASE)),
            voice_selected,

            // New voice: add it here (in Voice::index() order).
            voices: [Box::new(reese), Box::new(growl), Box::new(basic), Box::new(swarm)],
            voice_views,
            voice_dirty: std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),

            main_sub_lvl,
            dry_sub_lvl,

            amp_l,
            amp_r,
            amp_bypass,
            amp_boost,
            amp_blend,
            amp_crossover,

            reverb: Box::new(reverb_stereo(reverb_size, reverb_decay, reverb_damp)), reverb_bypass, reverb_dry,

            limiter: limiter_stereo(LIMITER_ATTACK, LIMITER_RELEASE), limiter_bypass, limiter_thresh,

            master_vol,

            cc_input: CcInput::connect(),
        }
    }

    pub fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub_tri.set_sample_rate(sr);
        self.dry_sub.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        for voice in self.voices.iter_mut() { voice.set_sample_rate(sr); }
        self.amp_l.set_sample_rate(sr);
        self.amp_r.set_sample_rate(sr);
        self.reverb.set_sample_rate(sr);
        self.limiter.set_sample_rate(sr);
    }

    // Each voice's own name, indexed by Voice::index() -- the UI reads the
    // currently-selected one straight off Handles rather than Zgicabra
    // tracking a separate copy of "which voice is this".
    pub fn voice_names (&self) -> [&'static str; VOICE_COUNT] {
        std::array::from_fn(|i| self.voices[i].name())
    }

    // Everything before the amp stage, gated by the envelope. Returns
    // (dry_l, dry_r, dry_sub) -- split out of a single tick() so build_stream
    // can batch dry_l/dry_r across a block and run the amp stage once per
    // block instead of once per sample (see run_nam / NamStage::process_block).
    // Every voice holds its own clone of the same SharedSignal (see
    // signal.rs) and reads the fields it cares about straight off it in
    // render() -- nothing here needs to snapshot or broadcast it.
    pub fn tick_pre_nam (&mut self) -> (f32, f32, f32) {
        let bend_mult = 2f32.powf(self.signal.bend.value());
        let base_freq = self.freq.value() * bend_mult;

        // Only the selected voice actually renders -- every Voice::tick()
        // always renders unconditionally, so gating who gets ticked (vs.
        // just told on_silence()) is this loop's job, not the voice's.
        let sel = self.voice_selected.value() as usize;
        let mut voice_l = 0.0;
        let mut voice_r = 0.0;
        for voice in self.voices.iter_mut() {
            let (l, r) = if voice.index() == sel { voice.tick(base_freq) } else { voice.on_silence(); (0.0, 0.0) };
            voice_l += l;
            voice_r += r;
        }

        let main_sub = self.main_sub_tri.filter_mono(base_freq) * self.main_sub_lvl.value();

        let env = self.envelope.filter_mono(self.gate.value());

        // dry_sub: one octave below base_freq, bypasses amp/reverb/limiter entirely.
        let dry_sub = self.dry_sub.filter_mono(base_freq * 0.5) * self.dry_sub_lvl.value() * env;

        let dry_l = (voice_l + main_sub) * env;
        let dry_r = (voice_r + main_sub) * env;

        (dry_l, dry_r, dry_sub)
    }

    // Knob-rate values, read once per block rather than per sample.
    fn nam_block_params (&self) -> (f32, f32, f32, f32) {
        let level = if self.amp_bypass.value() >= 1.0 { 0.0 } else { 1.0 };
        (level, self.amp_blend.value(), self.amp_boost.value(), self.amp_crossover.value())
    }

    pub fn run_nam (&mut self, block_l: &mut [f32], block_r: &mut [f32]) {
        let (level, blend, boost, crossover_hz) = self.nam_block_params();
        self.amp_l.process_block(block_l, level, blend, boost, crossover_hz);
        self.amp_r.process_block(block_r, level, blend, boost, crossover_hz);
    }

    // Pull frames from the NAM wet blocks
    pub fn tick_post_nam (&mut self, dry_l: f32, dry_r: f32, dry_sub: f32) -> (f32, f32) {

        let mut l = dry_l.tanh();
        let mut r = dry_r.tanh();

        // Global reverb
        if self.reverb_bypass.value() < 1.0 {
            let wet = self.reverb_dry.value().clamp(0.0, 1.0);
            let mut tail = [0.0f32; 2];
            self.reverb.tick(&[l, r], &mut tail);
            l = l + (tail[0] - l) * wet;
            r = r + (tail[1] - r) * wet;
        }

        // Safety limiter
        if self.limiter_bypass.value() < 1.0 {
            let (ll, rr) = self.limiter.filter_stereo(l, r);
            l = ll;
            r = rr;
        }

        // Master volume
        let vol = self.master_vol.value() * self.signal.level.value();
        (
            ((l + dry_sub) * vol).clamp(-1.0, 1.0),
            ((r + dry_sub) * vol).clamp(-1.0, 1.0),
        )
    }
}

