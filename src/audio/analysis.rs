//
// Analysis -- small FFT-based spectral analysis used to research wavetable
// recreations of a recorded sample (see BasicVoice's `feedback` oscillator).
// Chunks a mono sample into fixed-duration windows, FFTs each (Hann-
// windowed, zero-padded to a power of two), and reports chunk-to-chunk
// spectral distance -- the evidence for whether a recreation needs to
// morph over time or can get away with one static cycle.
//
// Not wired into the live audio graph -- see the `feedback_report` test
// below for how to run it.
//

use std::cmp::{min, max};
use std::f32::consts::TAU;
use std::sync::Arc;

use fundsp::fft::real_fft;
use fundsp::prelude64::*;
use num_complex::Complex32;

use super::sample::Sample;
use super::NAM_SAMPLE_RATE;

// Normalized (sum-to-one) magnitude spectrum for one chunk, plus where it
// started in the source sample -- enough to print a report or feed
// `spectral_distance`.
pub struct ChunkSpectrum {
    pub start_sample: usize,
    pub magnitudes:   Vec<f32>, // bins 0..=fft_len/2, normalized so they sum to 1
}

impl ChunkSpectrum {
    // Frequency (Hz) of the loudest bin, i.e. this chunk's dominant partial.
    pub fn peak_hz (&self, sample_rate: f32, fft_len: usize) -> f32 {
        let (bin, _) = self.magnitudes.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        bin as f32 * sample_rate / fft_len as f32
    }
}

// Splits `samples` into non-overlapping chunks of `chunk_len` samples (the
// final chunk may be shorter), Hann-windows each to limit spectral leakage,
// zero-pads to the next power of two, and real-FFTs it.
pub fn analyze_chunks (samples: &[f32], chunk_len: usize) -> Vec<ChunkSpectrum> {
    samples.chunks(chunk_len).enumerate().map(|(i, chunk)| {
        let fft_len = max(chunk.len(), 2).next_power_of_two();
        let mut buf = vec![0.0f32; fft_len];

        let window_n = (max(chunk.len(), 2) - 1) as f32;
        for (n, &s) in chunk.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU * n as f32 / window_n).cos();
            buf[n] = s * w;
        }

        let spectrum = real_fft(&mut buf);
        let magnitudes: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();
        let sum: f32 = magnitudes.iter().sum::<f32>().max(1e-9);

        ChunkSpectrum {
            start_sample: i * chunk_len,
            magnitudes: magnitudes.iter().map(|m| m / sum).collect(),
        }
    }).collect()
}

// Cosine distance between two normalized magnitude spectra: 0 means
// identical shape, 1 means orthogonal (unrelated harmonic content).
// Compares only the overlapping prefix, since only the final chunk (the
// leftover tail) has a shorter FFT than the rest.
pub fn spectral_distance (a: &[f32], b: &[f32]) -> f32 {
    let n = min(a.len(), b.len());
    let dot: f32 = (0..n).map(|i| a[i] * b[i]).sum();
    let na: f32 = a[..n].iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b[..n].iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-9 || nb < 1e-9 { return 0.0; }
    1.0 - (dot / (na * nb)).clamp(-1.0, 1.0)
}

// Static feedback recording, embedded at compile time -- same reasoning as
// reese.rs's IMPACT_SAMPLE (see there): no reliable runtime wav/ folder
// alongside the deployed binary.
static FEEDBACK_SAMPLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/wav/feedback.wav"));

// Builds a bandlimited wavetable from one representative FFT frame of
// feedback.wav, for use as a fundsp oscillator (see `feedback()` below).
//
// `feedback_report` (below) found the sample's spectrum essentially static
// (chunk-to-chunk cosine distance ~0.002-0.005) across its whole sustain,
// diverging only in the final, truncated chunk -- an artifact of that
// chunk being mostly silence, not a real timbral shift. So one cycle, not
// a time-varying morph, is enough to recreate it: bin `i` of a single FFT
// frame directly gives partial `i`'s (amplitude, phase), which is exactly
// what `Wavetable::new` wants.
fn feedback_wavetable () -> Arc<Wavetable> {
    let sample = Sample::parse(FEEDBACK_SAMPLE);
    let samples: Vec<f32> = (0..sample.length()).map(|i| sample.at(i)).collect();

    let sample_rate = NAM_SAMPLE_RATE as f32;
    let chunk_len = (0.1 * sample_rate) as usize;

    // Chunk 1 (0.1-0.2s): mid-sustain, past any pluck/attack transient.
    let spectrum = chunk_complex_spectrum(&samples, chunk_len, 1);

    // The loudest non-DC bin is the fundamental; every other partial in
    // this recording lands close to an integer multiple of it.
    let fundamental_bin = spectrum.iter().enumerate().skip(1)
        .max_by(|a, b| a.1.norm().partial_cmp(&b.1.norm()).unwrap())
        .map(|(i, _)| i)
        .expect("non-empty spectrum");

    let bin_for = |i: u32| min(fundamental_bin * i as usize, spectrum.len() - 1);
    let phase     = |i: u32| spectrum[bin_for(i)].arg() as f64 / std::f64::consts::TAU;
    let amplitude = |_pitch: f64, i: u32| spectrum[bin_for(i)].norm() as f64;

    Arc::new(Wavetable::new(20.0, 20_000.0, 4.0, &phase, &amplitude))
}

// The `feedback` oscillator: a bandlimited recreation of wav/feedback.wav,
// playable at any pitch like BasicVoice's other built-in oscillators.
pub fn feedback () -> An<WaveSynth<U1>> {
    An(WaveSynth::new(feedback_wavetable()))
}

// Real-FFTs one `chunk_len`-sample chunk of `samples` (Hann-windowed,
// zero-padded to a power of two), returning its complex spectrum.
fn chunk_complex_spectrum (samples: &[f32], chunk_len: usize, chunk_index: usize) -> Vec<Complex32> {
    let start = chunk_index * chunk_len;
    let chunk = &samples[start..min(start + chunk_len, samples.len())];

    let fft_len = max(chunk.len(), 2).next_power_of_two();
    let mut buf = vec![0.0f32; fft_len];

    let window_n = (max(chunk.len(), 2) - 1) as f32;
    for (n, &s) in chunk.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * n as f32 / window_n).cos();
        buf[n] = s * w;
    }

    real_fft(&mut buf).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Prints a per-chunk report for wav/feedback.wav and the chunk-to-chunk
    // spectral distance that decides whether BasicVoice's `feedback`
    // oscillator needs a time-based morph. Run with:
    //   cargo test feedback_report -- --nocapture
    #[test]
    fn feedback_report () {
        let sample = Sample::parse(FEEDBACK_SAMPLE);
        let samples: Vec<f32> = (0..sample.length()).map(|i| sample.at(i)).collect();

        let sample_rate = NAM_SAMPLE_RATE as f32;
        let chunk_len = (0.1 * sample_rate) as usize; // 0.1s chunks per the brief

        let chunks = analyze_chunks(&samples, chunk_len);

        println!("feedback.wav: {} samples, {:.3}s, {} chunks of {chunk_len} samples",
            samples.len(), samples.len() as f32 / sample_rate, chunks.len());

        let mut distances = vec![];
        for (i, chunk) in chunks.iter().enumerate() {
            let fft_len = chunk.magnitudes.len() * 2;
            let t = chunk.start_sample as f32 / sample_rate;
            let peak = chunk.peak_hz(sample_rate, fft_len);
            print!("  chunk {i} @ {t:.2}s: peak {peak:.1}Hz");

            if i > 0 {
                let d = spectral_distance(&chunks[i - 1].magnitudes, &chunk.magnitudes);
                distances.push(d);
                print!("  distance-from-prev {d:.4}");
            }
            println!();
        }

        if !distances.is_empty() {
            let mean: f32 = distances.iter().sum::<f32>() / distances.len() as f32;
            let max = distances.iter().cloned().fold(0.0f32, f32::max);
            println!("mean chunk-to-chunk distance: {mean:.4}, max: {max:.4}");
        }
    }

    // Prints the actual harmonic amplitudes/phases `feedback_wavetable()`
    // pulled out of chunk 1 -- the Fourier coefficients the oscillator was
    // built from. Run with:
    //   cargo test feedback_harmonics -- --nocapture
    #[test]
    fn feedback_harmonics () {
        let sample = Sample::parse(FEEDBACK_SAMPLE);
        let samples: Vec<f32> = (0..sample.length()).map(|i| sample.at(i)).collect();

        let sample_rate = NAM_SAMPLE_RATE as f32;
        let chunk_len = (0.1 * sample_rate) as usize;
        let spectrum = chunk_complex_spectrum(&samples, chunk_len, 1);

        let fundamental_bin = spectrum.iter().enumerate().skip(1)
            .max_by(|a, b| a.1.norm().partial_cmp(&b.1.norm()).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let fft_len = spectrum.len() * 2; // microfft real_fft: N reals -> N/2 complex bins
        let bin_hz = sample_rate / fft_len as f32;

        let peak_mag = spectrum.iter().map(|c| c.norm()).fold(0.0f32, f32::max);

        println!("fundamental bin {fundamental_bin} ({:.1}Hz), fft_len {fft_len}, bin width {bin_hz:.2}Hz",
            fundamental_bin as f32 * bin_hz);

        for i in 1u32..=16 {
            let bin = min(fundamental_bin * i as usize, spectrum.len() - 1);
            let c = spectrum[bin];
            let rel_amp = c.norm() / peak_mag;
            let phase_deg = c.arg().to_degrees();
            println!("  h{i:>2} bin {bin:>5}  {:>7.1}Hz  amp {rel_amp:.4}  phase {phase_deg:>7.1}deg",
                bin as f32 * bin_hz);
        }
    }
}
