use std::path::PathBuf;
use std::time::Instant;

use stt_core::{AsrEngine, AudioStream, BoxError, Dictation};
use stt_engine::{audio_from_wav, Engine, ModelDir};

pub(crate) const USAGE: &str = "usage: stt-shell transcribe <wav> [--model-dir <dir>]";

struct Request {
    wav: PathBuf,
    model_dir: PathBuf,
}

enum ParseError {
    Usage,
    MissingModelDir,
}

pub fn run(args: &[String]) -> i32 {
    let request = match parse(args) {
        Ok(request) => request,
        Err(ParseError::Usage) => {
            eprintln!("{USAGE}");
            return 2;
        }
        Err(ParseError::MissingModelDir) => {
            eprintln!("model dir missing: pass --model-dir or set STT_MODEL_DIR");
            return 4;
        }
    };

    let model = match ModelDir::open(&request.model_dir) {
        Ok(model) => model,
        Err(err) => return fail(4, err),
    };
    let audio = match audio_from_wav(&request.wav) {
        Ok(audio) => audio,
        Err(err) => return fail(3, err),
    };
    let engine = match Engine::open(model) {
        Ok(engine) => engine,
        Err(err) => return fail(1, err),
    };
    match decode(&engine, audio) {
        Ok((text, ms)) => {
            println!("{text}");
            eprintln!("latency_ms={ms}");
            0
        }
        Err(err) => fail(1, err),
    }
}

fn parse(args: &[String]) -> Result<Request, ParseError> {
    let mut wav = None;
    let mut model_dir = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--model-dir" {
            i += 1;
            let dir = args.get(i).ok_or(ParseError::Usage)?;
            if model_dir.is_some() {
                return Err(ParseError::Usage);
            }
            model_dir = Some(PathBuf::from(dir));
        } else if arg.starts_with('-') || wav.is_some() {
            return Err(ParseError::Usage);
        } else {
            wav = Some(PathBuf::from(arg));
        }
        i += 1;
    }
    let wav = wav.ok_or(ParseError::Usage)?;
    let model_dir = match model_dir {
        Some(dir) => dir,
        None => std::env::var_os("STT_MODEL_DIR")
            .map(PathBuf::from)
            .ok_or(ParseError::MissingModelDir)?,
    };
    Ok(Request { wav, model_dir })
}

fn decode(engine: &Engine, audio: AudioStream) -> Result<(String, u128), BoxError> {
    let mut dictation = Dictation::new();
    dictation.hold();
    let started = Instant::now();
    let mut transcript = None;
    for item in engine.stream(audio) {
        let step = dictation.on_hypothesis(item?)?;
        if let Some(t) = step.transcript {
            transcript = Some(t);
        }
    }
    let ms = started.elapsed().as_millis();
    match transcript {
        Some(t) => Ok((t.text, ms)),
        None => Err("engine produced no final transcript".into()),
    }
}

fn fail(code: i32, err: BoxError) -> i32 {
    eprintln!("{err}");
    code
}
