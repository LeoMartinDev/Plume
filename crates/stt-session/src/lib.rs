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

/// Uninhabited: `run` never returns `Ok`.
pub enum Never {}

/// Own the whole topology. Spawns the compositor worker, then runs the
/// GPUI overlay on the CALLER's thread (must be main). Startup order is
/// load-bearing: model -> engine -> injector -> hold chord; the window
/// opens LAST so a broken backend fails on stderr, not in a dead bubble.
pub fn run(config: Config) -> Result<Never, StartupError> {
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
    std::process::exit(0);
}
