#[derive(Clone, Debug, PartialEq)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioChunk {
    /// Loudness in `0..=1`. Full scale is an RMS of 1/16. Silence is 0.
    pub fn voice_level(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum_sq = self
            .samples
            .iter()
            .map(|sample| sample * sample)
            .sum::<f32>();
        let rms = (sum_sq / self.samples.len() as f32).sqrt();
        (rms / 0.0625).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_level_tracks_rms_and_clamps() {
        let silent = AudioChunk {
            samples: vec![0.0, 0.0, 0.0],
            sample_rate: 16_000,
        };
        assert_eq!(silent.voice_level(), 0.0);

        let empty = AudioChunk {
            samples: vec![],
            sample_rate: 16_000,
        };
        assert_eq!(empty.voice_level(), 0.0);

        let half = AudioChunk {
            samples: vec![0.03125, -0.03125],
            sample_rate: 16_000,
        };
        assert_eq!(half.voice_level(), 0.5);

        let full = AudioChunk {
            samples: vec![0.0625, 0.0625],
            sample_rate: 16_000,
        };
        assert_eq!(full.voice_level(), 1.0);

        let clipped = AudioChunk {
            samples: vec![1.0, -1.0],
            sample_rate: 16_000,
        };
        assert_eq!(clipped.voice_level(), 1.0);
    }
}
