use std::path::PathBuf;
use std::time::Instant;

use stt_core::{AsrEngine, Dictation};
use stt_engine::{audio_from_wav, Engine, ModelDir};

const PACK_SHA: &str = "8364d9e2dd9da23789b480bdbba9e423717e42ee";

fn transcribe(name: &str) -> Option<(String, u128, String)> {
    let dir = match std::env::var_os("STT_MODEL_DIR") {
        Some(p) => ModelDir::open(p).expect("STT_MODEL_DIR must be a valid pack"),
        None => {
            eprintln!("skip: set STT_MODEL_DIR to run fixture_transcribe");
            return None;
        }
    };
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let expected = std::fs::read_to_string(base.join(format!("{name}.expected.txt"))).unwrap();
    let started = Instant::now();
    let engine = Engine::open(dir).unwrap();
    let audio = audio_from_wav(&base.join(format!("{name}.wav"))).unwrap();
    let mut dictation = Dictation::new();
    dictation.hold();
    let mut transcript = None;
    for item in engine.stream(audio) {
        let step = dictation.on_hypothesis(item.unwrap()).unwrap();
        if let Some(t) = step.transcript {
            transcript = Some(t.text);
        }
    }
    let ms = started.elapsed().as_millis();
    eprintln!("{name}: latency_ms={ms} pack={PACK_SHA}");
    Some((
        transcript.expect("engine produced no final transcript"),
        ms,
        expected,
    ))
}

fn check_fixture(name: &str) {
    let Some((text, ms, expected)) = transcribe(name) else {
        return;
    };
    assert_eq!(text, expected.trim_end());
    assert!(
        ms < 5_000,
        "short fixture must decode under 5s on CPU, got {ms}ms"
    );
}

// FLEURS fr_fr validation offset 191 id 1587, byte-identical audio. The
// dataset raw ends with a period. The INT4 streaming model withholds
// end-of-stream punctuation (measured over 4158 French rows plus a
// doubled-clip mechanism experiment: mid-stream periods appear, the final
// one never triggers). The literal below pins actual model behavior. If a
// future model emits the period, update the literal consciously.
#[test]
fn french_bonjour_matches_checked_in_transcript() {
    check_fixture("fr-bonjour");
}

#[test]
fn english_hello_matches_checked_in_transcript() {
    check_fixture("en-hello");
}
