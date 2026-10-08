use crate::decode::CHUNK_SAMPLES;
use plume_core::{AudioChunk, AudioStream, BoxError, CancellationToken, Normalizer};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AudioReadStatus(Arc<Mutex<Option<String>>>);
impl AudioReadStatus {
    pub fn error(&self) -> Option<String> {
        self.0.lock().unwrap().clone()
    }
    fn fail(&self, error: impl ToString) {
        *self.0.lock().unwrap() = Some(error.to_string());
    }
}
pub fn audio_from_wav(path: &Path) -> Result<AudioStream, BoxError> {
    audio_from_wav_with_control(path, CancellationToken::default()).map(|(audio, _)| audio)
}
pub fn audio_from_wav_with_control(
    path: &Path,
    cancel: CancellationToken,
) -> Result<(AudioStream, AudioReadStatus), BoxError> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err("invalid WAV format".into());
    }
    let raw: Box<dyn Iterator<Item = Result<f32, hound::Error>> + Send> = match spec.sample_format {
        hound::SampleFormat::Float => Box::new(reader.into_samples::<f32>()),
        hound::SampleFormat::Int => {
            if ![8, 16, 24, 32].contains(&spec.bits_per_sample) {
                return Err("unsupported WAV integer width".into());
            }
            let scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
            Box::new(
                reader
                    .into_samples::<i32>()
                    .map(move |s| s.map(|v| v as f32 / scale)),
            )
        }
    };
    let status = AudioReadStatus::default();
    let errors = status.clone();
    let mut samples = raw;
    let channels = usize::from(spec.channels);
    let mut done = false;
    let mut normalizer = Normalizer::default();
    let stream = Box::new(std::iter::from_fn(move || {
        if done || cancel.is_cancelled() {
            return None;
        }
        let mut mono = Vec::with_capacity(CHUNK_SAMPLES);
        for _ in 0..CHUNK_SAMPLES {
            if cancel.is_cancelled() {
                return None;
            }
            let mut total = 0.0;
            for channel in 0..channels {
                match samples.next() {
                    Some(Ok(s)) => total += s,
                    Some(Err(error)) => {
                        errors.fail(error);
                        done = true;
                        return None;
                    }
                    None => {
                        if channel != 0 {
                            errors.fail("incomplete WAV frame");
                        }
                        done = true;
                        break;
                    }
                }
            }
            if done {
                break;
            }
            mono.push(total / channels as f32);
        }
        let mut chunk = match normalizer.push(AudioChunk {
            samples: mono,
            sample_rate: spec.sample_rate,
        }) {
            Ok(c) => c,
            Err(e) => {
                errors.fail(e);
                done = true;
                return None;
            }
        };
        if done {
            chunk.samples.extend(normalizer.finish().samples);
        }
        if chunk.samples.is_empty() {
            None
        } else {
            Some(chunk)
        }
    }));
    Ok((stream, status))
}

/// Replayed recordings use the same first-speech gate as live capture. Initialize
/// Silero inside the decoder worker, keeping shortcut consumption responsive.
pub fn gate_recording(
    audio: AudioStream,
    path: std::path::PathBuf,
    cancel: CancellationToken,
    status: AudioReadStatus,
) -> AudioStream {
    let mut source = audio;
    let mut vad = None;
    let mut gate = crate::SpeechGate::default();
    let mut pending = std::collections::VecDeque::new();
    let mut done = false;
    Box::new(std::iter::from_fn(move || {
        if done || cancel.is_cancelled() {
            return None;
        }
        if vad.is_none() {
            match crate::SpeechDetector::open(&path) {
                Ok(detector) => vad = Some(detector),
                Err(e) => {
                    status.fail(e);
                    done = true;
                    return None;
                }
            }
        }
        loop {
            if cancel.is_cancelled() {
                return None;
            }
            while pending.len() < 512 && !done {
                match source.next() {
                    Some(chunk) => pending.extend(chunk.samples),
                    None => done = true,
                }
            }
            if pending.is_empty() {
                return None;
            }
            let len = pending.len().min(512);
            let frame: Vec<_> = pending.drain(..len).collect();
            let vad = vad.as_mut().unwrap();
            let speech = vad.push(&frame).and_then(|speech| {
                if len < 512 {
                    vad.finish().map(|tail| speech || tail)
                } else {
                    Ok(speech)
                }
            });
            let speech = match speech {
                Ok(speech) => speech,
                Err(e) => {
                    status.fail(e);
                    done = true;
                    return None;
                }
            };
            if let Some(chunk) = gate.push(
                AudioChunk {
                    samples: frame,
                    sample_rate: 16_000,
                },
                speech,
            ) {
                return Some(chunk);
            }
        }
    }))
}

#[cfg(test)]
mod read_tests {
    use super::*;
    #[test]
    fn buffered_read_reports_truncation_and_obeys_cancellation() {
        let path = std::env::temp_dir().join(format!(
            "plume-wav-{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..32_000 {
            writer.write_sample(100i16).unwrap();
        }
        writer.finalize().unwrap();
        let token = CancellationToken::default();
        let (mut audio, status) = audio_from_wav_with_control(&path, token.clone()).unwrap();
        token.cancel();
        assert!(audio.next().is_none());
        assert!(status.error().is_none());
        let (audio, status) =
            audio_from_wav_with_control(&path, CancellationToken::default()).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(8192)
            .unwrap();
        let _ = audio.collect::<Vec<_>>();
        assert!(status.error().is_some());
        std::fs::remove_file(path).unwrap();
    }
}
