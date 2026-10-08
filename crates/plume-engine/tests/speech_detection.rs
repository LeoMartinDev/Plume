use plume_core::{AsrEngine, AudioChunk, CancellationToken, Hypothesis};
use plume_engine::{
    audio_from_wav, audio_from_wav_with_control, gate_recording, Engine, SpeechDetector,
};
use std::path::PathBuf;
fn vad_path() -> Option<PathBuf> {
    std::env::var_os("PLUME_VAD_PATH").map(PathBuf::from)
}
#[test]
fn real_silero_silence_noise_and_short_fixture_speech() {
    let Some(path) = vad_path() else {
        eprintln!("set PLUME_VAD_PATH to validate native Silero fixtures");
        return;
    };
    let mut silence = SpeechDetector::open(&path).unwrap();
    for _ in 0..100 {
        assert!(!silence.push(&[0.0; 512]).unwrap());
    }
    assert!(!silence.finish().unwrap());
    let mut noise = SpeechDetector::open(&path).unwrap();
    let mut seed = 17u32;
    for _ in 0..100 {
        let samples: Vec<_> = (0..512)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                ((seed >> 16) as f32 / 65535.0 - 0.5) * 0.02
            })
            .collect();
        assert!(!noise.push(&samples).unwrap());
    }
    for name in ["en-hello", "fr-bonjour"] {
        let mut detector = SpeechDetector::open(&path).unwrap();
        let mut voiced = false;
        let audio = audio_from_wav(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/{name}.wav")),
        )
        .unwrap();
        for chunk in audio {
            for window in chunk.samples.chunks(512) {
                voiced |= detector.push(window).unwrap();
            }
        }
        voiced |= detector.finish().unwrap();
        assert!(voiced, "Silero rejected fixture {name}");
    }
}
#[test]
fn real_whisper_gated_recording_and_native_abort() {
    let (Some(vad), Some(model)) = (vad_path(), std::env::var_os("PLUME_WHISPER_PATH")) else {
        eprintln!("set PLUME_VAD_PATH and PLUME_WHISPER_PATH for native Whisper validation");
        return;
    };
    let engine = Engine::open_whisper(model).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/en-hello.wav");
    let token = CancellationToken::default();
    let (audio, status) = audio_from_wav_with_control(&source, token.clone()).unwrap();
    let audio = gate_recording(audio, vad, token.clone(), status.clone());
    let text = engine
        .stream_with_control(audio, token)
        .find_map(|h| match h.unwrap() {
            Hypothesis::Final(t) => Some(t.text),
            _ => None,
        })
        .unwrap();
    assert!(!text.trim().is_empty());
    assert!(status.error().is_none());
    // Cancel during a long native full() call, not merely before submission.
    let token = CancellationToken::default();
    let stop = token.clone();
    let engine = engine.snapshot();
    let audio = Box::new(std::iter::once(AudioChunk {
        samples: vec![0.1; 16_000 * 120],
        sample_rate: 16_000,
    }));
    let began = std::time::Instant::now();
    let worker = std::thread::spawn(move || {
        engine
            .stream_with_control(audio, stop)
            .take(1024)
            .collect::<Vec<_>>()
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    token.cancel();
    let output = worker.join().unwrap();
    assert!(output.len() < 1024, "cancelled iterator must terminate");
    assert!(output.iter().any(Result::is_err));
    assert!(
        began.elapsed() < std::time::Duration::from_secs(10),
        "Whisper did not abort promptly"
    );
}

#[test]
fn real_nemotron_native_abort() {
    let Some(model) = std::env::var_os("PLUME_MODEL_DIR") else {
        eprintln!("set PLUME_MODEL_DIR for native Nemotron cancellation");
        return;
    };
    let engine = Engine::open(plume_engine::ModelDir::open(model).unwrap()).unwrap();
    let token = CancellationToken::default();
    let stop = token.clone();
    let audio = Box::new(std::iter::once(AudioChunk {
        samples: vec![0.1; 16_000 * 120],
        sample_rate: 16_000,
    }));
    let began = std::time::Instant::now();
    let worker = std::thread::spawn(move || {
        engine
            .stream_with_control(audio, stop)
            .take(1024)
            .collect::<Vec<_>>()
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    token.cancel();
    let output = worker.join().unwrap();
    assert!(output.len() < 1024, "cancelled iterator must terminate");
    assert!(output.iter().any(Result::is_err));
    assert!(
        began.elapsed() < std::time::Duration::from_secs(10),
        "Nemotron did not abort promptly"
    );
}

#[test]
fn real_nemotron_with_the_shared_speech_gate() {
    let (Some(model), Some(vad)) = (std::env::var_os("PLUME_MODEL_DIR"), vad_path()) else {
        eprintln!("set PLUME_MODEL_DIR and PLUME_VAD_PATH for gated Nemotron fixtures");
        return;
    };
    let engine = Engine::open(plume_engine::ModelDir::open(model).unwrap()).unwrap();
    for name in ["en-hello", "fr-bonjour"] {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/{name}.wav"));
        let token = CancellationToken::default();
        let (audio, status) = audio_from_wav_with_control(&source, token.clone()).unwrap();
        let audio = gate_recording(audio, vad.clone(), token.clone(), status.clone());
        let text = engine
            .stream_with_control(audio, token)
            .find_map(|h| match h.unwrap() {
                Hypothesis::Final(t) => Some(t.text),
                _ => None,
            })
            .unwrap();
        assert!(status.error().is_none());
        let expected = std::fs::read_to_string(source.with_extension("expected.txt")).unwrap();
        // Trimming initial silence shifts decoder chunk boundaries, so model
        // punctuation may differ. Every spoken word, including the onset, must remain.
        let words = |text: &str| {
            text.split_whitespace()
                .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(words(&text), words(expected.trim_end()));
    }
}
