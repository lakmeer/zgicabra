
//
// AuditionVoice: cycles through fundsp's Generators (see the doc comment
// atop mod.rs), a live-cutoff Noise generator, and this project's own
// custom voices (ReeseVoice/FmVoice/DsfVoice/StutterVoice) at runtime, for
// auditioning oscillator/voice character without wiring up a dedicated slot
// for each one. Self-contained fundsp AudioNode
// (1 in: base_freq, 1 out, unleveled -- caller multiplies by its own level
// param, same convention as the sub-oscillators). Every generator kind is
// built once at construction and kept warm; `AuditionCycler::cycle()` just
// swaps a `Shared` index, same pattern as NamModelCycler (see nam.rs) --
// picking a generator is a rare UI action, not a per-block cost worth
// optimizing further.
//
// Every generator's inputs beyond base_freq (pulse/poly_pulse's width,
// dsf_saw/dsf_square's roughness, the custom voices' own params) are read
// live from `params`, a fixed bank of AUDITION_PARAM_SLOTS Shared cells --
// see GENERATOR_PARAMS for which slots a given generator actually uses (the
// GUI reads that same table to label sliders, see gui.rs::draw_audition_
// params). Slots beyond a generator's arity are simply ignored; switching
// generators does not reset them, so a slot can carry a stale value over
// from whatever previously used it -- harmless for an audition tool.
//
// Skipped vs. the full Generators list: the _hz fixed-frequency variants
// (this voice is note-driven, so runtime frequency always wins), the
// _r/_bits parameterized duplicates (roughness/bit-count baked constants,
// redundant with the base fn's own default here), and multizero (a
// multichannel dupe of zero()).
//

use fundsp::prelude64::*;

use super::reese::ReeseVoice;
use super::fm::FmVoice;
use super::dsf::DsfVoice;
use super::stutter::StutterVoice;

pub const GENERATOR_NAMES: [&str; 19] = [
    "Bypass", "Saw", "Square", "Triangle", "Soft Saw",
    "Ramp", "Organ", "Hammond", "Pulse", "Poly Saw",
    "Poly Square", "Poly Pulse", "Dsf Saw", "Dsf Square",
    "Reese Voice", "Fm Voice", "Dsf Voice", "Noise", "Stutter",
];

// Extra-input labels per generator, beyond input 0 (always base_freq) --
// parallel to GENERATOR_NAMES. Drives both which of `params`'s slots a
// generator reads (tick() below) and the slider labels in the GUI.
pub const GENERATOR_PARAMS: [&[&str]; 19] = [
    &[],                    // Bypass
    &[],                    // Saw
    &[],                    // Square
    &[],                    // Triangle
    &[],                    // Soft Saw
    &[],                    // Ramp
    &[],                    // Organ
    &[],                    // Hammond
    &["width"],             // Pulse
    &[],                    // Poly Saw
    &[],                    // Poly Square
    &["width"],             // Poly Pulse
    &["roughness"],         // Dsf Saw
    &["roughness"],         // Dsf Square
    &["detune_cents_max"],  // Reese Voice
    &[
        "ratio_a", "ratio_b", "ratio_c", "index_b", "index_c", "detune_cents_max",
        "lfo_rate_a_hz", "lfo_depth_a_hz", "lfo_rate_b_hz", "lfo_depth_b_hz", "lfo_rate_c_hz", "lfo_depth_c_hz",
    ], // Fm Voice
    &["roughness", "cutoff_hz", "resonance", "lfo_rate_hz", "lfo_depth_hz", "drive"], // Dsf Voice
    &["cutoff_hz"],         // Noise -- white noise through a live lowpass
    &[],                    // Stutter
];

// Widest extra-input count above, rounded up -- the GUI always draws this
// many slider slots per audition voice, however many the selected generator
// actually uses (see draw_audition_params).
pub const AUDITION_PARAM_SLOTS: usize = 12;

// Every audition param shares this fixed range regardless of label/generator
// -- also the range `AuditionCycler::randomise` draws from, see gui.rs's
// draw_audition_params for the GUI-side rationale.
pub const AUDITION_PARAM_LO: f32 = 0.0;
pub const AUDITION_PARAM_HI: f32 = 10.0;

fn build_generators () -> Vec<Box<dyn AudioUnit>> {
    vec![
        Box::new(saw()),
        Box::new(square()),
        Box::new(triangle()),
        Box::new(soft_saw()),
        Box::new(ramp()),
        Box::new(organ()),
        Box::new(hammond()),
        Box::new(pulse()),
        Box::new(poly_saw()),
        Box::new(poly_square()),
        Box::new(poly_pulse()),
        Box::new(dsf_saw()),
        Box::new(dsf_square()),
        // frac=1.0: these voices scale their detune by an input param, so
        // an audition slot needs the full-strength per-voice fraction to
        // actually hear it move (see ReeseVoice/FmVoice's `frac` doc).
        Box::new(An(ReeseVoice::new(1.0))),
        Box::new(An(FmVoice::new(1.0))),
        Box::new(An(DsfVoice::new())),
        // Noise: white noise through a live lowpass. Input 0 (base_freq) is
        // discarded (sink()) rather than driving pitch -- noise has none --
        // input 1 (cutoff_hz) is the only thing that matters, same slot
        // convention as e.g. Dsf Saw's roughness.
        Box::new((white() | sink() | pass()) >> lowpass_q(1.0)),
        Box::new(An(StutterVoice::new())),
    ]
}

#[derive(Clone)]
pub struct AuditionVoice {
    selected: Shared,
    units:    Vec<Box<dyn AudioUnit>>,
    params:   [Shared; AUDITION_PARAM_SLOTS],
}

impl AuditionVoice {
    pub fn new (selected: Shared, params: [Shared; AUDITION_PARAM_SLOTS]) -> AuditionVoice {
        AuditionVoice { selected, units: build_generators(), params }
    }
}

impl AudioNode for AuditionVoice {
    const ID: u64 = 0x7A_13;
    type Inputs = U1;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let mut output: Frame<f32, U1> = Frame::default();

        let index = self.selected.value() as usize;
        if index == 0 || index > self.units.len() { return output; } // Bypass
        let unit = &mut self.units[index - 1];

        let n_in = unit.inputs();
        let mut in_buf = [0.0f32; 1 + AUDITION_PARAM_SLOTS];
        if n_in >= 1 { in_buf[0] = input[0]; }
        for (slot, cell) in self.params.iter().enumerate().take(n_in.saturating_sub(1)) {
            in_buf[slot + 1] = cell.value();
        }

        let mut out_buf = [0.0f32; 1];
        unit.tick(&in_buf[..n_in], &mut out_buf);
        output[0] = out_buf[0];
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        for unit in self.units.iter_mut() { unit.set_sample_rate(sample_rate); }
    }
}

// GUI-facing handle: cheap to clone (Arc'd atomic cells), drives which
// generator an AuditionVoice is currently running and its live extra-input
// params without touching the audio thread directly -- same shape as
// NamModelCycler.
#[derive(Clone)]
pub struct AuditionCycler {
    selected: Shared,
    params:   [Shared; AUDITION_PARAM_SLOTS],
}

impl AuditionCycler {
    pub fn new (selected: Shared, params: [Shared; AUDITION_PARAM_SLOTS]) -> AuditionCycler {
        AuditionCycler { selected, params }
    }

    pub fn selected_name (&self) -> &'static str {
        GENERATOR_NAMES.get(self.selected.value() as usize).copied().unwrap_or("?")
    }

    // Extra-input labels for the currently selected generator, in slot
    // order -- empty slots past this slice are unused by it.
    pub fn selected_params (&self) -> &'static [&'static str] {
        GENERATOR_PARAMS.get(self.selected.value() as usize).copied().unwrap_or(&[])
    }

    pub fn param (&self, slot: usize) -> &Shared {
        &self.params[slot]
    }

    pub fn cycle (&self, delta: i32) {
        let count = GENERATOR_NAMES.len() as i32;
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }

    // Picks a random generator and re-rolls every param slot -- slots the
    // chosen generator doesn't use are rolled too (harmless, see the
    // "stale value" note atop this file) rather than special-cased.
    pub fn randomise (&self) {
        let mut rng = rand::thread_rng();
        self.selected.set_value(rand::Rng::gen_range(&mut rng, 0..GENERATOR_NAMES.len() as i32) as f32);

        let mid    = (AUDITION_PARAM_LO + AUDITION_PARAM_HI) / 2.0;
        let spread = (AUDITION_PARAM_HI - AUDITION_PARAM_LO) / 6.0;
        for cell in self.params.iter() {
            cell.set_value((mid + crate::tools::rand_normal(spread)).clamp(AUDITION_PARAM_LO, AUDITION_PARAM_HI));
        }
    }

    // (column label, cell) pairs -- generator index plus every param slot,
    // for snapshot.rs to dump/restore uniformly (parallel to ParamSpec::cells).
    pub fn cells (&self) -> Vec<(String, &Shared)> {
        let mut cells = vec![("selected".to_string(), &self.selected)];
        cells.extend(self.params.iter().enumerate().map(|(i, cell)| (format!("param{i}"), cell)));
        cells
    }
}
