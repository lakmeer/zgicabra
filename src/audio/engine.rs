
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use fundsp::prelude64::*;

use super::signal::SharedSignal;
use super::voice::{Voice, KnobPickup};
use super::growl::GrowlVoice;
use super::swarm::SwarmVoice;
use super::reese::ReeseVoice;
use super::basic::BasicVoice;
use super::cc_input::CcInput;
use super::crusher::{crusher, LOW_MID_HZ, MID_HIGH_HZ};
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

// Engine's own CC-controllable knob list (see the same mechanism on Voice,
// in voice.rs's module doc) -- name/min/max, in the order CC7 selects them
// and CC8 sets their value. Hand-written (not macro-derived) since Engine
// is a single unique struct, not one of several concrete Voice types.
pub const ENGINE_KNOB_RANGES: &[(&str, f32, f32)] = &[
    ("master_vol",     0.0, 1.5),
    ("limiter_thresh", -24.0, 0.0),
    ("reverb_dry",     0.0, 1.0),
    ("main_sub_lvl",   0.0, 1.0),
    ("dry_sub_lvl",    0.0, 1.0),
];

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

    pub main_sub: An<Sine<f64>>,
    pub dry_sub:  An<Sine<f64>>,
    pub envelope: Box<dyn AudioUnit>,

    pub voices: [Box<dyn Voice>; VOICE_COUNT],
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

    pub selected_knob: Shared,
    knob_pickup: KnobPickup,
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

            main_sub: sine(),
            dry_sub:  sine(),
            envelope: Box::new(adsr_live(ENVELOPE_ATTACK, 0.0, 1.0, ENVELOPE_RELEASE)),
            voice_selected,

            // New voice: add it here (in Voice::index() order).
            voices: [Box::new(reese), Box::new(growl), Box::new(basic), Box::new(swarm)],
            voice_views,
            voice_dirty: std::array::from_fn(|_| Arc::new(AtomicBool::new(false))),

            main_sub_lvl,
            dry_sub_lvl,

            crusher_l: Box::new(crusher(&shared(MASTER_CRUSH_DEPTH), shared(LOW_MID_HZ), shared(MID_HIGH_HZ), shared(0.0), shared(0.0), shared(0.0))),
            crusher_r: Box::new(crusher(&shared(MASTER_CRUSH_DEPTH), shared(LOW_MID_HZ), shared(MID_HIGH_HZ), shared(0.0), shared(0.0), shared(0.0))),

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

            selected_knob: shared(0.0),
            knob_pickup: KnobPickup::new(),
        }
    }

    // Engine's own knob list -- see ENGINE_KNOB_RANGES. Mirrors the Voice
    // knob_count/knob_name/knob_range/knob_value/set_knob_value/
    // selected_knob/set_selected_knob surface, hand-written here since
    // Engine isn't a #[derive(Voice)] type.
    pub fn knob_count (&self) -> usize { ENGINE_KNOB_RANGES.len() }

    pub fn knob_name (&self, index: usize) -> &'static str {
        ENGINE_KNOB_RANGES.get(index).map(|k| k.0).unwrap_or("")
    }

    pub fn knob_range (&self, index: usize) -> (f32, f32) {
        ENGINE_KNOB_RANGES.get(index).map(|k| (k.1, k.2)).unwrap_or((0.0, 1.0))
    }

    pub fn knob_value (&self, index: usize) -> f32 {
        match index {
            0 => self.master_vol.value(),
            1 => self.limiter_thresh.value(),
            2 => self.reverb_dry.value(),
            3 => self.main_sub_lvl.value(),
            4 => self.dry_sub_lvl.value(),
            _ => 0.0,
        }
    }

    pub fn selected_knob (&self) -> usize {
        std::cmp::min(self.selected_knob.value().floor() as usize, self.knob_count().saturating_sub(1))
    }

    pub fn set_selected_knob (&mut self, index: usize) {
        let index = std::cmp::min(index, self.knob_count().saturating_sub(1));
        self.selected_knob.set_value(index as f32);
        self.knob_pickup.reset();
    }

    pub fn set_knob_value (&mut self, index: usize, raw: f32) {
        let raw = raw.clamp(0.0, 1.0);
        let (min, max) = self.knob_range(index);
        let current = self.knob_value(index);
        let target_norm = if max > min { (current - min) / (max - min) } else { 0.0 };
        let Some(raw) = self.knob_pickup.update(raw, target_norm) else { return; };
        let value = min + raw * (max - min);
        match index {
            0 => self.master_vol.set_value(value),
            1 => self.limiter_thresh.set_value(value),
            2 => self.reverb_dry.set_value(value),
            3 => self.main_sub_lvl.set_value(value),
            4 => self.dry_sub_lvl.set_value(value),
            _ => {},
        }
    }

    pub fn set_sample_rate (&mut self, sr: f64) {
        self.main_sub.set_sample_rate(sr);
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
        let main_sub = self.main_sub.filter_mono(base_freq) * self.main_sub_lvl.value() * env;
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
