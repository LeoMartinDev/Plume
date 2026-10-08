mod decode;
mod model;
mod wav;

pub use model::{Engine, Language, LanguageTarget, ModelDir};
use plume_core::{
    AsrEngine, AudioStream, BoxError, CancellationToken, Hypothesis, HypothesisStream, Transcript,
};
use std::sync::atomic::Ordering;
pub use wav::{audio_from_wav, audio_from_wav_with_control, gate_recording, AudioReadStatus};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

impl AsrEngine for Engine {
    fn snapshot(&self) -> Self {
        Engine::snapshot(self)
    }
    fn stream(&self, audio: AudioStream) -> HypothesisStream {
        self.stream_with_control(audio, CancellationToken::default())
    }

    fn stream_with_control(
        &self,
        audio: AudioStream,
        cancel: CancellationToken,
    ) -> HypothesisStream {
        let language = self.language.load(Ordering::Relaxed);
        match &self.backend {
            model::EngineBackend::Nemotron(inner) => {
                match decode::NemotronStream::controlled(inner.clone(), audio, language, cancel) {
                    Ok(stream) => Box::new(stream),
                    Err(error) => Box::new(std::iter::once(Err(error))),
                }
            }
            model::EngineBackend::Whisper(context) => {
                let context = context.clone();
                Box::new(std::iter::once_with(move || {
                    transcribe_whisper(&context, audio, language, cancel)
                        .map(|text| Hypothesis::Final(Transcript { text }))
                }))
            }
        }
    }
}

fn transcribe_whisper(
    context: &WhisperContext,
    audio: AudioStream,
    language: i64,
    cancel: CancellationToken,
) -> Result<String, BoxError> {
    let samples = collect_whisper_samples(Box::new(audio.take_while({
        let cancel = cancel.clone();
        move |_| !cancel.is_cancelled()
    })))?;
    if cancel.is_cancelled() {
        return Err("transcription cancelled".into());
    }
    if samples.is_empty() {
        return Ok(String::new());
    }

    let mut state = context.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let language = match language {
        8 => Some("fr"),
        0 => Some("en"),
        _ => None,
    };
    params.set_language(language);
    params.set_translate(false);
    params.set_no_context(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    unsafe extern "C" fn abort(data: *mut std::ffi::c_void) -> bool {
        // The token lives until the synchronous full() call returns.
        unsafe { (*(data as *const CancellationToken)).is_cancelled() }
    }
    unsafe {
        params.set_abort_callback(Some(abort));
        params
            .set_abort_callback_user_data((&cancel as *const CancellationToken).cast_mut().cast());
    }
    state.full(params, &samples)?;

    let count = state.full_n_segments();
    let mut text = String::new();
    for index in 0..count {
        if cancel.is_cancelled() {
            return Err("transcription cancelled".into());
        }
        let segment = state
            .get_segment(index)
            .ok_or_else(|| format!("whisper segment {index} is missing"))?;
        text.push_str(segment.to_str()?);
    }
    Ok(text.trim().to_string())
}

/// Whisper consumes 16 kHz PCM. Audio inputs commonly run at 44.1 or 48 kHz,
/// so normalise here instead of aborting the live session on its first buffer.
fn collect_whisper_samples(audio: AudioStream) -> Result<Vec<f32>, BoxError> {
    let mut samples = Vec::new();
    for chunk in audio {
        if chunk.sample_rate == 0 {
            return Err("whisper received audio with sample rate 0".into());
        }
        samples.extend(decode::resample_to_16k(&chunk.samples, chunk.sample_rate));
    }
    Ok(samples)
}

#[cfg(test)]
mod whisper_tests {
    use super::*;
    use plume_core::AudioChunk;

    #[test]
    fn whisper_resamples_a_48khz_microphone_stream() {
        let audio: AudioStream = Box::new(std::iter::once(AudioChunk {
            samples: vec![0.25; 480],
            sample_rate: 48_000,
        }));
        let samples = collect_whisper_samples(audio).expect("48 kHz audio is supported");
        assert_eq!(samples.len(), 160);
        assert!(samples.iter().all(|sample| (*sample - 0.25).abs() < 1e-6));
    }

    #[test]
    #[ignore]
    fn downloaded_whisper_model_transcribes_the_english_fixture() {
        let model = std::env::var("PLUME_WHISPER_MODEL").expect("PLUME_WHISPER_MODEL");
        let engine = Engine::open_whisper(model).expect("open Whisper model");
        engine.set_language(Language::English);
        let frozen = engine.snapshot();
        engine.language_target().set(Language::French);
        assert_eq!(frozen.language.load(Ordering::Relaxed), 0);
        assert_eq!(engine.language.load(Ordering::Relaxed), 8);
        let wav = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("en-hello.wav");
        let audio = audio_from_wav(&wav).expect("load fixture");
        let final_text = frozen
            .stream(audio)
            .find_map(|item| match item.expect("Whisper decode") {
                Hypothesis::Final(transcript) => Some(transcript.text),
                Hypothesis::Partial(_) => None,
            })
            .expect("final transcript");
        let normalized = final_text.to_ascii_lowercase();
        assert!(
            normalized.contains("pronunciation") && normalized.contains("written"),
            "unexpected transcript: {final_text:?}"
        );
    }

    #[test]
    #[ignore = "requires PLUME_WHISPER_MODEL pointing to a multilingual Whisper model"]
    fn downloaded_whisper_model_transcribes_the_french_fixture() {
        let model = std::env::var("PLUME_WHISPER_MODEL").expect("PLUME_WHISPER_MODEL");
        let engine = Engine::open_whisper(model).expect("open Whisper model");
        engine.set_language(Language::French);
        let wav = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/fr-bonjour.wav");
        let audio = audio_from_wav(&wav).expect("load fixture");
        let final_text = engine
            .stream(audio)
            .find_map(|item| match item.expect("Whisper decode") {
                Hypothesis::Final(transcript) => Some(transcript.text),
                Hypothesis::Partial(_) => None,
            })
            .expect("final transcript");
        let normalized = final_text.to_lowercase();
        assert!(
            normalized.contains("maladie") && normalized.contains("juillet"),
            "unexpected transcript: {final_text:?}"
        );
    }
}

mod vad;
pub use vad::SpeechDetector;

mod gate;
pub use gate::SpeechGate;
