use stt_core::AudioChunk;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputBuffer<'a> {
    F32(&'a [f32]),
    I16(&'a [i16]),
    U16(&'a [u16]),
}

pub fn to_audio_chunk(buffer: InputBuffer<'_>, sample_rate: u32, channels: u16) -> AudioChunk {
    let channels = usize::from(channels.max(1));
    let samples = match buffer {
        InputBuffer::F32(frames) => downmix(frames.iter().copied(), channels),
        InputBuffer::I16(frames) => {
            downmix(frames.iter().map(|&s| f32::from(s) / 32768.0), channels)
        }
        InputBuffer::U16(frames) => downmix(
            frames.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0),
            channels,
        ),
    };
    AudioChunk {
        samples,
        sample_rate,
    }
}

fn downmix(samples: impl IntoIterator<Item = f32>, channels: usize) -> Vec<f32> {
    let samples: Vec<f32> = samples.into_iter().collect();
    if channels == 1 {
        return samples;
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{to_audio_chunk, InputBuffer};
    use stt_core::AudioChunk;

    #[test]
    fn f32_mono_keeps_samples_and_native_rate() {
        let chunk = to_audio_chunk(InputBuffer::F32(&[0.0, 0.5, -1.0]), 48_000, 1);
        assert_eq!(
            chunk,
            AudioChunk {
                samples: vec![0.0, 0.5, -1.0],
                sample_rate: 48_000,
            }
        );
    }

    #[test]
    fn i16_mono_scales_by_32768() {
        let chunk = to_audio_chunk(InputBuffer::I16(&[0, 32767, -32768]), 16_000, 1);
        assert_eq!(
            chunk,
            AudioChunk {
                samples: vec![0.0, 32767.0 / 32768.0, -1.0],
                sample_rate: 16_000,
            }
        );
    }

    #[test]
    fn u16_mono_centers_on_32768() {
        let chunk = to_audio_chunk(InputBuffer::U16(&[0, 32768, 65535]), 44_100, 1);
        assert_eq!(
            chunk,
            AudioChunk {
                samples: vec![-1.0, 0.0, 32767.0 / 32768.0],
                sample_rate: 44_100,
            }
        );
    }

    #[test]
    fn stereo_f32_averages_each_frame() {
        let chunk = to_audio_chunk(InputBuffer::F32(&[0.0, 1.0, 0.5, 0.5]), 48_000, 2);
        assert_eq!(
            chunk,
            AudioChunk {
                samples: vec![0.5, 0.5],
                sample_rate: 48_000,
            }
        );
    }

    #[test]
    fn empty_buffer_keeps_the_native_rate() {
        let chunk = to_audio_chunk(InputBuffer::F32(&[]), 16_000, 1);
        assert_eq!(
            chunk,
            AudioChunk {
                samples: vec![],
                sample_rate: 16_000,
            }
        );
    }
}
