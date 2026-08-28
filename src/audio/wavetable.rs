
use std::cmp::{min, max};

use fundsp::prelude64::*;
use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;

static WT_FILES: Dir = include_dir!("$CARGO_MANIFEST_DIR/wt");

const DEFAULT_FRAME_LEN: usize = 2048;

fn parse_wt (bytes: &[u8]) -> (Vec<f32>, usize) {
    assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
    assert_eq!(&bytes[8..12], b"WAVE", "not a WAVE file");

    let mut format_tag: u16 = 0;
    let mut data: &[u8] = &[];
    let mut frame_len = DEFAULT_FRAME_LEN;

    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = &bytes[pos + 8..pos + 8 + size];

        match id {
            b"fmt " => format_tag = u16::from_le_bytes([body[0], body[1]]),
            b"clm " => {
                if let Some(n) = std::str::from_utf8(body).ok()
                    .and_then(|text| text.strip_prefix("<!>"))
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|tok| tok.parse().ok())
                {
                    frame_len = n;
                }
            }
            b"data" => data = body,
            _ => {}
        }

        pos += 8 + size + (size & 1); // chunks are word-aligned
    }

    assert_eq!(format_tag, 3, "wavetable file is not 32-bit float PCM");

    let samples = data.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    (samples, frame_len)
}

#[derive(Clone)]
pub struct Wavetable {
    data: Vec<f32>,
    frame_len: usize,
    frame_count: usize,
    phase: f32, // cycle phase accumulator, 0..1, never reset
    sample_duration: f32,
}

impl Wavetable {
    fn load (name: &str) -> Self {
        let file = WT_FILES.get_file(format!("{name}.wt"))
            .unwrap_or_else(|| panic!("no embedded wavetable wt/{name}.wt"));

        let (data, frame_len) = parse_wt(file.contents());
        let frame_len = max(frame_len, 1);
        let frame_count = data.len() / frame_len;
        assert!(frame_count > 0, "wt/{name}.wt has no complete frames");

        Wavetable {
            data,
            frame_len,
            frame_count,
            phase: 0.0,
            sample_duration: 1.0 / DEFAULT_SR as f32,
        }
    }

    #[inline]
    fn read (&self, f: usize) -> f32 {
        let base = f * self.frame_len;
        let x  = self.phase * self.frame_len as f32;
        let i0 = x as usize % self.frame_len;
        let i1 = (i0 + 1) % self.frame_len;
        let w  = x - x.floor();
        self.data[base + i0] * (1.0 - w) + self.data[base + i1] * w
    }
}

impl AudioNode for Wavetable {
    const ID: u64 = 9002;
    type Inputs  = U2;
    type Outputs = U1;

    fn reset (&mut self) { self.phase = 0.0; }
    fn set_sample_rate (&mut self, sample_rate: f64) { self.sample_duration = 1.0 / sample_rate as f32; }

    #[inline]
    fn tick (&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        let frequency = input[0];
        let position  = input[1].clamp(0.0, 1.0);

        self.phase += frequency * self.sample_duration;
        self.phase -= floor(self.phase);

        let last = self.frame_count - 1;
        let f    = position * last as f32;
        let f0   = min(f as usize, last);
        let f1   = min(f0 + 1, last);
        let w    = f - f0 as f32;

        [self.read(f0) * (1.0 - w) + self.read(f1) * w].into()
    }

    fn route (&mut self, input: &SignalFrame, _frequency: f64) -> SignalFrame {
        Routing::Arbitrary(0.0).route(input, self.outputs())
    }
}

pub fn wavetable (name: &str) -> An<Wavetable> { An(Wavetable::load(name)) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wt_bank_report () {
        for file in WT_FILES.files() {
            if file.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("wt")) {
                let (samples, frame_len) = parse_wt(file.contents());
                println!("{:<20} frame_len {frame_len:>5}  samples {:>7}  frames {}",
                    file.path().display().to_string(), samples.len(), samples.len() / frame_len);
            }
        }
    }

    #[test]
    fn frozen_tables_load () {
        for name in ["rt_9", "rt_7", "rt_fm_5"] {
            assert!(Wavetable::load(name).frame_count > 0, "{name} loaded no frames");
        }
    }
}
