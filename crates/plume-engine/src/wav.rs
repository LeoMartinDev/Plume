use std::path::Path;

use plume_core::{AudioChunk, AudioStream, BoxError};

use crate::decode::resample_to_16k;
use crate::decode::CHUNK_SAMPLES;

pub fn audio_from_wav(path: &Path) -> Result<AudioStream, BoxError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err("wav has no channels".into());
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => match spec.bits_per_sample {
            16 => reader
                .samples::<i16>()
                .map(|s| s.map(|s| s as f32 / 32768.0))
                .collect::<Result<Vec<_>, _>>()?,
            24 | 32 => {
                let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|s| s as f32 / scale))
                    .collect::<Result<Vec<_>, _>>()?
            }
            bits => return Err(format!("wav integer width {bits} is not supported").into()),
        },
    };
    if samples.is_empty() {
        return Err("wav has no samples".into());
    }
    let mono: Vec<f32> = if spec.channels == 1 {
        samples
    } else {
        samples
            .chunks(spec.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / spec.channels as f32)
            .collect()
    };
    let pcm = resample_to_16k(&mono, spec.sample_rate);
    let chunks: Vec<AudioChunk> = pcm
        .chunks(CHUNK_SAMPLES)
        .map(|window| AudioChunk {
            samples: window.to_vec(),
            sample_rate: 16_000,
        })
        .collect();
    Ok(Box::new(chunks.into_iter()))
}
