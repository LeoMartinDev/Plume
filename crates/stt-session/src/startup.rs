use std::sync::mpsc;

use stt_engine::Engine;
use stt_inject::NativeInjector;
use stt_overlay::Bubble;

use crate::chords::Chord;
use crate::config::Config;
use crate::decoder::Decoder;
use crate::delivery::TranscriptDelivery;
use crate::runtime::{BubbleSink, RuntimeConfig, RuntimeUpdates, SessionOutputs, SessionRuntime};
use crate::StartupError;

const VOICE_LEVEL_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertionConfig {
    pub mode: stt_core::InsertionMode,
    pub copy_on_failure: bool,
}

impl Default for InsertionConfig {
    fn default() -> Self {
        Self {
            mode: stt_core::InsertionMode::Auto,
            copy_on_failure: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DictationResult {
    pub text: String,
    pub injection: Result<stt_core::InjectionReport, String>,
    pub copied_on_failure: bool,
}

/// Proven pack plus parsed chords. The only value `start` accepts.
/// There is no constructor that omits `Engine`.
pub struct PreparedSession {
    config: Config,
    engine: Engine,
    insertion: InsertionConfig,
}

impl PreparedSession {
    pub fn from_open(config: Config, engine: Engine) -> Self {
        PreparedSession {
            config,
            engine,
            insertion: InsertionConfig::default(),
        }
    }

    pub fn with_insertion(mut self, insertion: InsertionConfig) -> Self {
        self.insertion = insertion;
        self
    }
}

/// Sends a new hold chord to the running compositor. The listener swaps
/// hooks between sessions, so a settings change applies without a restart.
#[derive(Clone)]
pub struct HoldTarget {
    tx: mpsc::Sender<String>,
}

impl HoldTarget {
    pub fn set(&self, hold: &str) {
        let _ = self.tx.send(hold.to_string());
    }
}

/// Detached compositor plus the overlay's incoming snapshots.
/// Dropping this does not stop SessionRuntime. The process drop does, as today.
pub struct LiveSession {
    pub bubbles: mpsc::Receiver<Bubble>,
    pub levels: mpsc::Receiver<f32>,
    pub hold: HoldTarget,
    pub engine: EngineTarget,
    pub insertion: InsertionTarget,
    pub results: mpsc::Receiver<DictationResult>,
}

#[derive(Clone)]
pub struct InsertionTarget {
    tx: mpsc::Sender<InsertionConfig>,
}

impl InsertionTarget {
    pub fn set(&self, config: InsertionConfig) {
        let _ = self.tx.send(config);
    }
}

#[derive(Clone)]
pub struct EngineTarget {
    tx: mpsc::Sender<Engine>,
}

impl EngineTarget {
    pub fn set(&self, engine: Engine) {
        let _ = self.tx.send(engine);
    }
}

/// Injector, hold bind, spawn SessionRuntime. Returns the Bubble receiver.
/// Does not open a window. Does not read prefs. Does not load a pack.
/// Call only after prepare_display. Bind then sees the forced X11 env.
pub fn start(ready: PreparedSession) -> Result<LiveSession, StartupError> {
    let PreparedSession {
        config,
        engine,
        insertion,
    } = ready;
    let injector = NativeInjector::connect().map_err(StartupError::backend)?;
    let hold = Chord::bind(&config.hold).map_err(|err| {
        StartupError::backend(format!("hold chord {}: {err}", config.hold.as_str()).into())
    })?;
    let (bubble_tx, bubble_rx) = mpsc::channel();
    let (level_tx, level_rx) = mpsc::sync_channel(VOICE_LEVEL_QUEUE_CAPACITY);
    let (hold_tx, hold_rx) = mpsc::channel();
    let (engine_tx, engine_rx) = mpsc::channel();
    let (insertion_tx, insertion_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let decoder = Decoder::start().map_err(|err| StartupError::backend(err.into()))?;
    let delivery = TranscriptDelivery::new(injector, result_tx);
    let worker = SessionRuntime::new(
        RuntimeConfig {
            session: config,
            hold,
            engine,
            insertion,
        },
        RuntimeUpdates {
            hold: hold_rx,
            engine: engine_rx,
            insertion: insertion_rx,
        },
        SessionOutputs {
            bubbles: BubbleSink::new(bubble_tx),
            levels: level_tx,
        },
        decoder,
        delivery,
    );
    std::thread::Builder::new()
        .name("stt-compositor".to_string())
        .spawn(|| worker.run())
        .map_err(|err| StartupError::backend(err.into()))?;
    Ok(LiveSession {
        bubbles: bubble_rx,
        levels: level_rx,
        hold: HoldTarget { tx: hold_tx },
        engine: EngineTarget { tx: engine_tx },
        insertion: InsertionTarget { tx: insertion_tx },
        results: result_rx,
    })
}
