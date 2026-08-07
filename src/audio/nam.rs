
//
// NAM amp model + cab IR stage, as a self-contained fundsp AudioNode (1 in,
// 1 out). Plays the role of the fuzz/distortion stage: `fuzz` is the
// dry/wet blend against the selected model's output, rather than a separate
// waveshaper after this stage.
//

use std::fs;
use std::io;
use std::sync::{Arc, Mutex};

use fundsp::prelude64::*;
use nam_rs::{Model, NamModel};

const NAM_SAMPLE_RATE: u32 = 48_000;
const NAM_DIR: &str = "nam";
const NAM_IR_DIR: &str = "nam/ir";
// Real cab IRs are tens to a few hundred ms; some files in nam/ir/ are ~75s
// exports with the actual response in the first ~150ms and a near-silent
// tail padding out the rest. Convolving live audio against the untrimmed
// file means an FFT convolution with millions of taps per block -- nowhere
// near real-time. Trim to this window on load.
const NAM_IR_MAX_SECONDS: f64 = 0.5;
const DEFAULT_NAM_MODEL: &str = "6505";

// A handle for cycling through the discovered NAM models (index 0 is always
// "Bypass" -- no model, dry passthrough) and reading the current selection's
// name. Independent of Zgicabra's Voice enum: something else (currently
// gui.rs's Model cycler buttons) drives `selected` directly.
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

    pub fn cycle (&self, delta: i32) {
        let count = self.names.len() as i32;
        if count == 0 { return; }
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }
}

// Same shape as NamModelCycler, for cycling through the discovered cab IR
// files (index 0 is always "Bypass" -- no convolution, raw amp signal).
#[derive(Clone)]
pub struct IrCycler {
    selected: Shared,
    names:    Arc<Vec<String>>,
}

impl IrCycler {
    pub fn new (selected: Shared, names: Arc<Vec<String>>) -> IrCycler {
        IrCycler { selected, names }
    }

    pub fn selected_name (&self) -> &str {
        let i = self.selected.value() as usize;
        self.names.get(i).map(String::as_str).unwrap_or("?")
    }

    pub fn cycle (&self, delta: i32) {
        let count = self.names.len() as i32;
        if count == 0 { return; }
        let current = self.selected.value() as i32;
        let next = (current + delta).rem_euclid(count);
        self.selected.set_value(next as f32);
    }
}

// Discovers every *.nam file in NAM_DIR (sorted for a stable, predictable
// cycle order) and loads each one. Index 0 is always a "Bypass" slot (no
// model, dry passthrough) so the cycler always has a way back to clean.
pub fn load_nam_models () -> io::Result<(Vec<Option<Model>>, Vec<String>)> {
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(NAM_DIR)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to read NAM model directory '{NAM_DIR}': {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("nam")))
        .collect();
    paths.sort();

    let mut models: Vec<Option<Model>> = vec![None];
    let mut names:  Vec<String>        = vec!["Bypass".to_string()];

    for path in paths {
        let path_str = path.to_string_lossy().into_owned();

        let nam_model = NamModel::from_file(&path_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load NAM model '{path_str}': {e}")))?;
        let model = Model::from_nam(&nam_model)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to build NAM model '{path_str}': {e}")))?;

        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or(&path_str).to_string();

        models.push(Some(model));
        names.push(name);
    }

    Ok((models, names))
}

// Discovers every *.wav file in NAM_IR_DIR (sorted for a stable, predictable
// cycle order) and loads each as a cab impulse response. Index 0 is always a
// "Bypass" slot (no convolution) so the cycler always has a way back to the
// raw amp signal.
pub fn load_irs () -> io::Result<(Vec<Option<Wave>>, Vec<String>)> {
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(NAM_IR_DIR)
        .map_err(|e| io::Error::new(e.kind(), format!("failed to read IR directory '{NAM_IR_DIR}': {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("wav")))
        .collect();
    paths.sort();

    let mut irs:   Vec<Option<Wave>> = vec![None];
    let mut names: Vec<String>       = vec!["Bypass".to_string()];

    for path in paths {
        let path_str = path.to_string_lossy().into_owned();

        let mut wave = Wave::load(&path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("failed to load IR '{path_str}': {e}")))?;

        if wave.sample_rate() as u32 != NAM_SAMPLE_RATE {
            println!("║ ⚠ IR '{path_str}' is {}Hz, not {NAM_SAMPLE_RATE}Hz (the rate every NAM model in nam/ was captured at) -- convolution will be pitched/timed wrong.", wave.sample_rate());
        }

        let max_samples = (wave.sample_rate() * NAM_IR_MAX_SECONDS) as usize;
        if wave.length() > max_samples {
            println!("║ ⚠ IR '{path_str}' is {:.1}s -- trimming to {NAM_IR_MAX_SECONDS}s (cab IRs are short; the rest was silent tail and made convolution too slow for real-time).", wave.length() as f64 / wave.sample_rate());
            wave.retain(0, max_samples);
        }

        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or(&path_str).to_string();

        irs.push(Some(wave));
        names.push(name);
    }

    Ok((irs, names))
}

// Index of DEFAULT_NAM_MODEL within a loaded name list, or 0 (Bypass) if not found.
pub fn default_model_index (names: &[String]) -> usize {
    names.iter().position(|n| n == DEFAULT_NAM_MODEL).unwrap_or(0)
}

// `Model` (nam-rs) isn't Clone, but AudioNode requires `Self: Clone` as a
// structural bound (fundsp's generic combinator plumbing needs it, even
// though nothing here actually clones a live NamStage). Arc<Mutex<_>> gets
// Clone/Send/Sync for free without needing Model itself to support it; the
// mutex is never contended since only the audio callback thread ever
// touches it.
#[derive(Clone)]
pub struct NamStage {
    models:      Vec<Option<Arc<Mutex<Model>>>>,
    selected:    Shared,
    fuzz:        Shared,
    dry_scratch: [f32; MAX_BUFFER_SIZE],
    irs:         Vec<Option<Wave>>,
    ir_selected: Shared,
    convolver:   Option<An<Convolver>>,
    active_ir:   usize,
    ir_input:    BufferVec,
    ir_output:   BufferVec,
}

impl NamStage {
    pub fn new (models: Vec<Option<Model>>, selected: Shared, fuzz: Shared, irs: Vec<Option<Wave>>, ir_selected: Shared) -> NamStage {
        let models = models.into_iter().map(|m| m.map(|model| Arc::new(Mutex::new(model)))).collect();
        NamStage {
            models, selected, fuzz, irs, ir_selected,
            dry_scratch: [0.0; MAX_BUFFER_SIZE],
            convolver:   None,
            active_ir:   0,
            ir_input:    BufferVec::new(1),
            ir_output:   BufferVec::new(1),
        }
    }

    fn process_buffer (&mut self, block: &mut [f32]) {
        let Some(model) = self.models.get(self.selected.value() as usize).and_then(Option::as_ref) else {
            return; // Bypass (or an out-of-range index): leave `block` untouched.
        };

        let fuzz = self.fuzz.value().clamp(0.0, 1.0);
        if fuzz <= 0.0 { return; } // fully dry: skip the model entirely

        self.dry_scratch[..block.len()].copy_from_slice(block);
        model.lock().unwrap().process_buffer(block);
        self.apply_ir(block); // cab sim only applies to the modeled ("wet") signal

        for (i, wet) in block.iter_mut().enumerate() {
            *wet = self.dry_scratch[i] * (1.0 - fuzz) + *wet * fuzz;
        }
    }

    // Rebuilds the convolver only when the selection actually changes
    // (switching is a rare UI action, not a per-block cost).
    fn apply_ir (&mut self, block: &mut [f32]) {
        let index = self.ir_selected.value() as usize;
        if index != self.active_ir {
            self.active_ir = index;
            self.convolver = self.irs.get(index).and_then(Option::as_ref).map(|wave| convolve(wave, 0));
        }

        let Some(conv) = &mut self.convolver else { return; };

        self.ir_input.channel_f32_mut(0)[..block.len()].copy_from_slice(block);
        conv.process(block.len(), &self.ir_input.buffer_ref(), &mut self.ir_output.buffer_mut());
        block.copy_from_slice(&self.ir_output.channel_f32_mut(0)[..block.len()]);
    }
}

// `process()` must be driven in <=MAX_BUFFER_SIZE chunks (see build_stream in
// mod.rs) -- the convolver is a partitioned FFT convolution sized for block
// input (see fft-convolver's `init`), so driving it one sample at a time
// re-runs its block machinery per sample, which caused audible stutter.
// `tick()` is only correct for that reason at size 1; avoid calling it in a
// hot per-sample loop for anything larger.
impl AudioNode for NamStage {
    const ID: u64 = 0x7A_12;
    type Inputs = U1;
    type Outputs = U1;

    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let mut buf = [input[0]];
        self.process_buffer(&mut buf);
        let mut output: Frame<f32, U1> = Frame::default();
        output[0] = buf[0];
        output
    }

    fn process (&mut self, size: usize, input: &BufferRef, output: &mut BufferMut) {
        let out = output.channel_f32_mut(0);
        out[..size].copy_from_slice(&input.channel_f32(0)[..size]);
        self.process_buffer(&mut out[..size]);
    }
}
