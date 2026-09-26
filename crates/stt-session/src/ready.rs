use std::sync::mpsc;

use stt_engine::Engine;
use stt_inject::NativeInjector;
use stt_overlay::Bubble;

use crate::chords::Chord;
use crate::config::Config;
use crate::drive::{BubbleSink, Idle};
use crate::target::Target;
use crate::StartupError;

/// Proven pack plus parsed chords. The only value `start` accepts.
/// There is no constructor that omits `Engine`.
pub struct Ready {
    config: Config,
    engine: Engine,
}

impl Ready {
    pub fn from_open(config: Config, engine: Engine) -> Self {
        Ready { config, engine }
    }
}

/// Detached compositor plus the overlay's incoming snapshots.
/// Dropping this does not stop Idle. The process drop does, as today.
pub struct LiveSession {
    pub bubbles: mpsc::Receiver<Bubble>,
    pub levels: mpsc::Receiver<f32>,
}

/// Injector, hold bind, spawn Idle. Returns the Bubble receiver.
/// Does not open a window. Does not read prefs. Does not load a pack.
/// Call only after prepare_display. Bind then sees the forced X11 env.
pub fn start(ready: Ready) -> Result<LiveSession, StartupError> {
    let Ready { config, engine } = ready;
    let injector = NativeInjector::connect().map_err(StartupError::backend)?;
    let hold = Chord::bind(&config.hold).map_err(|err| {
        StartupError::backend(format!("hold chord {}: {err}", config.hold.as_str()).into())
    })?;
    let (bubble_tx, bubble_rx) = mpsc::channel();
    let (level_tx, level_rx) = mpsc::sync_channel(8);
    let worker = Idle::new(
        hold,
        config.cancel,
        engine,
        Target::new(injector),
        BubbleSink::new(bubble_tx),
        level_tx,
    );
    std::thread::Builder::new()
        .name("stt-compositor".to_string())
        .spawn(|| worker.run())
        .map_err(|err| StartupError::backend(err.into()))?;
    Ok(LiveSession {
        bubbles: bubble_rx,
        levels: level_rx,
    })
}
