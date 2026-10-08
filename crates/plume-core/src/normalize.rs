use crate::{AudioChunk, BoxError};

/// Keeps interpolation phase and the boundary sample across device callbacks.
#[derive(Default)]
pub struct Normalizer {
    rate: u32,
    pending: Vec<f32>,
    position: u64,
}
impl Normalizer {
    pub fn push(&mut self, chunk: AudioChunk) -> Result<AudioChunk, BoxError> {
        if chunk.sample_rate == 0 {
            return Err("microphone sample rate is zero".into());
        }
        if self.rate != 0 && self.rate != chunk.sample_rate {
            return Err("microphone sample rate changed during recording".into());
        }
        self.rate = chunk.sample_rate;
        if self.rate == 16_000 {
            return Ok(chunk);
        }
        self.pending.extend(chunk.samples);
        let step = u64::from(self.rate);
        let mut samples = Vec::new();
        while (self.position / 16_000) as usize + 1 < self.pending.len() {
            let index = (self.position / 16_000) as usize;
            let frac = (self.position % 16_000) as f32 / 16_000.0;
            samples.push(self.pending[index] * (1.0 - frac) + self.pending[index + 1] * frac);
            self.position += step;
        }
        let consumed =
            ((self.position / 16_000) as usize).min(self.pending.len().saturating_sub(1));
        self.pending.drain(..consumed);
        self.position -= consumed as u64 * 16_000;
        Ok(AudioChunk {
            samples,
            sample_rate: 16_000,
        })
    }
    pub fn finish(&mut self) -> AudioChunk {
        let mut samples = Vec::new();
        let step = u64::from(self.rate.max(1));
        while self.position < self.pending.len() as u64 * 16_000 {
            samples.push(self.pending[(self.position / 16_000) as usize]);
            self.position += step;
        }
        self.pending.clear();
        AudioChunk {
            samples,
            sample_rate: 16_000,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampling_is_independent_of_callback_boundaries() {
        for rate in [44_100, 48_000, 8_000] {
            let input: Vec<_> = (0..rate).map(|i| (i as f32 / 100.0).sin()).collect();
            let convert = |size| {
                let mut n = Normalizer::default();
                let mut out = Vec::new();
                for samples in input.chunks(size) {
                    out.extend(
                        n.push(AudioChunk {
                            samples: samples.to_vec(),
                            sample_rate: rate,
                        })
                        .unwrap()
                        .samples,
                    );
                }
                out.extend(n.finish().samples);
                out
            };
            assert_eq!(convert(137), convert(input.len()));
            assert_eq!(convert(137).len(), 16_000);
        }
    }
}
