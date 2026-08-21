
//
// 3-operator FM generator. Currently unused -- kept, and translated to a
// graph expression, as the counterexample: FM's nested modulation needs
// each operator's *base* frequency downstream as a modulation scale, so
// every stage has to carry state forward through the frame. The straight-
// line version was shorter and clearer. This is where combinators stop
// paying for themselves; contrast stutter.rs, which is one line.
//
// Classic op_c-modulates-op_b-modulates-op_a stack; each operator gets an
// additive-Hz vibrato from one shared depth knob. 1 in (freq) -> 1 out.
// ratio_a/ratio_c/index_c are fixed.
//

use fundsp::prelude64::*;

const DETUNE_CENTS_MAX: f32 = 50.0;
const VIBRATO_HZ_MAX:   f32 = 8.0;
const VIBRATO_RATE_HZ:  f32 = 5.0;

const RATIO_A: f32 = 1.0;
const RATIO_C: f32 = 2.0;
const INDEX_C: f32 = 1.0;

// All four params are 0..1 and rescaled here: ratio_b -> 1..5,
// index_b -> 0..4, detune -> cents, vibrato -> Hz.
pub fn fm (
    ratio_b: &Shared, index_b: &Shared, detune: &Shared, vibrato: &Shared,
) -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
    let (detune, vibrato) = (detune.clone(), vibrato.clone());
    let (ratio_b, index_b) = (ratio_b.clone(), index_b.clone());

    // [freq] -> [voice_freq, wobble]
    (pass() | sine_hz(VIBRATO_RATE_HZ))
        >> map(move |f: &Frame<f32, U2>| (
            f[0] * 2f32.powf(detune.value() * DETUNE_CENTS_MAX / 1200.0),
            f[1] * (vibrato.value() * VIBRATO_HZ_MAX),
        ))
    // op_c -> [out_c, voice_freq, wobble]
        >> map(|f: &Frame<f32, U2>| (f[0] * RATIO_C + f[1], f[0], f[1]))
        >> (sine() | pass() | pass())
    // op_b, modulated by op_c -> [out_b, voice_freq, wobble, freq_b_base]
        >> map(move |f: &Frame<f32, U3>| {
            let (out_c, voice_freq, wobble) = (f[0], f[1], f[2]);
            let freq_b_base = voice_freq * (1.0 + ratio_b.value() * 4.0);
            let freq_c_base = voice_freq * RATIO_C;
            (freq_b_base + wobble + out_c * (INDEX_C * freq_c_base), voice_freq, wobble, freq_b_base)
        })
        >> (sine() | pass() | pass() | pass())
    // op_a, modulated by op_b
        >> map(move |f: &Frame<f32, U4>| {
            let (out_b, voice_freq, wobble, freq_b_base) = (f[0], f[1], f[2], f[3]);
            voice_freq * RATIO_A + wobble + out_b * (index_b.value() * 4.0 * freq_b_base)
        })
        >> sine()
}
