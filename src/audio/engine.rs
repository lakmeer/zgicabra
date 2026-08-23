
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
use super::crusher::crusher;
use super::nam;

pub use super::growl::GrowlView;
pub use super::reese::ReeseView;
pub use super::basic::BasicView;
pub use super::swarm::SwarmView;

use super::{ENVELOPE_ATTACK, ENVELOPE_RELEASE};

pub const VOICE_COUNT: usize = 4;

const LIMITER_ATTACK:  f32 = 0.003;
const LIMITER_RELEASE: f32 = 0.1;

const MASTER_CRUSH_DEPTH: f32 = 1.0;

// Peak, matching nam_graph.rs's per-band monitors -- this feeds a level
// indicator, not a loudness readout. RMS runs a slower window alongside it
// for the same tap, giving the UI meter both a fast peak and a perceived
// loudness reading.
const OUT_METER_PEAK_SMOOTH_S: f64 = 0.05;
const OUT_METER_RMS_SMOOTH_S:  f64 = 0.2;


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
    // decided here in Engine (tick), not inside the voice itself.
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

    crusher_l: Box<dyn AudioUnit>,
    crusher_r: Box<dyn AudioUnit>,

    pub reverb: Box<dyn AudioUnit>, // 2 in (L, R) / 2 out, built from reverb_stereo
    pub reverb_bypass: Shared,
    pub reverb_dry:    Shared,

    pub limiter: An<Limiter<U2>>,
    pub limiter_bypass: Shared,
    pub limiter_thresh: Shared,

    pub master_vol: Shared,

    out_meter_peak_l: Box<dyn AudioUnit>,
    out_meter_peak_r: Box<dyn AudioUnit>,
    out_meter_rms_l:  Box<dyn AudioUnit>,
    out_meter_rms_r:  Box<dyn AudioUnit>,
    pub out_level_peak_l: Shared,
    pub out_level_peak_r: Shared,
    pub out_level_rms_l:  Shared,
    pub out_level_rms_r:  Shared,

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

        reverb_bypass: Shared,
        reverb_dry: Shared,
        reverb_decay: f32,
        reverb_damp: f32,
        reverb_size: f32,

        limiter_bypass: Shared,
        limiter_thresh: Shared,

        master_vol: Shared,

        out_level_peak_l: Shared,
        out_level_peak_r: Shared,
        out_level_rms_l:  Shared,
        out_level_rms_r:  Shared,

    ) -> Engine {
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

            crusher_l: Box::new(crusher(&shared(MASTER_CRUSH_DEPTH), shared(0.0), shared(0.0), shared(0.0))),
            crusher_r: Box::new(crusher(&shared(MASTER_CRUSH_DEPTH), shared(0.0), shared(0.0), shared(0.0))),

            reverb: Box::new(reverb_stereo(reverb_size, reverb_decay, reverb_damp)), reverb_bypass, reverb_dry,

            limiter: limiter_stereo(LIMITER_ATTACK, LIMITER_RELEASE), limiter_bypass, limiter_thresh,

            master_vol,

            out_meter_peak_l: Box::new(monitor(&out_level_peak_l, Meter::Peak(OUT_METER_PEAK_SMOOTH_S))),
            out_meter_peak_r: Box::new(monitor(&out_level_peak_r, Meter::Peak(OUT_METER_PEAK_SMOOTH_S))),
            out_meter_rms_l:  Box::new(monitor(&out_level_rms_l,  Meter::Rms(OUT_METER_RMS_SMOOTH_S))),
            out_meter_rms_r:  Box::new(monitor(&out_level_rms_r,  Meter::Rms(OUT_METER_RMS_SMOOTH_S))),
            out_level_peak_l,
            out_level_peak_r,
            out_level_rms_l,
            out_level_rms_r,

            cc_input: CcInput::connect(),
        }
    }

    pub fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub_tri.set_sample_rate(sr);
        self.dry_sub.set_sample_rate(sr);
        self.envelope.set_sample_rate(sr);
        for voice in self.voices.iter_mut() { voice.set_sample_rate(sr); }
        self.crusher_l.set_sample_rate(sr);
        self.crusher_r.set_sample_rate(sr);
        self.reverb.set_sample_rate(sr);
        self.limiter.set_sample_rate(sr);
        self.out_meter_peak_l.set_sample_rate(sr);
        self.out_meter_peak_r.set_sample_rate(sr);
        self.out_meter_rms_l.set_sample_rate(sr);
        self.out_meter_rms_r.set_sample_rate(sr);
    }

    // Each voice's own name, indexed by Voice::index() -- the UI reads the
    // currently-selected one straight off Handles rather than Zgicabra
    // tracking a separate copy of "which voice is this".
    pub fn voice_names (&self) -> [&'static str; VOICE_COUNT] {
        std::array::from_fn(|i| self.voices[i].name())
    }

    // Every voice holds its own clone of the same SharedSignal (see
    // signal.rs) and reads the fields it cares about straight off it in
    // render() -- nothing here needs to snapshot or broadcast it.
    pub fn tick (&mut self) -> (f32, f32) {
        let bend_mult = 2f32.powf(self.signal.bend.value());
        let base_freq = self.freq.value() * bend_mult;

        // Published before voices render, so a voice's own DSP (e.g.
        // ReeseVoice's crusher) can see this sample's envelope value --
        // Engine's own dry_l/dry_r multiply below happens too late for that.
        let env = self.envelope.filter_mono(self.gate.value());
        self.signal.env.set_value(env);

        let sel = self.voice_selected.value() as usize;
        let mut voice_l = 0.0;
        let mut voice_r = 0.0;
        // Each voice reads self.signal.env (published above) and gates its
        // own output by it in render() -- Engine no longer applies a blanket
        // env multiply here, so a voice can choose to leave part of its
        // signal ungated (ReeseVoice's feedback loop).
        for voice in self.voices.iter_mut() {
            let (l, r) = if voice.index() == sel { voice.tick(base_freq) } else { voice.on_silence(); (0.0, 0.0) };
            voice_l += l;
            voice_r += r;
        }

        // main_sub isn't a Voice -- still gated here directly, same as dry_sub.
        let main_sub = self.main_sub_tri.filter_mono(base_freq) * self.main_sub_lvl.value() * env;
        let dry_sub  = self.dry_sub.filter_mono(base_freq * 0.5) * self.dry_sub_lvl.value() * env;

        let dry_l = voice_l + main_sub;
        let dry_r = voice_r + main_sub;

        let mut l = dry_l.tanh();
        let mut r = dry_r.tanh();

        // Master crush, identical fixed params on both channels
        //let mut crushed_l = [0.0f32];
        //let mut crushed_r = [0.0f32];
        //self.crusher_l.tick(&[l], &mut crushed_l);
        //self.crusher_r.tick(&[r], &mut crushed_r);
        //l = crushed_l[0];
        //r = crushed_r[0];

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
        let out_l = ((l + dry_sub) * vol).clamp(-1.0, 1.0);
        let out_r = ((r + dry_sub) * vol).clamp(-1.0, 1.0);

        // Output meter -- taps the exact signal handed to cpal, post
        // limiter/volume/clamp, so it reads what's actually audible.
        let mut buf_l = [0.0f32];
        let mut buf_r = [0.0f32];
        self.out_meter_peak_l.tick(&[out_l], &mut buf_l);
        self.out_meter_peak_r.tick(&[out_r], &mut buf_r);
        self.out_meter_rms_l.tick(&[out_l], &mut buf_l);
        self.out_meter_rms_r.tick(&[out_r], &mut buf_r);

        (out_l, out_r)
    }
}
