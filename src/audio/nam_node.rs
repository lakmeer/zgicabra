
//
// NAM model inference as a fundsp graph node (1 in, 1 out).
//
// fundsp hands a node at most MAX_BUFFER_SIZE (64) samples per process()
// call, which is far below the block size nam-rs wants -- per-call overhead
// dominates and throughput collapses. So this node accumulates into a
// window of `window` samples and runs one process_buffer per window.
//
// The swap is the same read-then-overwrite trick GrowlVoice used to do by
// hand (one buffer, not two: the wet value is consumed before this sample's
// dry value lands on top of it), just relocated inside the node and keyed to
// the window rather than to the cpal callback size. Latency is exactly
// `window` samples, independent of what the host hands us.
//
// Everything else that NamStage folded together -- input/output gain, DC
// blocking, dry/wet blend, crossover -- is graph routing now, and lives in
// nam_graph.rs. This node is only the model.
//

use std::sync::{Arc, Mutex};

use fundsp::prelude64::*;
use nam_rs::Model;

// Default inference window. See the bench in nam_graph.rs's tests for the
// throughput/latency tradeoff: at 48kHz this is ~10.7ms of latency.
pub const NAM_WINDOW: usize = 512;

// Arc<Mutex<_>> because AudioNode requires Clone as a structural bound and
// nam_rs::Model isn't Clone (its weights and its dilation state are
// interleaved, so there is no cheap shallow copy). The mutex is never
// contended -- only the audio thread ever touches it. See nam.rs for the
// long version of this note.
#[derive(Clone)]
pub struct NamNode {
    model:  Arc<Mutex<Model>>,
    buf:    Vec<f32>,
    cursor: usize,
}

impl NamNode {
    pub fn new (model: Arc<Mutex<Model>>, window: usize) -> NamNode {
        NamNode { model, buf: vec![0.0; std::cmp::max(window, 1)], cursor: 0 }
    }

    // Swap samples through the window: wet out of each slot, dry in, same
    // slot. Loops because `dry` may straddle the window boundary.
    fn swap (&mut self, dry: &[f32], wet: &mut [f32]) {
        let mut done = 0;
        while done < dry.len() {
            let take = std::cmp::min(self.buf.len() - self.cursor, dry.len() - done);
            for k in 0..take {
                let slot = &mut self.buf[self.cursor + k];
                wet[done + k] = *slot;
                *slot = dry[done + k];
            }
            self.cursor += take;
            done        += take;

            if self.cursor == self.buf.len() {
                // Always runs, even when the caller is blending fully dry, so
                // the model's dilation state stays warm -- a cold model emits
                // a receptive-field-long startup transient.
                self.model.lock().unwrap().process_buffer(&mut self.buf);
                self.cursor = 0;
            }
        }
    }
}

impl AudioNode for NamNode {
    const ID: u64 = 0x7A_70;
    type Inputs  = U1;
    type Outputs = U1;

    // Per-sample fallback. Correct, but the whole point of this node is
    // process() below -- a graph driven by tick() gets one-sample swaps and
    // the same inference cost, just spread differently.
    fn tick (&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        let mut wet = [0.0];
        self.swap(&[input[0]], &mut wet);
        Frame::from(wet)
    }

    fn process (&mut self, size: usize, input: &BufferRef, output: &mut BufferMut) {
        // Input and output are distinct buffers, so both slices can be held
        // at once -- no staging copy needed.
        let dry = input.channel_f32(0);
        let wet = output.channel_f32_mut(0);
        self.swap(&dry[..size], &mut wet[..size]);
    }

    // Deliberately no set_sample_rate: the model runs at its own capture rate
    // and pick_output_config already pins the device to it (see mod.rs).
}
