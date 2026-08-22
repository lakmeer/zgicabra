
//
// NAM amp model stage, as an FxNode (7 in, 2 out). The model selector is an
// ordinary discovered-model index into NAM_DIR. p1 = amp_blend, the dry/wet
// blend against the selected model's output. p2 = amp_boost, a pre-model
// input gain for driving a model harder without touching upstream levels.
//

use std::io;
use std::sync::{Arc, Mutex};

use fundsp::prelude64::*;
use include_dir::{include_dir, Dir};
use nam_rs::{Model, NamModel};

const DEFAULT_NAM_MODEL: &str = "mesa";
const TARGET_LOUDNESS_DB: f32 = -18.0;
const DC_BLOCKER_R: f32 = 0.9993;

// NAM models embedded straight into the binary at compile time -- a few
// hundred KB each, and the boot-time systemd service execs zgicabra from
// target/release with no reliable way to locate a runtime nam/ folder
// alongside it (unlike libsixense_x64.so/libstdc++.so.6, these are plain
// JSON, not something a RUNPATH/dlopen mechanism can find). No filesystem
// access needed at runtime, so this sidesteps that entirely.
static NAM_MODELS: Dir = include_dir!("$CARGO_MANIFEST_DIR/nam");

// Crossover split runs at the same fixed rate the model itself is pinned to
// (see Engine::set_sample_rate's NamStage no-op and pick_output_config in
// mod.rs) -- not read from set_sample_rate since, like the DC blocker, this
// stage never actually sees a different rate in practice.
const CROSSOVER_SAMPLE_RATE: f32 = super::NAM_SAMPLE_RATE as f32;

// A handle for cycling through the discovered NAM models (index 0 is always
// "Bypass" -- no model, dry passthrough) and reading the current selection's
// name. Independent of Zgicabra's Voice enum: some external caller drives
// `selected` directly via cycle() -- currently unreferenced now that the
// GUI's Model cycler buttons (its only caller) are gone.
#[derive(Clone)]
pub struct NamModelCycler {
    selected: Shared,
    names:    Arc<Vec<String>>,
}

impl NamModelCycler {
    pub fn new (selected: Shared, names: Arc<Vec<String>>) -> NamModelCycler {
        NamModelCycler { selected, names }
    }

    pub fn selected_name (&self) -> &str {
        let i = self.selected.value() as usize;
        self.names.get(i).map(String::as_str).unwrap_or("?")
    }

    // Raw Shared cell, for handing to a NamStage that this cycler's model
    // selection should drive (see GrowlVoice).
    pub fn shared (&self) -> Shared {
        self.selected.clone()
    }

    pub fn cycle (&self, delta: i32) {
        let count = self.names.len() as i32;
        if count == 0 { return; }
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }
}

// Discovers every embedded *.nam file (sorted for a stable, predictable
// cycle order) and loads each one. Index 0 is always a "Bypass" slot (no
// model, dry passthrough) so the cycler always has a way back to clean.
pub fn load_nam_models () -> io::Result<(Vec<Option<NamModelSlot>>, Vec<String>)> {
    let mut files: Vec<&include_dir::File> = NAM_MODELS.files()
        .filter(|f| f.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("nam")))
        .collect();
    files.sort_by_key(|f| f.path());

    let mut slots: Vec<Option<NamModelSlot>> = vec![None];
    let mut names: Vec<String>               = vec!["Bypass".to_string()];
    // Every model's input_gain is trimmed toward this model's input_level_dbu
    // (falls back to the first model that has one), so switching models
    // doesn't over/under-drive one relative to how it was captured.
    let mut target_input_dbu: Option<f32> = None;

    let mut loaded: Vec<(String, Model, Option<f32>, f32)> = Vec::new();

    for file in files {
        let path_str = file.path().display().to_string();

        let json = file.contents_utf8()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("embedded NAM model '{path_str}' is not valid UTF-8")))?;
        let nam_model = NamModel::from_json_str(json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load NAM model '{path_str}': {e}")))?;
        let model = Model::from_nam(&nam_model)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build NAM model '{path_str}': {e}")))?;

        let name = file.path().file_stem().and_then(|s| s.to_str()).unwrap_or(&path_str).to_string();

        // Normalize toward TARGET_LOUDNESS_DB using the file's own loudness
        // metadata. Models without it pass through unscaled.
        let output_gain = nam_model.loudness()
            .map(|loudness| db_amp(TARGET_LOUDNESS_DB - loudness))
            .unwrap_or(1.0);

        let input_dbu = nam_model.input_level_dbu();
        if name == DEFAULT_NAM_MODEL {
            target_input_dbu = input_dbu.or(target_input_dbu);
        } else if target_input_dbu.is_none() {
            target_input_dbu = input_dbu;
        }

        loaded.push((name, model, input_dbu, output_gain));
    }

    for (name, model, input_dbu, output_gain) in loaded {
        let input_gain = match (input_dbu, target_input_dbu) {
            (Some(own), Some(target)) => db_amp(target - own),
            _ => 1.0,
        };

        slots.push(Some(NamModelSlot { model: Arc::new(Mutex::new(model)), input_gain, output_gain }));
        names.push(name);
    }

    Ok((slots, names))
}

// Index of DEFAULT_NAM_MODEL within a loaded name list, or 0 (Bypass) if not found.
pub fn default_model_index (names: &[String]) -> usize {
    names.iter().position(|n| n == DEFAULT_NAM_MODEL).unwrap_or(0)
}

// Loads exactly one named embedded model by itself ({name}.nam) -- no Bypass
// slot, no relative input-gain calibration, just loudness-metadata output
// normalization. Used for the fixed "amp" stage (see mod.rs).
pub fn load_named_model (name: &str) -> io::Result<NamModelSlot> {
    let path_str = format!("{name}.nam");

    let file = NAM_MODELS.get_file(&path_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("embedded NAM model '{path_str}' not found")))?;
    let json = file.contents_utf8()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("embedded NAM model '{path_str}' is not valid UTF-8")))?;
    let nam_model = NamModel::from_json_str(json)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load NAM model '{path_str}': {e}")))?;
    let model = Model::from_nam(&nam_model)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build NAM model '{path_str}': {e}")))?;

    let output_gain = nam_model.loudness()
        .map(|loudness| db_amp(TARGET_LOUDNESS_DB - loudness))
        .unwrap_or(1.0);

    Ok(NamModelSlot { model: Arc::new(Mutex::new(model)), input_gain: 1.0, output_gain })
}

// `Model` (nam-rs) isn't Clone, but AudioNode requires `Self: Clone` as a
// structural bound (fundsp's generic combinator plumbing needs it, even
// though nothing here actually clones a live NamStage). Arc<Mutex<_>> gets
// Clone/Send/Sync for free without needing Model itself to support it; the
// mutex is never contended since only the audio callback thread ever
// touches it.
#[derive(Clone)]
pub struct NamModelSlot {
    pub(super) model:       Arc<Mutex<Model>>,
    // Relative to the .nam's own input_level_dbu metadata, trimmed toward a
    // common target so every model gets driven at roughly the level it was
    // captured at (see load_nam_models).
    pub(super) input_gain:  f32,
    // Toward TARGET_LOUDNESS_DB from the .nam's own loudness metadata, so
    // cycling models doesn't jump wildly in level.
    pub(super) output_gain: f32,
}

// Scratch/chunk cap for NamStage::process_block. nam-rs's own process_buffer
// already internally chunks at its `MAX_BLOCK` (see wavenet.rs), so this
// only exists as a fixed-size scratch buffer bound -- cpal callback sizes
// are always far below it in practice.
pub(crate) const NAM_BLOCK_CAP: usize = 4096;

// Clone is a structural bound only (see GrowlVoice/AudioNode) -- nothing
// actually clones a live NamStage.
#[derive(Clone)]
pub struct NamStage {
    models:    Vec<Option<NamModelSlot>>,
    // Model selector -- the FxNode's one extra int field.
    selected:  Shared,
    // One-pole DC blocker state, carried across calls (see DC_BLOCKER_R).
    dc_prev_x: f32,
    dc_prev_y: f32,
    // Pre-model dry copy of the current block (the high band once crossover
    // has split it off), for the dry/wet blend at the end of process_block --
    // sized once at construction, never reallocated on the audio thread.
    dry_scratch: Vec<f32>,
    // Crossover low-band state (one-pole lowpass, carried across calls) and
    // its scratch -- the low band never touches the model, just gets added
    // back after. Sized once, never reallocated on the audio thread.
    xover_lp:     f32,
    low_scratch:  Vec<f32>,
}

impl NamStage {
    pub fn new (models: Vec<Option<NamModelSlot>>, selected: Shared) -> NamStage {
        NamStage {
            models, selected, dc_prev_x: 0.0, dc_prev_y: 0.0,
            dry_scratch: vec![0.0; NAM_BLOCK_CAP],
            xover_lp: 0.0, low_scratch: vec![0.0; NAM_BLOCK_CAP],
        }
    }

    // No-op: the model runs at its own fixed training rate (NAM_SAMPLE_RATE
    // in mod.rs, which pick_output_config already pins the device to) and
    // the DC blocker's R is a fixed constant, not derived from sample rate.
    // Exists only so Engine::set_sample_rate's uniform per-field loop
    // doesn't need a special case for this one field.
    pub fn set_sample_rate (&mut self, _sr: f64) {}

    // Must run over a whole block, not one sample at a time -- nam-rs's
    // process_buffer is a block kernel and per-sample calls defeat it,
    // causing audible stutter.
    //
    // Always runs the selected model, even at blend=0, so its WaveNet
    // dilation state stays warm -- otherwise every blend sweep from 0
    // restarts the model cold and its startup transient (receptive_field()
    // samples) mixes into the output. The dry/wet blend below already
    // reduces to 100% dry at blend=0.
    //
    // level/blend/boost/crossover_hz are read once per block by the caller
    // since these are knob-rate, not audio-rate.
    //
    // crossover_hz splits the block into a low band that stays dry (never
    // touches the model, summed back in full) and a high band that goes
    // through the model/blend/level pipeline -- 0Hz makes the split a no-op.
    pub(crate) fn process_block (&mut self, block: &mut [f32], level: f32, blend: f32, boost: f32, crossover_hz: f32) {
        let Some(slot) = self.models.get(self.selected.value() as usize).and_then(Option::as_ref) else {
            return; // Bypass (or an out-of-range index): leave `block` untouched (dry).
        };
        let model = slot.model.clone();
        let (input_gain, output_gain) = (slot.input_gain, slot.output_gain);

        let alpha = Self::xover_alpha(crossover_hz);
        for (i, s) in block.iter_mut().enumerate() {
            self.xover_lp += alpha * (*s - self.xover_lp);
            self.low_scratch[i] = self.xover_lp;
            *s -= self.xover_lp; // high band only, from here on
        }

        self.dry_scratch[..block.len()].copy_from_slice(block);

        let pre = boost * input_gain;
        if pre != 1.0 {
            for s in block.iter_mut() { *s *= pre; }
        }

        model.lock().unwrap().process_buffer(block);
        self.dc_block(block);

        if output_gain != 1.0 {
            for s in block.iter_mut() { *s *= output_gain; }
        }

        for (i, wet) in block.iter_mut().enumerate() {
            let dry = self.dry_scratch[i];
            let modeled = dry * (1.0 - blend) + *wet * blend;
            *wet = dry * (1.0 - level) + modeled * level + self.low_scratch[i];
        }
    }

    // One-pole (6dB/oct) lowpass coefficient for the crossover split -- a
    // "gentle" slope, not a steep multi-order crossover, since this is just
    // keeping sub-bass out of the model rather than a precise 2-way split.
    // 0Hz collapses to alpha=0: the lowpass state never moves off its
    // initial 0.0, so the "low band" stays silent and the high band is the
    // untouched full signal -- the inaudible/no-op default the caller wants.
    fn xover_alpha (fc: f32) -> f32 {
        1.0 - (-2.0 * std::f32::consts::PI * fc / CROSSOVER_SAMPLE_RATE).exp()
    }

    // ~5Hz one-pole highpass to strip WaveNet DC bias (see DC_BLOCKER_R).
    fn dc_block (&mut self, block: &mut [f32]) {
        for s in block.iter_mut() {
            let x = *s;
            let y = x - self.dc_prev_x + DC_BLOCKER_R * self.dc_prev_y;
            self.dc_prev_x = x;
            self.dc_prev_y = y;
            *s = y;
        }
    }
}
