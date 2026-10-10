use std::sync::mpsc;

use plume_engine::Engine;
use plume_inject::NativeInjector;
use plume_overlay::Bubble;

use crate::chords::Chord;
use crate::config::Config;
use crate::decoder::Decoder;
use crate::delivery::TranscriptDelivery;
use crate::runtime::{BubbleSink, RuntimeConfig, RuntimeUpdates, SessionOutputs, SessionRuntime};
use crate::StartupError;

const VOICE_LEVEL_QUEUE_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionMode {
    #[default]
    System = 0,
    Preview = 1,
    Suspended = 2,
}

#[derive(Clone, Debug)]
pub enum PreviewEvent {
    Feedback(plume_overlay::Feedback),
    Transcript(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertionConfig {
    pub destination: plume_core::Destination,
    pub mode: plume_core::InsertionMode,
    pub copy_on_failure: bool,
}

impl Default for InsertionConfig {
    fn default() -> Self {
        Self {
            destination: plume_core::Destination::FocusedField,
            mode: plume_core::InsertionMode::Auto,
            copy_on_failure: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DictationResult {
    pub text: String,
    pub injection: Result<plume_core::InjectionReport, String>,
    pub copied_on_failure: bool,
    pub record_id: Option<u64>,
    pub audio_available: bool,
    pub transcription_error: Option<String>,
    pub recovered: bool,
}

/// Proven pack plus parsed chords. The only value `start` accepts.
/// There is no constructor that omits `Engine`.
pub struct PreparedSession {
    config: Config,
    engine: Engine,
    insertion: InsertionConfig,
    recordings_path: Option<std::path::PathBuf>,
    vad_path: Option<std::path::PathBuf>,
    mode: SessionMode,
}

impl PreparedSession {
    pub fn from_open(config: Config, engine: Engine) -> Self {
        PreparedSession {
            config,
            engine,
            insertion: InsertionConfig::default(),
            recordings_path: None,
            vad_path: None,
            mode: SessionMode::System,
        }
    }

    pub fn with_recordings(mut self, path: std::path::PathBuf, vad: std::path::PathBuf) -> Self {
        self.recordings_path = Some(path);
        self.vad_path = Some(vad);
        self
    }
    pub fn with_mode(mut self, mode: SessionMode) -> Self {
        self.mode = mode;
        self
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
    pub preview: mpsc::Receiver<PreviewEvent>,
    pub bubbles: mpsc::Receiver<Bubble>,
    pub levels: mpsc::Receiver<f32>,
    pub hold: HoldTarget,
    pub engine: EngineTarget,
    pub insertion: InsertionTarget,
    pub results: mpsc::Receiver<DictationResult>,
    pub control: SessionControl,
    pub recordings: crate::SharedRecordings,
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

pub(crate) enum SessionCommand {
    ConfigureShortcuts(Config, mpsc::Sender<Result<(), String>>),
    SetMode(SessionMode, mpsc::Sender<Result<(), String>>),
    Retry(u64),
    EditShortcuts(bool, Option<mpsc::Sender<Result<(), String>>>),
}
#[derive(Clone)]
pub struct SessionControl {
    output_mode: std::sync::Arc<std::sync::atomic::AtomicU8>,
    tx: mpsc::SyncSender<SessionCommand>,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    toggle: mpsc::Sender<Option<String>>,
}
pub struct HistoryEditGuard {
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
pub struct ShortcutEditGuard {
    tx: mpsc::SyncSender<SessionCommand>,
    _reservation: HistoryEditGuard,
}
impl Drop for ShortcutEditGuard {
    fn drop(&mut self) {
        let (tx, rx) = mpsc::channel();
        if self
            .tx
            .send(SessionCommand::EditShortcuts(false, Some(tx)))
            .is_ok()
        {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(1));
        }
    }
}
impl Drop for HistoryEditGuard {
    fn drop(&mut self) {
        self.busy.store(false, std::sync::atomic::Ordering::Release);
    }
}
impl SessionControl {
    pub fn configure_shortcuts(&self, config: Config) -> Result<(), String> {
        let _reservation = self
            .reserve_history_edit()
            .map_err(|_| "Wait until dictation finishes.".to_string())?;
        let (tx, rx) = mpsc::channel();
        self.tx
            .try_send(SessionCommand::ConfigureShortcuts(config, tx))
            .map_err(|_| "Shortcut service unavailable".to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(1))
            .map_err(|_| "Shortcut service did not respond".to_string())?
    }

    /// Reserve readiness until the compositor acknowledges the new destination.
    pub fn set_mode(&self, mode: SessionMode) -> Result<(), String> {
        self.set_mode_after(mode, || Ok(()))
    }

    pub fn mode(&self) -> SessionMode {
        match self.output_mode.load(std::sync::atomic::Ordering::Acquire) {
            1 => SessionMode::Preview,
            2 => SessionMode::Suspended,
            _ => SessionMode::System,
        }
    }

    /// Persist a destination change while preventing a new capture from starting.
    pub fn set_mode_after(
        &self,
        mode: SessionMode,
        persist: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let _reservation = self
            .reserve_history_edit()
            .map_err(|_| "Wait until dictation finishes.".to_string())?;
        persist()?;
        let (tx, rx) = mpsc::channel();
        self.tx
            .try_send(SessionCommand::SetMode(mode, tx))
            .map_err(|_| "Dictation service unavailable".to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(1))
            .map_err(|_| "Dictation service did not respond".to_string())?
    }
    pub fn reserve_shortcut_edit(&self) -> Result<ShortcutEditGuard, String> {
        let reservation = self
            .reserve_history_edit()
            .map_err(|_| "Wait until dictation finishes before changing shortcuts.".to_string())?;
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(SessionCommand::EditShortcuts(true, Some(tx)))
            .map_err(|_| "Dictation unavailable".to_string())?;
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Ok(())) => Ok(ShortcutEditGuard {
                tx: self.tx.clone(),
                _reservation: reservation,
            }),
            result => {
                let _ = self.tx.send(SessionCommand::EditShortcuts(false, None));
                Err(result
                    .ok()
                    .and_then(Result::err)
                    .unwrap_or_else(|| "Shortcut service unavailable".into()))
            }
        }
    }
    pub fn reserve_history_edit(&self) -> Result<HistoryEditGuard, String> {
        use std::sync::atomic::Ordering;
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Wait until dictation finishes before deleting recordings.".to_string())?;
        Ok(HistoryEditGuard {
            busy: self.busy.clone(),
        })
    }
    pub fn is_busy(&self) -> bool {
        self.busy.load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn set_toggle(&self, toggle: Option<String>) {
        let _ = self.toggle.send(toggle);
    }
    pub fn retry(&self, id: u64) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Dictation is busy".to_string())?;
        if self.tx.try_send(SessionCommand::Retry(id)).is_err() {
            self.busy.store(false, Ordering::Release);
            return Err("Dictation unavailable".into());
        }
        Ok(())
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
        recordings_path,
        vad_path,
        mode,
    } = ready;
    let injector = NativeInjector::connect().map_err(StartupError::backend)?;
    let mut hold = Chord::bind(&config).map_err(|err| {
        StartupError::backend(format!("hold chord {}: {err}", config.hold.as_str()).into())
    })?;
    if mode == SessionMode::Suspended {
        hold.passthrough(true).map_err(StartupError::backend)?;
    }
    let (bubble_tx, bubble_rx) = mpsc::channel();
    let (level_tx, level_rx) = mpsc::sync_channel(VOICE_LEVEL_QUEUE_CAPACITY);
    let (hold_tx, hold_rx) = mpsc::channel();
    let (engine_tx, engine_rx) = mpsc::channel();
    let (insertion_tx, insertion_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let (preview_tx, preview_rx) = mpsc::channel();
    let output_mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(mode as u8));
    let recordings = crate::RecordingStore::open(
        recordings_path.unwrap_or_else(|| config.model_dir().join("recordings")),
    )
    .map_err(StartupError::backend)?
    .shared();
    let vad_path = vad_path.unwrap_or_else(|| config.model_dir().join("silero_vad.onnx"));
    let (command_tx, commands) = mpsc::sync_channel(1);
    let (toggle_tx, toggle) = mpsc::channel();
    let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = SessionControl {
        output_mode: output_mode.clone(),
        tx: command_tx,
        busy: busy.clone(),
        toggle: toggle_tx,
    };
    let decoder = Decoder::start().map_err(|err| StartupError::backend(err.into()))?;
    let delivery = TranscriptDelivery::new(injector, result_tx);
    let worker = SessionRuntime::new(
        RuntimeConfig {
            output_mode: output_mode.clone(),
            session: config,
            hold,
            engine,
            insertion,
            recordings: recordings.clone(),
            vad_path,
        },
        RuntimeUpdates {
            hold: hold_rx,
            engine: engine_rx,
            insertion: insertion_rx,
            commands,
            toggle,
            busy,
        },
        SessionOutputs {
            bubbles: BubbleSink::with_preview(bubble_tx, preview_tx, output_mode),
            levels: level_tx,
        },
        decoder,
        delivery,
    );
    std::thread::Builder::new()
        .name("plume-compositor".to_string())
        .spawn(|| worker.run())
        .map_err(|err| StartupError::backend(err.into()))?;
    Ok(LiveSession {
        preview: preview_rx,
        bubbles: bubble_rx,
        levels: level_rx,
        hold: HoldTarget { tx: hold_tx },
        engine: EngineTarget { tx: engine_tx },
        insertion: InsertionTarget { tx: insertion_tx },
        results: result_rx,
        control,
        recordings,
    })
}

#[cfg(test)]
mod control_tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};
    fn control() -> (SessionControl, mpsc::Receiver<SessionCommand>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let (toggle, _) = mpsc::channel();
        (
            SessionControl {
                output_mode: Arc::new(std::sync::atomic::AtomicU8::new(SessionMode::System as u8)),
                tx,
                toggle,
                busy: Arc::new(AtomicBool::new(false)),
            },
            rx,
        )
    }
    #[test]
    fn mode_change_reserves_capture_until_persistence_and_native_acknowledgement() {
        let (control, commands) = control();
        let observed = control.clone();
        let worker = std::thread::spawn(move || {
            let SessionCommand::SetMode(mode, reply) = commands.recv().unwrap() else {
                panic!("unexpected command");
            };
            assert_eq!(mode, SessionMode::System);
            assert!(observed.is_busy());
            assert!(observed.retry(42).is_err());
            reply.send(Ok(())).unwrap();
        });
        control
            .set_mode_after(SessionMode::System, || {
                assert!(control.is_busy());
                assert!(control.reserve_shortcut_edit().is_err());
                Ok(())
            })
            .unwrap();
        worker.join().unwrap();
        assert!(!control.is_busy());
    }

    #[test]
    fn shortcut_configuration_reserves_capture_until_native_acknowledgement() {
        let (control, commands) = control();
        let config = Config::from_prefs("Alt+Space", "Esc", "models".into()).unwrap();
        let observed = control.clone();
        let worker = std::thread::spawn(move || {
            let SessionCommand::ConfigureShortcuts(_, reply) = commands.recv().unwrap() else {
                panic!("unexpected command");
            };
            assert!(observed.is_busy());
            assert!(observed.retry(42).is_err());
            reply.send(Ok(())).unwrap();
        });
        control.configure_shortcuts(config.clone()).unwrap();
        worker.join().unwrap();
        assert!(!control.is_busy());
        let _busy = control.reserve_history_edit().unwrap();
        assert!(control.configure_shortcuts(config).is_err());
    }

    #[test]
    fn failed_persistence_or_busy_capture_never_sends_a_destination_change() {
        let (control, commands) = control();
        assert!(control
            .set_mode_after(SessionMode::System, || Err("disk full".into()))
            .is_err());
        assert!(commands.try_recv().is_err());
        assert!(!control.is_busy());
        let _busy = control.reserve_history_edit().unwrap();
        assert!(control
            .set_mode_after(SessionMode::System, || panic!(
                "must not persist while busy"
            ))
            .is_err());
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn editing_reserves_the_session_until_native_shortcuts_are_restored() {
        let (control, commands) = control();
        let observed = control.clone();
        let worker = std::thread::spawn(move || {
            for expected in [true, false] {
                let SessionCommand::EditShortcuts(active, Some(reply)) = commands.recv().unwrap()
                else {
                    panic!("unexpected command")
                };
                assert_eq!(active, expected);
                assert!(observed.is_busy());
                reply.send(Ok(())).unwrap();
            }
        });
        let guard = control.reserve_shortcut_edit().unwrap();
        assert!(control.retry(7).is_err());
        assert!(control.reserve_history_edit().is_err());
        drop(guard);
        worker.join().unwrap();
        assert!(!control.is_busy());
    }
    #[test]
    fn shortcut_edit_failure_rolls_back_and_releases_the_session() {
        let (control, commands) = control();
        let worker = std::thread::spawn(move || {
            let SessionCommand::EditShortcuts(true, Some(reply)) = commands.recv().unwrap() else {
                panic!("unexpected command")
            };
            reply.send(Err("native failure".into())).unwrap();
            assert!(matches!(
                commands.recv().unwrap(),
                SessionCommand::EditShortcuts(false, None)
            ));
        });
        assert!(control.reserve_shortcut_edit().is_err());
        worker.join().unwrap();
        assert!(!control.is_busy());
    }
}
