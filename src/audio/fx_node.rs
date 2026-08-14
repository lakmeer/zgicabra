
//
// FxNode: shared shape for this file's effect impls (kept for potential
// reuse inside a future Voice -- see voice.rs). Not currently wired into
// Engine. Each impl is a self-contained fundsp AudioNode, 7 in / 2 out:
//   in:  [in_l, in_r, level, p1, p2, p3, p4]
//   out: [left, right]
// `level` (0..1) is a dry/wet crossfade; p1-p4 (0..1) are free for each
// impl to interpret and rescale.
//

use fundsp::prelude64::*;

pub trait FxNode: AudioNode<Inputs = U7, Outputs = U2> {
    fn name(&self) -> &'static str;
    fn param_names(&self) -> [&'static str; 4];
}
