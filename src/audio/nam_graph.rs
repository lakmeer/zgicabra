
//
// The NAM signal path as fundsp graph expressions.
//
// Everything NamStage used to fold into one process_block -- crossover
// split, gain staging, DC blocking, dry/wet blend, mid/side collapse -- is
// routing, and routing is what fundsp's combinators are for. NamNode
// (nam_node.rs) is the only piece that has to be a hand-written node,
// because it is the only piece that is actually an algorithm.
//
// Params arrive as `Shared` cells read inside map/var closures rather than
// as positional Frame slots, so a stage's signature says what it takes.
//

use fundsp::prelude64::*;

use super::nam::NamModelSlot;
use super::nam_node::NamNode;

// DC blocker cutoff. WaveNet output carries a DC bias; this is well below
// the audible range and matches the ~5Hz one-pole NamStage used.
const DC_BLOCK_HZ: f32 = 5.0;

// Smoothing for the per-band output monitors. Peak rather than RMS because
// these feed a level indicator, not a loudness readout.
const METER_SMOOTH_S: f64 = 0.05;

// 1 in -> 2 out: [low, high].
//
// Complementary one-pole: high is defined as input - low, so low + high ==
// input exactly. That matters because the low band bypasses the model and
// gets summed back in full -- highpole_hz() is a different filter, not the
// complement of lowpole(), and using it would leave a dip at the crossover.
//
// A 0Hz cutoff collapses to the no-op the callers want: the filter state
// never leaves 0, so low is silent and high is the untouched input.
pub fn crossover (cutoff_hz: &Shared) -> An<impl AudioNode<Inputs = U1, Outputs = U2>> {
    (((pass() | var(cutoff_hz)) >> lowpole()) ^ pass())
        >> map(|f: &Frame<f32, U2>| (f[0], f[1] - f[0]))
}

// Stereo <-> mid/side. 2 in, 2 out each way.
pub fn to_mid_side () -> An<impl AudioNode<Inputs = U2, Outputs = U2>> {
    map(|f: &Frame<f32, U2>| ((f[0] + f[1]) * 0.5, (f[0] - f[1]) * 0.5))
}

pub fn from_mid_side () -> An<impl AudioNode<Inputs = U2, Outputs = U2>> {
    map(|f: &Frame<f32, U2>| (f[0] + f[1], f[0] - f[1]))
}

// One band through one model: gain stage in, inference, DC block, gain
// stage out, then a dry/wet blend against the untouched input.
//
// The dry branch is pre-input-gain, matching what NamStage did -- blend=0
// has to be bit-identical to bypass, not "bypass times input_gain".
//
// `out_level` gets the post-blend peak so the UI can see the band working;
// monitor() passes the signal through untouched.
pub fn nam_band (
    slot:      &NamModelSlot,
    blend:     &Shared,
    out_level: &Shared,
    window:    usize,
) -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
    let blend = blend.clone();

    ((mul(slot.input_gain)
        >> An(NamNode::new(slot.model.clone(), window))
        >> dcblock_hz(DC_BLOCK_HZ)
        >> mul(slot.output_gain))
     ^ pass())
        >> map(move |f: &Frame<f32, U2>| {
            let (wet, dry) = (f[0], f[1]);
            dry + blend.value() * (wet - dry)
        })
        >> monitor(out_level, Meter::Peak(METER_SMOOTH_S))
}

// Single-model variant: split, model only the high band, sum back. The low
// band bypasses the model entirely -- this is what NamStage's crossover_hz
// param did, and a 0Hz cutoff makes it a no-op that models everything.
// 1 in, 1 out.
pub fn nam_high_band (
    slot:      &NamModelSlot,
    blend:     &Shared,
    cutoff_hz: &Shared,
    out_level: &Shared,
    window:    usize,
) -> An<impl AudioNode<Inputs = U1, Outputs = U1>> {
       crossover(cutoff_hz)                                     // [low, high]
    >> (pass() | nam_band(slot, blend, out_level, window))       // low stays dry
    >> map(|f: &Frame<f32, U2>| f[0] + f[1])
}

// The stage this spike exists to evaluate. 2 in (L, R) -> 2 out (L, R).
//
// Split each channel into low/high, collapse each band to mid/side, run one
// model on each band's mid, rebuild. Two inferences per block instead of
// four, and -- because each band has its own NamNode -- no chance of one
// band's dilation state bleeding into the other's.
pub fn nam_mid_side (
    lo: &NamModelSlot, hi: &NamModelSlot,
    blend: &Shared, cutoff_hz: &Shared,
    lo_level: &Shared, hi_level: &Shared,
    window: usize,
) -> An<impl AudioNode<Inputs = U2, Outputs = U2>> {
       (crossover(cutoff_hz) | crossover(cutoff_hz))            // [Llo, Lhi, Rlo, Rhi]
    >> map(|f: &Frame<f32, U4>| (f[0], f[2], f[1], f[3]))       // regroup band-major
    >> (to_mid_side() | to_mid_side())                          // [Mlo, Slo, Mhi, Shi]
    >> (nam_band(lo, blend, lo_level, window) | pass()
      | nam_band(hi, blend, hi_level, window) | pass())         // models on the mids
    >> (from_mid_side() | from_mid_side())                      // [Llo, Rlo, Lhi, Rhi]
    >> map(|f: &Frame<f32, U4>| (f[0] + f[2], f[1] + f[3]))     // sum the bands
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::nam::load_named_model;
    use super::super::nam_node::NamNode;

    const TEST_MODEL: &str = "mesa";

    // A deterministic, broadband-ish signal: a sweep, so a phase or
    // alignment error shows up as an obvious mismatch rather than a
    // coincidentally-close one.
    fn sweep (n: usize) -> Vec<f32> {
        (0..n).map(|i| {
            let t = i as f32 / 48_000.0;
            (std::f32::consts::TAU * (80.0 + 4000.0 * t) * t).sin() * 0.3
        }).collect()
    }

    // The decisive check: driving NamNode in 64-sample process() calls must
    // produce exactly what process_buffer produces over the whole signal,
    // delayed by one window. This is where the cursor arithmetic would be
    // wrong -- note WINDOW is deliberately not a multiple of 64 in one of
    // the cases below, so `size` straddles the window boundary.
    fn assert_node_matches_direct (window: usize) {
        let slot = load_named_model(TEST_MODEL).expect("test model");
        let total = window * 4;
        let input = sweep(total + window);

        // Reference: one model, one call per window, no node in the way.
        let reference = {
            let mut buf = input.clone();
            let model = load_named_model(TEST_MODEL).expect("test model");
            let mut m = model.model.lock().unwrap();
            for chunk in buf.chunks_mut(window) {
                if chunk.len() == window { m.process_buffer(chunk); }
            }
            buf
        };

        // Under test: same model, driven through the node in 64-sample blocks.
        let mut node = NamNode::new(slot.model.clone(), window);
        let mut got = vec![0.0f32; input.len()];
        let mut inb  = BufferArray::<U1>::new();
        let mut outb = BufferArray::<U1>::new();

        let mut done = 0;
        while done < input.len() {
            let n = std::cmp::min(MAX_BUFFER_SIZE, input.len() - done);
            inb.channel_f32_mut(0)[..n].copy_from_slice(&input[done..done + n]);
            node.process(n, &inb.buffer_ref(), &mut outb.buffer_mut());
            got[done..done + n].copy_from_slice(&outb.channel_f32_mut(0)[..n]);
            done += n;
        }

        // got[window + i] should be reference[i] -- exactly `window` samples
        // of latency, and the same arithmetic, so this is an exact compare
        // up to f32 rounding in the copies (there is none).
        for i in 0..total {
            let (a, b) = (got[window + i], reference[i]);
            assert!(
                (a - b).abs() < 1e-6,
                "window {window}, sample {i}: node {a} != direct {b}",
            );
        }
    }

    #[test]
    fn nam_node_matches_direct_inference_delayed_by_window () {
        assert_node_matches_direct(512);  // multiple of MAX_BUFFER_SIZE
        assert_node_matches_direct(300);  // deliberately not, so blocks straddle
    }

    // The crossover must reconstruct exactly, or the low band that bypasses
    // the model will not sum back flat.
    #[test]
    fn crossover_bands_sum_to_input () {
        let fc = shared(800.0);
        let mut node = crossover(&fc);
        node.set_sample_rate(48_000.0);

        for (i, x) in sweep(4096).into_iter().enumerate() {
            let out = node.tick(&Frame::from([x]));
            let sum = out[0] + out[1];
            assert!((sum - x).abs() < 1e-6, "sample {i}: {sum} != {x}");
        }
    }

    // A 0Hz cutoff has to be a true no-op: everything in the high band.
    #[test]
    fn crossover_at_zero_hz_passes_everything_high () {
        let fc = shared(0.0);
        let mut node = crossover(&fc);
        node.set_sample_rate(48_000.0);

        for x in sweep(512) {
            let out = node.tick(&Frame::from([x]));
            assert!(out[0].abs() < 1e-9, "low band should be silent, got {}", out[0]);
            assert!((out[1] - x).abs() < 1e-6);
        }
    }

    // Not a correctness test -- this is the measurement that picks
    // NAM_WINDOW. Run with: cargo test nam_block_size_throughput -- --nocapture
    #[test]
    fn nam_block_size_throughput () {
        let slot = load_named_model(TEST_MODEL).expect("test model");
        let total = 48_000 * 2; // two seconds of audio per size

        println!("\nNAM process_buffer throughput ({TEST_MODEL}, {total} samples per run)");
        println!("{:>6}  {:>12}  {:>10}  {:>9}", "block", "samples/s", "xrealtime", "latency");

        for &block in &[64usize, 128, 256, 512, 1024, 4096] {
            let mut m = slot.model.lock().unwrap();
            let mut buf = vec![0.1f32; block];

            let start = std::time::Instant::now();
            let mut done = 0;
            while done < total {
                m.process_buffer(&mut buf);
                done += block;
            }
            let secs = start.elapsed().as_secs_f64();

            let sps = done as f64 / secs;
            println!(
                "{:>6}  {:>12.0}  {:>9.1}x  {:>7.1}ms",
                block, sps, sps / 48_000.0, block as f64 / 48.0,
            );
        }
        println!();
    }
}

// End-to-end: the two voices that now route through this graph must produce
// finite, non-silent audio. The binary's own --test selftest can't cover
// this -- it reads through the master chain, where `level` comes from the
// Hydra and is 0 with no hardware attached, so it reports silence either way
// (confirmed against the pre-spike baseline).
#[cfg(test)]
mod voices {
    use fundsp::prelude64::*;

    use super::super::signal::SharedSignal;
    use super::super::voice::Voice;
    use super::super::growl::GrowlVoice;
    use super::super::swarm::SwarmVoice;
    use super::super::nam::load_nam_models;
    use super::super::nam_node::NAM_WINDOW;

    // Enough to clear several inference windows, so we are looking at steady
    // state rather than the pre-roll of zeros the window costs.
    const TICKS: usize = NAM_WINDOW * 4;

    fn signal () -> SharedSignal {
        let sig = SharedSignal::new();
        sig.fuzz.set_value(0.5);   // drive the dry/wet blend so the model output matters
        sig.filter.set_value(0.7);
        sig
    }

    fn assert_audible (name: &str, out: &[f32]) {
        let peak = out.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!(out.iter().all(|s| s.is_finite()), "{name}: non-finite output");
        assert!(peak > 1e-5, "{name}: silent (peak {peak})");
        assert!(peak < 100.0, "{name}: runaway output (peak {peak})");
    }

    #[test]
    fn growl_nam_graph_produces_audio () {
        let mut v = GrowlVoice::new(shared(1.0), shared(1.5), shared(0.3), signal());
        v.set_sample_rate(48_000.0);

        let out: Vec<f32> = (0..TICKS)
            .map(|_| v.tick(110.0).0)
            .collect();

        assert_audible("growl", &out[NAM_WINDOW * 2..]);
    }

    #[test]
    fn swarm_nam_graph_produces_audio () {
        let (slots, names) = load_nam_models().expect("nam models");
        let mut v = SwarmVoice::new(
            slots, std::sync::Arc::new(names),
            shared(1.0), shared(1.5), shared(0.3),
            signal(),
        );
        v.set_sample_rate(48_000.0);

        let mut left  = Vec::with_capacity(TICKS);
        let mut right = Vec::with_capacity(TICKS);
        for _ in 0..TICKS {
            let (l, r) = v.tick(110.0);
            left.push(l);
            right.push(r);
        }

        assert_audible("swarm L", &left[NAM_WINDOW * 2..]);
        assert_audible("swarm R", &right[NAM_WINDOW * 2..]);
    }
}
