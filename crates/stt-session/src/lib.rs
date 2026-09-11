mod capture;
mod chords;
mod config;
mod drive;
mod target;

use std::sync::mpsc;

use stt_core::BoxError;
use stt_engine::{Engine, ModelDir};
use stt_inject::NativeInjector;

use crate::chords::Chord;
use crate::drive::{BubbleSink, Idle};
use crate::target::Target;

pub use config::{Config, ConfigError};

/// Startup failure with its process exit code: 4 for a missing or invalid
/// model pack (matches transcribe), 1 for injector, hotkey, or thread setup.
#[derive(Debug)]
pub struct StartupError {
    code: i32,
    source: BoxError,
}

impl StartupError {
    pub(crate) fn model(source: BoxError) -> Self {
        StartupError { code: 4, source }
    }

    pub(crate) fn backend(source: BoxError) -> Self {
        StartupError { code: 1, source }
    }

    pub fn exit_code(&self) -> i32 {
        self.code
    }
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for StartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Spawn the compositor worker, then run the GPUI overlay on the caller
/// thread, which must be main. Load the model, engine, injector, and hold
/// chord before the window opens. Returns when the overlay window closes.
pub fn run(config: Config) -> Result<(), StartupError> {
    let dir = ModelDir::open(&config.model_dir).map_err(StartupError::model)?;
    let engine = Engine::open(dir).map_err(StartupError::model)?;
    let injector = NativeInjector::connect().map_err(StartupError::backend)?;
    let hold = Chord::bind(&config.hold).map_err(|err| {
        StartupError::backend(format!("hold chord {}: {err}", config.hold.as_str()).into())
    })?;
    let (bubble_tx, bubble_rx) = mpsc::channel();
    let worker = Idle::new(
        hold,
        config.cancel,
        engine,
        Target::new(injector),
        BubbleSink::new(bubble_tx),
    );
    std::thread::Builder::new()
        .name("stt-compositor".to_string())
        .spawn(|| worker.run())
        .map_err(|err| StartupError::backend(err.into()))?;
    stt_overlay::run_with(bubble_rx);
    Ok(())
}
