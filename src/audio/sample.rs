
use std::sync::Arc;

use fundsp::prelude64::*;

pub struct Sample {
    data: Vec<f32>,
}

impl Sample {
    pub fn parse (bytes: &[u8]) -> Sample {
        assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
        assert_eq!(&bytes[8..12], b"WAVE", "not a WAVE file");

        let mut bits_per_sample: u16 = 0;
        let mut data: &[u8] = &[];

        let mut pos = 12;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = &bytes[pos + 8..pos + 8 + size];

            match id {
                b"fmt " => {
                    let format_tag = u16::from_le_bytes([body[0], body[1]]);
                    assert_eq!(format_tag, 1, "only PCM wav files are supported");
                    bits_per_sample = u16::from_le_bytes([body[14], body[15]]);
                }
                b"data" => data = body,
                _ => {}
            }

            pos += 8 + size + (size & 1); // chunks are word-aligned
        }

        let samples = match bits_per_sample {
            16 => data.chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                .collect(),
            24 => data.chunks_exact(3)
                .map(|b| {
                    let raw = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
                    let signed = (raw << 8) >> 8; // sign-extend 24 -> 32 bit
                    signed as f32 / 8_388_608.0 // 2^23
                })
                .collect(),
            other => panic!("unsupported wav bit depth: {other}"),
        };

        Sample { data: samples }
    }

    pub fn length (&self) -> usize {
        self.data.len()
    }

    pub fn at (&self, index: usize) -> f32 {
        self.data[index]
    }
}

#[derive(Clone)]
pub struct SamplePlayer {
    sample: Arc<Sample>,
    index:  usize,
}

impl SamplePlayer {
    pub fn new (sample: &Arc<Sample>) -> Self {
        Self { sample: sample.clone(), index: 0 }
    }
}

impl AudioNode for SamplePlayer {
    const ID: u64 = 9001;
    type Inputs = numeric_array::typenum::U0;
    type Outputs = numeric_array::typenum::U1;

    fn reset (&mut self) { self.index = 0; }

    #[inline]
    fn tick (&mut self, _input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        if self.index < self.sample.length() {
            let value = self.sample.at(self.index);
            self.index += 1;
            [value].into()
        } else {
            [0.0].into()
        }
    }

    fn route (&mut self, input: &SignalFrame, _frequency: f64) -> SignalFrame {
        Routing::Generator(0.0).route(input, self.outputs())
    }
}

pub fn play_sample (sample: &Arc<Sample>) -> An<SamplePlayer> {
    An(SamplePlayer::new(sample))
}
