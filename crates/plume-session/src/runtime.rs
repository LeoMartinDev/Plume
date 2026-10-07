use crate::capture::{self, AudioPump};
use crate::chords::{CancelGuard, Chord, ChordSpec};
use crate::decoder::{DecodeJob, Decoder};
use crate::delivery::TranscriptDelivery;
use crate::startup::InsertionConfig;
use plume_audio::Mic;
use plume_core::{AsrEngine, Dictation, HotkeyEvent, TextInjector};
use plume_overlay::Bubble;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const POLL_QUANTUM: Duration = Duration::from_millis(5);

pub(crate) struct RuntimeConfig<E> {
    pub(crate) session: crate::Config,
    pub(crate) hold: Chord,
    pub(crate) engine: E,
    pub(crate) insertion: InsertionConfig,
}

pub(crate) struct RuntimeUpdates<E> {
    pub(crate) hold: mpsc::Receiver<String>,
    pub(crate) engine: mpsc::Receiver<E>,
    pub(crate) insertion: mpsc::Receiver<InsertionConfig>,
}

pub(crate) struct SessionOutputs {
    pub(crate) bubbles: BubbleSink,
    pub(crate) levels: mpsc::SyncSender<f32>,
}

#[derive(Debug)]
enum Outcome {
    Released,
    Cancelled,
    Aborted(String),
}

pub(crate) struct BubbleSink {
    tx: mpsc::Sender<Bubble>,
}

impl BubbleSink {
    pub(crate) fn new(tx: mpsc::Sender<Bubble>) -> Self {
        BubbleSink { tx }
    }

    pub(crate) fn push_from(&self, dictation: &Dictation, id: u64) {
        let _ = self
            .tx
            .send(Bubble::from_dictation(dictation).with_capture_id(id));
    }
}

pub(crate) struct SessionRuntime<E: AsrEngine, I: TextInjector> {
    hold: Option<Chord>,
    hold_raw: String,
    cancel_spec: ChordSpec,
    engine: E,
    insertion: InsertionConfig,
    updates: RuntimeUpdates<E>,
    outputs: SessionOutputs,
    decoder: Decoder<E>,
    delivery: TranscriptDelivery<I>,
    next_capture_id: u64,
}

impl<E: AsrEngine, I: TextInjector> SessionRuntime<E, I> {
    pub(crate) fn new(
        config: RuntimeConfig<E>,
        updates: RuntimeUpdates<E>,
        outputs: SessionOutputs,
        decoder: Decoder<E>,
        delivery: TranscriptDelivery<I>,
    ) -> Self {
        let RuntimeConfig {
            session,
            hold,
            engine,
            insertion,
        } = config;
        Self {
            hold: Some(hold),
            hold_raw: session.hold.as_str().to_string(),
            cancel_spec: session.cancel,
            engine,
            insertion,
            updates,
            outputs,
            decoder,
            delivery,
            next_capture_id: 0,
        }
    }

    fn retarget_hold(&mut self) {
        let Some(raw) = drain_latest(&self.updates.hold) else {
            return;
        };
        if raw == self.hold_raw {
            return;
        }
        let spec = match ChordSpec::parse(&raw) {
            Ok(spec) => spec,
            Err(err) => {
                tracing::warn!("plume-session: hold rejected: {err}");
                return;
            }
        };
        self.hold = None;
        match Chord::bind(&spec) {
            Ok(chord) => {
                tracing::debug!("plume-session: hold is {}", spec.as_str());
                self.hold_raw = spec.as_str().to_string();
                self.hold = Some(chord);
            }
            Err(err) => {
                tracing::warn!("plume-session: hold bind failed: {err}");
                self.restore_hold();
            }
        }
    }

    fn restore_hold(&mut self) {
        let Ok(spec) = ChordSpec::parse(&self.hold_raw) else {
            return;
        };
        match Chord::bind(&spec) {
            Ok(chord) => self.hold = Some(chord),
            Err(err) => tracing::warn!("plume-session: hold restore failed: {err}"),
        }
    }
}

fn drain_latest(rx: &mpsc::Receiver<String>) -> Option<String> {
    let mut latest = None;
    while let Ok(raw) = rx.try_recv() {
        latest = Some(raw);
    }
    latest
}

impl<E: AsrEngine + Sync + Send + Clone + 'static, I: TextInjector> SessionRuntime<E, I> {
    pub(crate) fn run(mut self) -> ! {
        loop {
            while let Ok(engine) = self.updates.engine.try_recv() {
                self.engine = engine;
            }
            while let Ok(insertion) = self.updates.insertion.try_recv() {
                self.insertion = insertion;
            }
            self.delivery.drain_with_feedback(
                &self.decoder.completions,
                self.insertion,
                |bubble| {
                    let _ = self.outputs.bubbles.tx.send(bubble);
                },
            );
            self.retarget_hold();
            match self.hold.as_mut().and_then(|hold| hold.next_event()) {
                Some(HotkeyEvent::Pressed) => {
                    let outcome = self.session_once();
                    self.delivery.target.reset();
                    match &outcome {
                        Outcome::Released => {}
                        Outcome::Cancelled => tracing::debug!("plume-session: cancelled"),
                        Outcome::Aborted(reason) => {
                            tracing::warn!("plume-session: aborted: {reason}");
                            let (title, advice) = if reason.starts_with("mic:") {
                                (
                                    "Microphone unavailable",
                                    "Check your microphone and microphone permissions.",
                                )
                            } else {
                                (
                                    "Dictation unavailable",
                                    "Check your shortcut and model in Settings.",
                                )
                            };
                            let _ = self.outputs.bubbles.tx.send(Bubble::feedback(
                                self.next_capture_id,
                                plume_overlay::Feedback::Error {
                                    title: title.into(),
                                    advice: advice.into(),
                                },
                            ));
                        }
                    }
                }
                _ => std::thread::sleep(POLL_QUANTUM),
            }
        }
    }

    fn session_once(&mut self) -> Outcome {
        let Some(mut hold) = self.hold.take() else {
            return Outcome::Aborted("hold chord is not bound".into());
        };
        let mut dictation = Dictation::new();
        dictation.hold();
        let started = tracing::enabled!(tracing::Level::DEBUG).then(Instant::now);
        tracing::debug!("plume-session: capture={} pressed", self.next_capture_id);
        let mic = match Mic::open() {
            Ok(mic) => mic,
            Err(err) => {
                dictation.cancel();
                self.outputs
                    .bubbles
                    .push_from(&dictation, self.next_capture_id);
                self.hold = Some(hold);
                return Outcome::Aborted(format!("mic: {err}"));
            }
        };
        if let Some(started) = started {
            tracing::debug!(
                capture = self.next_capture_id,
                microphone_ready_ms = started.elapsed().as_millis(),
                "Microphone ready"
            );
        }
        let guard = match CancelGuard::arm(&self.cancel_spec) {
            Ok(guard) => guard,
            Err(err) => {
                dictation.cancel();
                self.outputs
                    .bubbles
                    .push_from(&dictation, self.next_capture_id);
                self.hold = Some(hold);
                return Outcome::Aborted(format!("cancel guard: {err}"));
            }
        };
        let (gate, pump, mic_pump) = capture::open_gate(mic, self.outputs.levels.clone());
        let id = self.next_capture_id;
        let joins_previous = id > self.delivery.next_commit_id;
        let cancelled = Arc::new(AtomicBool::new(false));
        let job = DecodeJob {
            id,
            joins_previous,
            engine: self.engine.clone(),
            audio: gate.into_audio_stream(),
            cancelled: cancelled.clone(),
        };
        if let Err(err) = self.decoder.submit(job) {
            cancelled.store(true, Ordering::Release);
            pump.close();
            dictation.cancel();
            self.outputs
                .bubbles
                .push_from(&dictation, self.next_capture_id);
            self.hold = Some(hold);
            return Outcome::Aborted(err);
        }
        self.next_capture_id += 1;
        std::thread::spawn(move || mic_pump.run());
        self.outputs.bubbles.push_from(&dictation, id);
        let outcome = Self::live_loop(
            &mut hold,
            id,
            &self.outputs.bubbles,
            &mut dictation,
            pump,
            guard,
            cancelled,
        );
        if let Some(started) = started {
            tracing::debug!(
                capture = id,
                ?outcome,
                hold_ms = started.elapsed().as_millis(),
                "Capture finished"
            );
        }
        self.hold = Some(hold);
        outcome
    }

    fn live_loop(
        hold: &mut Chord,
        id: u64,
        bubbles: &BubbleSink,
        dictation: &mut Dictation,
        pump: AudioPump,
        mut guard: CancelGuard,
        cancelled: Arc<AtomicBool>,
    ) -> Outcome {
        let mut pump = Some(pump);
        loop {
            if guard.cancelled() {
                dictation.cancel();
                cancelled.store(true, Ordering::Release);
                if let Some(pump) = pump.take() {
                    pump.close();
                }
                bubbles.push_from(dictation, id);
                return Outcome::Cancelled;
            }
            if matches!(hold.next_event(), Some(HotkeyEvent::Released)) {
                dictation.release();
                if let Some(pump) = pump.take() {
                    pump.finish();
                }
                bubbles.push_from(dictation, id);
                return Outcome::Released;
            }
            std::thread::sleep(POLL_QUANTUM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_shortcut_update_wins() {
        let (tx, rx) = mpsc::channel();
        tx.send("Ctrl+Space".into()).unwrap();
        tx.send("Ctrl+m".into()).unwrap();
        assert_eq!(drain_latest(&rx).as_deref(), Some("Ctrl+m"));
        assert_eq!(drain_latest(&rx), None);
    }
}
