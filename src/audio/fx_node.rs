
//
// FxNode: shape shared by this file's individual effect impls (kept for
// potential reuse inside a future Voice -- see voice.rs). Every impl is a
// self-contained fundsp AudioNode, 7 in / 2 out:
//   in:  [in_l, in_r, level, p1, p2, p3, p4]
//   out: [left, right]
// `level` (0..1) is a dry/wet crossfade against the node's own input.
// p1-p4 (0..1) are free for each impl to interpret and rescale as it likes.
//
// The old FxSlot/FxCycler hot-swap-pool machinery that used to cycle
// through these at runtime (fx1-4 slots) is gone -- see the Voice trait in
// voice.rs, and Compressor (compressor.rs) for the new fixed limiter stage.
//

use fundsp::prelude64::*;

pub trait FxNode: AudioNode<Inputs = U7, Outputs = U2> {
    fn name(&self) -> &'static str;
    fn param_names(&self) -> [&'static str; 4];
}
