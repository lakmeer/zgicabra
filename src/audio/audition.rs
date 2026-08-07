
//
// AuditionVoice: cycles through fundsp's Generators (see the doc comment
// atop mod.rs) at runtime, for auditioning raw oscillator/noise character
// without wiring up a dedicated voice type. Self-contained fundsp AudioNode
// (1 in: base_freq, 1 out, unleveled -- caller multiplies by its own level
// param, same convention as the sub-oscillators). Every generator kind is
// built once at construction and kept warm; `AuditionCycler::cycle()` just
// swaps a `Shared` index, same pattern as NamModelCycler (see nam.rs) --
// picking a generator is a rare UI action, not a per-block cost worth
// optimizing further.
//
// Extra per-generator inputs (pulse/poly_pulse's duty, dsf_saw/dsf_square's
// roughness) are pinned to a fixed 0.5 rather than exposed live -- add a
// second live input if these need tweaking later.
//
// Skipped vs. the full Generators list: the _hz fixed-frequency variants
// (this voice is note-driven, so runtime frequency always wins), the
// _r/_bits parameterized duplicates (roughness/bit-count baked constants,
// redundant with the base fn's own default here), and multizero (a
// multichannel dupe of zero()).
//

use fundsp::prelude64::*;

pub const GENERATOR_NAMES: [&str; 14] = [
    "Bypass", "Saw", "Square", "Triangle", "Soft Saw",
    "Ramp", "Organ", "Hammond", "Pulse", "Poly Saw",
    "Poly Square", "Poly Pulse", "Dsf Saw", "Dsf Square",
];

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
    ]
}

#[derive(Clone)]
pub struct AuditionVoice {
    selected: Shared,
    units:    Vec<Box<dyn AudioUnit>>,
}

impl AuditionVoice {
    pub fn new (selected: Shared) -> AuditionVoice {
        AuditionVoice { selected, units: build_generators() }
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
        let mut in_buf = [0.0f32; 2];
        if n_in >= 1 { in_buf[0] = input[0]; }
        if n_in >= 2 { in_buf[1] = 0.5; }

        let mut out_buf = [0.0f32; 1];
        unit.tick(&in_buf[..n_in], &mut out_buf);
        output[0] = out_buf[0];
        output
    }

    fn set_sample_rate (&mut self, sample_rate: f64) {
        for unit in self.units.iter_mut() { unit.set_sample_rate(sample_rate); }
    }
}

// GUI-facing handle: cheap to clone (an Arc'd atomic cell), drives which
// generator an AuditionVoice is currently running without touching the
// audio thread directly -- same shape as NamModelCycler/IrCycler.
#[derive(Clone)]
pub struct AuditionCycler {
    selected: Shared,
}

impl AuditionCycler {
    pub fn new (selected: Shared) -> AuditionCycler {
        AuditionCycler { selected }
    }

    pub fn selected_name (&self) -> &'static str {
        GENERATOR_NAMES.get(self.selected.value() as usize).copied().unwrap_or("?")
    }

    pub fn cycle (&self, delta: i32) {
        let count = GENERATOR_NAMES.len() as i32;
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }
}
