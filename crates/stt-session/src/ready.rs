use std::sync::mpsc;

use stt_engine::Engine;
use stt_inject::NativeInjector;
use stt_overlay::Bubble;

use crate::chords::Chord;
use crate::config::Config;
use crate::drive::{run_decoder, BubbleSink, Idle, DECODE_QUEUE_CAPACITY};
use crate::target::Target;
use crate::StartupError;

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
pub struct Ready {
    config: Config,
    engine: Engine,
    insertion: InsertionConfig,
}

impl Ready {
    pub fn from_open(config: Config, engine: Engine) -> Self {
        Ready {
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
/// Dropping this does not stop Idle. The process drop does, as today.
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

/// Injector, hold bind, spawn Idle. Returns the Bubble receiver.
/// Does not open a window. Does not read prefs. Does not load a pack.
/// Call only after prepare_display. Bind then sees the forced X11 env.
pub fn start(ready: Ready) -> Result<LiveSession, StartupError> {
    let Ready {
        config,
        engine,
        insertion,
    } = ready;
    let injector = NativeInjector::connect().map_err(StartupError::backend)?;
    let hold = Chord::bind(&config.hold).map_err(|err| {
        StartupError::backend(format!("hold chord {}: {err}", config.hold.as_str()).into())
    })?;
    let (bubble_tx, bubble_rx) = mpsc::channel();
    let (level_tx, level_rx) = mpsc::sync_channel(8);
    let (hold_tx, hold_rx) = mpsc::channel();
    let (engine_tx, engine_rx) = mpsc::channel();
    let (insertion_tx, insertion_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    let (decode_tx, decode_rx) = mpsc::sync_channel(DECODE_QUEUE_CAPACITY);
    std::thread::Builder::new()
        .name("stt-decoder".to_string())
        .spawn(move || run_decoder(decode_rx, completion_tx))
        .map_err(|err| StartupError::backend(err.into()))?;
    let hold_raw = config.hold.as_str().to_string();
    let worker = Idle::new(
        hold,
        hold_raw,
        hold_rx,
        config.cancel,
        engine,
        engine_rx,
        insertion,
        insertion_rx,
        result_tx,
        decode_tx,
        completion_rx,
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
        hold: HoldTarget { tx: hold_tx },
        engine: EngineTarget { tx: engine_tx },
        insertion: InsertionTarget { tx: insertion_tx },
        results: result_rx,
    })
}
