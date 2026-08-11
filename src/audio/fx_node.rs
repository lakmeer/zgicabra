
//
// FxNode: shared interface for swappable DSP effects. Every impl is a
// self-contained fundsp AudioNode, 7 in / 2 out:
//   in:  [in_l, in_r, level, p1, p2, p3, p4]
//   out: [left, right]
// `level` (0..1) is a dry/wet crossfade against the node's own input --
// unlike GenNode (which has no upstream dry signal to fall back to), an
// FxNode sits in a serial chain, so a plain output multiply would silence
// everything downstream of it instead of bypassing just this stage. p1-p4
// (0..1) are free for each impl to interpret and rescale as it likes.
//
// FxSlot cycles through a fixed set of FxNode impls for the fx1-4 slots
// (index 0 = Bypass), same warm-pool pattern as GenSlot.
//

use fundsp::prelude64::*;

use super::filter::MoogFilterFx;

pub trait FxNode: AudioNode<Inputs = U7, Outputs = U2> {
    fn name(&self) -> &'static str;
    fn param_names(&self) -> [&'static str; 4];
}

pub const FX_NAMES: [&str; 2] = ["Bypass", "Moog Filter"];
pub const FX_PARAMS: [[&str; 4]; 2] = [
    ["", "", "", ""],
    ["cutoff", "resonance", "", ""],
];

// One entry per FX_NAMES minus the leading "Bypass" -- FxSlot::tick already
// special-cases index 0 as bypass without touching this vec (see below), so
// index 1 (the first real effect) must land on units[0], same convention
// GenNode's build_gens() uses.
fn build_fx () -> Vec<Box<dyn AudioUnit>> {
    vec![
        Box::new(An(MoogFilterFx::new())),
    ]
}

// Audio-thread owner of an fx1-4 slot: cycles among FX_NAMES by index
// (Bypass = 0), forwarding [in_l, in_r, level, p1..p4] into whichever
// FxNode is selected every tick.
pub struct FxSlot {
    selected: Shared,
    units:    Vec<Box<dyn AudioUnit>>,
}

impl FxSlot {
    pub fn new (selected: Shared) -> FxSlot {
        FxSlot { selected, units: build_fx() }
    }

    pub fn tick (&mut self, in_l: f32, in_r: f32, level: f32, p: [f32; 4]) -> (f32, f32) {
        let index = self.selected.value() as usize;
        if index == 0 || index > self.units.len() { return (in_l, in_r); } // Bypass
        let unit = &mut self.units[index - 1];

        let in_buf = [in_l, in_r, level, p[0], p[1], p[2], p[3]];
        let mut out = [0.0f32; 2];
        unit.tick(&in_buf, &mut out);
        (out[0], out[1])
    }

    pub fn set_sample_rate (&mut self, sample_rate: f64) {
        for unit in self.units.iter_mut() { unit.set_sample_rate(sample_rate); }
    }
}

// Cheap GUI-facing handle onto an fx1-4 slot, same shape as GenCycler.
#[derive(Clone)]
pub struct FxCycler {
    selected: Shared,
}

impl FxCycler {
    pub fn new (selected: Shared) -> FxCycler {
        FxCycler { selected }
    }

    pub fn selected_name (&self) -> &'static str {
        FX_NAMES.get(self.selected.value() as usize).copied().unwrap_or("?")
    }

    pub fn selected_params (&self) -> [&'static str; 4] {
        FX_PARAMS.get(self.selected.value() as usize).copied().unwrap_or(["", "", "", ""])
    }

    pub fn cycle (&self, delta: i32) {
        let count = FX_NAMES.len() as i32;
        let current = self.selected.value() as i32;
        self.selected.set_value((current + delta).rem_euclid(count) as f32);
    }

    pub fn cells (&self) -> [(&'static str, &Shared); 1] {
        [("selected", &self.selected)]
    }
}
