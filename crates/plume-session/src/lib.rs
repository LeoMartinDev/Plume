mod capture;
mod controller;
mod recordings;
pub use recordings::{RecordId, Recording, RecordingStore, SharedRecordings, MAX_RECORDINGS};
mod chords;
mod config;
mod decoder;
mod delivery;
mod destination;
mod preview;
mod runtime;
mod startup;

use plume_core::BoxError;
use plume_engine::{Engine, ModelDir};

pub use config::{Config, ConfigError};
pub use plume_core::{
    Destination, InjectionReport, InsertionMethod, InsertionMode, TargetAssessment,
};
pub use startup::{
    start, DictationResult, EngineTarget, HoldTarget, InsertionConfig, InsertionTarget,
    LiveSession, PreparedSession, PreviewEvent, SessionControl, SessionMode, ShortcutEditGuard,
};

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

/// plume-session binary only. Opens the engine on this thread, start(), run_with().
/// Owns Application::run. plume-app never calls this.
pub fn run(config: Config) -> Result<(), StartupError> {
    let dir = ModelDir::open(config.model_dir()).map_err(StartupError::model)?;
    let engine = Engine::open(dir).map_err(StartupError::model)?;
    let live = start(PreparedSession::from_open(config, engine))?;
    plume_overlay::run_with(live.bubbles, live.levels);
    Ok(())
}
