use plume_core::AudioChunk;
use std::collections::VecDeque;
/// A first-speech gate. Pauses are forwarded after the first voiced window.
#[derive(Default)]
pub struct SpeechGate {
    pending: VecDeque<f32>,
    started: bool,
}
impl SpeechGate {
    pub fn push(&mut self, chunk: AudioChunk, speech: bool) -> Option<AudioChunk> {
        if self.started {
            return Some(chunk);
        }
        self.pending.extend(chunk.samples);
        if speech {
            self.started = true;
            Some(AudioChunk {
                samples: self.pending.drain(..).collect(),
                sample_rate: 16_000,
            })
        } else {
            while self.pending.len() > 4_000 {
                self.pending.pop_front();
            }
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_short_word_preroll_and_pauses() {
        let mut gate = SpeechGate::default();
        for _ in 0..20 {
            assert!(gate
                .push(
                    AudioChunk {
                        samples: vec![0.0; 512],
                        sample_rate: 16_000
                    },
                    false
                )
                .is_none());
        }
        let onset = gate
            .push(
                AudioChunk {
                    samples: vec![0.5; 512],
                    sample_rate: 16_000,
                },
                true,
            )
            .unwrap();
        assert_eq!(onset.samples.len(), 4_512);
        assert_eq!(onset.samples[4_000], 0.5);
        assert_eq!(
            gate.push(
                AudioChunk {
                    samples: vec![0.0; 512],
                    sample_rate: 16_000
                },
                false
            )
            .unwrap()
            .samples
            .len(),
            512
        );
    }
}
