use crate::capture::{self, CaptureCommand, CaptureEvent};
use crate::chords::Chord;
use crate::controller::{Action, Controller, Mode, Phase};
use crate::decoder::{Completion, DecodeJob, DecodeOutcome, Decoder};
use crate::delivery::TranscriptDelivery;
use crate::startup::{InsertionConfig, PreviewEvent, SessionCommand, SessionMode};
use crate::{Recording, SharedRecordings};
use plume_core::{AsrEngine, CancellationToken, TextInjector};
use plume_overlay::{Bubble, Feedback};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(5);
pub(crate) struct RuntimeConfig<E> {
    pub output_mode: Arc<std::sync::atomic::AtomicU8>,
    pub session: crate::Config,
    pub hold: Chord,
    pub engine: E,
    pub insertion: InsertionConfig,
    pub recordings: SharedRecordings,
    pub vad_path: std::path::PathBuf,
}
pub(crate) struct RuntimeUpdates<E> {
    pub hold: mpsc::Receiver<String>,
    pub engine: mpsc::Receiver<E>,
    pub insertion: mpsc::Receiver<InsertionConfig>,
    pub toggle: mpsc::Receiver<Option<String>>,
    pub commands: mpsc::Receiver<SessionCommand>,
    pub busy: Arc<AtomicBool>,
}
pub(crate) struct SessionOutputs {
    pub bubbles: BubbleSink,
    pub levels: mpsc::SyncSender<f32>,
}
pub(crate) struct BubbleSink {
    tx: mpsc::Sender<Bubble>,
    preview: mpsc::Sender<PreviewEvent>,
    mode: Arc<std::sync::atomic::AtomicU8>,
}
impl BubbleSink {
    pub fn with_preview(
        tx: mpsc::Sender<Bubble>,
        preview: mpsc::Sender<PreviewEvent>,
        mode: Arc<std::sync::atomic::AtomicU8>,
    ) -> Self {
        Self { tx, preview, mode }
    }
    fn feedback(&self, id: u64, feedback: Feedback) {
        if self.mode.load(Ordering::Acquire) == SessionMode::Preview as u8 {
            let _ = self.preview.send(PreviewEvent::Feedback(feedback));
        } else {
            let _ = self.tx.send(Bubble::feedback(id, feedback));
        }
    }
}
struct Active {
    id: u64,
    ui_id: u64,
    cancel: CancellationToken,
    commands: Option<mpsc::Sender<CaptureCommand>>,
    events: Option<mpsc::Receiver<CaptureEvent>>,
    capture_done: bool,
    cleanup_retry_at: Option<Instant>,
    completion: Option<Completion>,
    read_status: Option<plume_engine::AudioReadStatus>,
    record: Option<Recording>,
    error: Option<String>,
    started: Option<Instant>,
    last_speech: Option<Instant>,
    mode: Option<Mode>,
    silence_shown: bool,
    limit_shown: bool,
    /// The VAD flagged speech during this capture. A silent capture skips
    /// "Transcribing" and goes straight to "No speech detected".
    heard_speech: bool,
    retry: bool,
    cancel_feedback: Option<Feedback>,
}
pub(crate) struct SessionRuntime<E: AsrEngine, I: TextInjector> {
    output_mode: Arc<std::sync::atomic::AtomicU8>,
    config: crate::Config,
    hold: Chord,
    engine: E,
    insertion: InsertionConfig,
    updates: RuntimeUpdates<E>,
    outputs: SessionOutputs,
    decoder: Decoder<E>,
    delivery: TranscriptDelivery<I>,
    recordings: SharedRecordings,
    vad_path: std::path::PathBuf,
    controller: Controller,
    active: Option<Active>,
    ui_serial: u64,
}
impl<E: AsrEngine + Send + Sync + Clone + 'static, I: TextInjector> SessionRuntime<E, I> {
    pub fn new(
        config: RuntimeConfig<E>,
        updates: RuntimeUpdates<E>,
        outputs: SessionOutputs,
        decoder: Decoder<E>,
        delivery: TranscriptDelivery<I>,
    ) -> Self {
        Self {
            output_mode: config.output_mode,
            config: config.session,
            hold: config.hold,
            engine: config.engine,
            insertion: config.insertion,
            recordings: config.recordings,
            vad_path: config.vad_path,
            updates,
            outputs,
            decoder,
            delivery,
            controller: Controller::new(),
            active: None,
            ui_serial: 0,
        }
    }
    pub fn run(mut self) -> ! {
        let mut previous = None;
        loop {
            if previous != Some(self.controller.phase) {
                tracing::debug!(state=?self.controller.phase,record_id=self.active.as_ref().map(|a|a.id),"dictation transition");
                previous = Some(self.controller.phase);
            }
            // All physical edges are consumed in the phase in which they occurred.
            self.drain_keys();
            while let Ok(completion) = self.decoder.completions.try_recv() {
                if let Some(active) = &mut self.active {
                    if completion.id == active.id {
                        if matches!(completion.outcome, DecodeOutcome::Failed(_))
                            && !active.capture_done
                        {
                            if let Some(commands) = &active.commands {
                                let _ = commands.send(CaptureCommand::Finish);
                            }
                            self.controller.phase = Phase::Transcribing;
                        }
                        active.completion = Some(completion);
                    }
                }
            }
            self.capture_events();
            self.tick();
            self.commit_if_finished();
            if self.controller.phase == Phase::Ready {
                self.update_settings();
                if let Ok(command) = self.updates.commands.try_recv() {
                    match command {
                        SessionCommand::ConfigureShortcuts(config, reply) => {
                            let result = self
                                .hold
                                .register(&config)
                                .map_err(|error| error.to_string());
                            let accepted = result.is_ok();
                            if reply.send(result).is_ok() && accepted {
                                self.config = config;
                                self.controller = Controller::new();
                            } else {
                                let _ = self.hold.register(&self.config);
                            }
                        }
                        SessionCommand::SetMode(mode, reply) => {
                            let result = self
                                .hold
                                .passthrough(mode == SessionMode::Suspended)
                                .map_err(|e| e.to_string());
                            let accepted = result.is_ok();
                            if reply.send(result).is_ok() && accepted {
                                self.output_mode.store(mode as u8, Ordering::Release);
                                self.controller = Controller::new();
                            } else {
                                let _ = self.hold.passthrough(
                                    self.output_mode.load(Ordering::Acquire)
                                        == SessionMode::Suspended as u8,
                                );
                            }
                        }
                        SessionCommand::Retry(id) => self.start_retry(id),
                        SessionCommand::EditShortcuts(active, reply) => {
                            let suspended = self.output_mode.load(Ordering::Acquire)
                                == SessionMode::Suspended as u8;
                            let result = self
                                .hold
                                .passthrough(active || suspended)
                                .map_err(|e| e.to_string());
                            if !active {
                                self.controller = Controller::new();
                            }
                            if let Err(error) = &result {
                                self.ui_serial += 1;
                                self.outputs.bubbles.feedback(
                                    self.ui_serial,
                                    Feedback::Error {
                                        title: "Shortcut service unavailable".into(),
                                        advice: error.clone(),
                                    },
                                );
                            }
                            if let Some(reply) = reply {
                                let _ = reply.send(result);
                            }
                        }
                    }
                }
            }
            self.hold.cancel_active(matches!(
                self.controller.phase,
                Phase::Starting(_) | Phase::Recording(_) | Phase::Transcribing | Phase::Cancelling
            ));
            std::thread::sleep(POLL);
        }
    }
    fn drain_keys(&mut self) {
        while let Some(event) = self.hold.next_event() {
            if self.output_mode.load(Ordering::Acquire) == SessionMode::Suspended as u8 {
                continue;
            }
            tracing::debug!(action = ?event.action, edge = ?event.edge, state = ?self.controller.phase, "dictation shortcut received");
            match self.controller.event(event) {
                Some(Action::Start(mode)) => self.start_capture(mode),
                Some(Action::Stop) => {
                    if let Some(active) = &self.active {
                        if let Some(tx) = &active.commands {
                            let _ = tx.send(CaptureCommand::Finish);
                        }
                        if active.retry || active.heard_speech {
                            self.outputs
                                .bubbles
                                .feedback(active.ui_id, Feedback::Transcribing);
                        }
                    }
                }
                Some(Action::Cancel) => {
                    if let Some(active) = &self.active {
                        active.cancel.cancel();
                        if let Some(tx) = &active.commands {
                            let _ = tx.send(CaptureCommand::Cancel);
                        }
                        if let Err(e) = self.recordings.lock().unwrap().mark_cancelled(active.id) {
                            tracing::warn!("cancel marker: {e}");
                        }
                        self.outputs
                            .bubbles
                            .feedback(active.ui_id, Feedback::Cancelling);
                    }
                }
                None => {}
            }
        }
    }
    fn start_capture(&mut self, mode: Mode) {
        self.ui_serial += 1;
        if self
            .updates
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.controller.phase = Phase::Ready;
            return;
        }
        let engine = self.engine.snapshot();
        let preview = self.output_mode.load(Ordering::Acquire) == SessionMode::Preview as u8;
        let writer_result = {
            let mut store = self.recordings.lock().unwrap();
            if preview {
                store.begin_preview()
            } else {
                store.begin()
            }
        };
        let writer = match writer_result {
            Ok(w) => w,
            Err(e) => {
                self.controller.phase = Phase::Ready;
                self.updates.busy.store(false, Ordering::Release);
                self.outputs.bubbles.feedback(
                    self.ui_serial,
                    Feedback::Error {
                        title: "Audio storage unavailable".into(),
                        advice: format!("Recording did not start. {e}"),
                    },
                );
                return;
            }
        };
        let id = writer.record.id;
        let cancel = CancellationToken::default();
        self.outputs
            .bubbles
            .feedback(self.ui_serial, Feedback::Starting);
        let capture = match capture::start_recorded(
            writer,
            self.vad_path.clone(),
            self.outputs.levels.clone(),
            cancel.clone(),
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = self.recordings.lock().unwrap().delete(id);
                self.fail_start(id, e.to_string());
                return;
            }
        };
        let mut active = Active {
            id,
            ui_id: self.ui_serial,
            cancel: cancel.clone(),
            commands: Some(capture.commands),
            events: Some(capture.events),
            capture_done: false,
            cleanup_retry_at: None,
            completion: None,
            read_status: None,
            record: None,
            error: None,
            started: None,
            last_speech: None,
            mode: Some(mode),
            silence_shown: false,
            limit_shown: false,
            heard_speech: false,
            retry: false,
            cancel_feedback: None,
        };
        if let Err(error) = self.decoder.submit(DecodeJob {
            id,
            engine,
            audio: capture.audio,
            cancelled: cancel,
        }) {
            active.cancel.cancel();
            active.completion = Some(Completion {
                id,
                outcome: DecodeOutcome::Failed(error),
            });
            self.controller.phase = Phase::Cancelling;
        }
        self.active = Some(active);
    }
    fn fail_start(&mut self, _id: u64, error: String) {
        self.controller.phase = Phase::Ready;
        self.updates.busy.store(false, Ordering::Release);
        self.outputs.bubbles.feedback(
            self.ui_serial,
            Feedback::Error {
                title: "Dictation unavailable".into(),
                advice: error,
            },
        );
    }
    fn start_retry(&mut self, id: u64) {
        self.ui_serial += 1;
        self.controller.phase = Phase::Transcribing;
        self.updates.busy.store(true, Ordering::Release);
        let engine = self.engine.snapshot();
        let saved = self.recordings.lock().unwrap().begin_retry(id);
        if let Err(error) = saved {
            self.fail_start(id, error.to_string());
            return;
        }
        let cancel = CancellationToken::default();
        let path = self.recordings.lock().unwrap().replay_path(id);
        let (audio, read_status) =
            match plume_engine::audio_from_wav_with_control(&path, cancel.clone()) {
                Ok(a) => a,
                Err(e) => {
                    self.fail_start(id, e.to_string());
                    return;
                }
            };
        let audio = plume_engine::gate_recording(
            audio,
            self.vad_path.clone(),
            cancel.clone(),
            read_status.clone(),
        );
        if let Err(e) = self.decoder.submit(DecodeJob {
            id,
            engine,
            audio,
            cancelled: cancel.clone(),
        }) {
            self.fail_start(id, e);
            return;
        }
        self.active = Some(Active {
            id,
            ui_id: self.ui_serial,
            cancel,
            commands: None,
            events: None,
            capture_done: true,
            cleanup_retry_at: None,
            completion: None,
            read_status: Some(read_status),
            record: None,
            error: None,
            started: None,
            last_speech: None,
            mode: None,
            silence_shown: false,
            limit_shown: false,
            heard_speech: false,
            retry: true,
            cancel_feedback: None,
        });
        self.outputs
            .bubbles
            .feedback(self.ui_serial, Feedback::Transcribing);
    }
    fn capture_events(&mut self) {
        let Some(active) = &mut self.active else {
            return;
        };
        if let Some(events) = &active.events {
            loop {
                let event = match events.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !active.capture_done {
                            // Unwinding drops the microphone and writer before
                            // this channel closes. Still wait for the decoder.
                            active.capture_done = true;
                            active.error = Some("recording worker disconnected".into());
                            if self.controller.phase != Phase::Cancelling {
                                self.controller.phase = Phase::Transcribing;
                            }
                        }
                        break;
                    }
                };
                match event {
                    CaptureEvent::Ready(at) => {
                        active.started = Some(at);
                        active.last_speech = Some(at);
                        if let Phase::Starting(mode) = self.controller.phase {
                            self.controller.phase = Phase::Recording(mode);
                            self.outputs.bubbles.feedback(
                                active.ui_id,
                                Feedback::RecordingNotice {
                                    silence: false,
                                    limit: false,
                                    cancel: self.config.cancel.as_str().into(),
                                },
                            );
                        }
                    }
                    CaptureEvent::Speech(at) => {
                        active.last_speech = Some(at);
                        active.heard_speech = true;
                        if active.silence_shown {
                            active.silence_shown = false;
                            if matches!(self.controller.phase, Phase::Recording(_)) {
                                self.outputs.bubbles.feedback(
                                    active.ui_id,
                                    Feedback::RecordingNotice {
                                        silence: false,
                                        limit: active.limit_shown,
                                        cancel: self.config.cancel.as_str().into(),
                                    },
                                );
                            }
                        }
                    }
                    CaptureEvent::Ended { record, error } => {
                        active.capture_done = true;
                        active.record = record.map(|r| *r);
                        active.error = error;
                        if self.controller.phase != Phase::Cancelling {
                            self.controller.phase = Phase::Transcribing;
                            // Speech in the last frames may only show up here.
                            let silent = active.record.as_ref().is_some_and(|r| !r.has_speech);
                            if active.retry || !silent {
                                self.outputs
                                    .bubbles
                                    .feedback(active.ui_id, Feedback::Transcribing);
                            }
                        }
                    }
                }
            }
        }
    }
    fn tick(&mut self) {
        if !matches!(self.controller.phase, Phase::Recording(_)) {
            return;
        }
        let Some(active) = &mut self.active else {
            return;
        };
        let Some(started) = active.started else {
            return;
        };
        let now = Instant::now();
        let timing = crate::controller::timing(
            active.mode.unwrap_or(Mode::Hold),
            now.duration_since(started),
            active
                .last_speech
                .map_or(Duration::ZERO, |at| now.duration_since(at)),
        );
        let silence = timing.silence;
        let limit = timing.warning;
        if silence != active.silence_shown || limit != active.limit_shown {
            active.silence_shown = silence;
            active.limit_shown = limit;
            self.outputs.bubbles.feedback(
                active.ui_id,
                Feedback::RecordingNotice {
                    silence,
                    limit,
                    cancel: self.config.cancel.as_str().into(),
                },
            );
        }
        if timing.limit {
            if let Some(tx) = &active.commands {
                let _ = tx.send(CaptureCommand::Limit);
            }
            self.controller.phase = Phase::Transcribing;
            if active.heard_speech {
                self.outputs
                    .bubbles
                    .feedback(active.ui_id, Feedback::Transcribing);
            }
        }
    }
    fn commit_if_finished(&mut self) {
        if !self.active.as_ref().is_some_and(|a| {
            a.capture_done
                && a.completion.is_some()
                && a.cleanup_retry_at.is_none_or(|at| Instant::now() >= at)
        }) {
            return;
        }
        let mut active = self.active.take().unwrap();
        if active.record.is_none() && !active.retry && !active.cancel.is_cancelled() {
            match self.recordings.lock().unwrap().recover_partial(active.id) {
                Ok(record) => active.record = record,
                Err(e) => active.error = Some(format!("audio recovery: {e}")),
            }
        }
        if active.cancel.is_cancelled() {
            if let Err(e) = self.recordings.lock().unwrap().delete(active.id) {
                // Do not advertise readiness while cancellation cleanup failed.
                self.outputs.bubbles.feedback(
                    active.ui_id,
                    Feedback::Error {
                        title: "Audio deletion failed".into(),
                        advice: e.to_string(),
                    },
                );
                active.cleanup_retry_at = Some(Instant::now() + Duration::from_secs(1));
                self.active = Some(active);
                self.controller.phase = Phase::Cancelling;
                return;
            } else {
                self.outputs.bubbles.feedback(
                    active.ui_id,
                    active.cancel_feedback.clone().unwrap_or(Feedback::Empty),
                );
            }
        } else {
            let mut result = match &active.completion.as_ref().unwrap().outcome {
                DecodeOutcome::Final(t) => Ok(t.clone()),
                DecodeOutcome::Failed(e) => Err(e.clone()),
                DecodeOutcome::Cancelled => Err("cancelled".into()),
            };
            if let Some(error) = active
                .read_status
                .as_ref()
                .and_then(|status| status.error())
            {
                result = Err(format!("audio recovery: {error}"));
            }
            if let Some(error) = active.error.clone() {
                result = Err(error);
            }
            let has_speech = active.retry || active.record.as_ref().is_some_and(|r| r.has_speech);
            if self.output_mode.load(Ordering::Acquire) == SessionMode::Preview as u8 {
                if self.cancel_before_delivery(&mut active) {
                    return;
                }
                match crate::preview::finish(
                    &self.recordings,
                    active.id,
                    has_speech,
                    result,
                    &self.outputs.bubbles.preview,
                ) {
                    Ok(feedback) => self.outputs.bubbles.feedback(active.ui_id, feedback),
                    Err(error) => {
                        self.outputs.bubbles.feedback(
                            active.ui_id,
                            Feedback::Error {
                                title: "Audio deletion failed".into(),
                                advice: error,
                            },
                        );
                        active.cancel.cancel();
                        active.cleanup_retry_at = Some(Instant::now() + Duration::from_secs(1));
                        self.active = Some(active);
                        self.controller.phase = Phase::Cancelling;
                        return;
                    }
                }
                self.controller.phase = Phase::Inserting;
                self.drain_keys();
                self.controller.finish(true, true, true);
                self.updates.busy.store(false, Ordering::Release);
                return;
            }
            if !has_speech && !active.retry {
                let _ = self.recordings.lock().unwrap().delete(active.id);
                if let Err(error) = result {
                    self.outputs.bubbles.feedback(
                        active.ui_id,
                        Feedback::Error {
                            title: "Recording unavailable".into(),
                            advice: error,
                        },
                    );
                } else {
                    self.outputs
                        .bubbles
                        .feedback(active.ui_id, Feedback::NoSpeech);
                }
            } else if !has_speech && result.as_ref().is_ok_and(|t| t.trim().is_empty()) {
                let _ = self.recordings.lock().unwrap().delete(active.id);
                self.outputs
                    .bubbles
                    .feedback(active.ui_id, Feedback::NoSpeech);
            } else {
                let mut store = self.recordings.lock().unwrap();
                let saved = if let Some(record) = active.record.clone().filter(|r| r.has_speech) {
                    store.stage(record)
                } else {
                    Ok(())
                };
                let saved = saved.and_then(|_| store.finish_decode(active.id, &result));
                drop(store);
                if let Err(error) = saved {
                    result = Err(format!("recording storage: {error}"));
                }
                if self.cancel_before_delivery(&mut active) {
                    return;
                }
                match result {
                    Ok(text) if !text.trim().is_empty() && !active.retry => {
                        self.controller.phase = Phase::Inserting;
                        self.outputs
                            .bubbles
                            .feedback(active.ui_id, Feedback::Inserting);
                        self.hold.cancel_active(true);
                        let cancel = active.cancel.clone();
                        let id = active.id;
                        let store = self.recordings.clone();
                        let hold = &mut self.hold;
                        let controller = &mut self.controller;
                        let bubbles = &self.outputs.bubbles;
                        let ui_id = active.ui_id;
                        let mut dispatch_closed = false;
                        let mut before_dispatch = || {
                            if dispatch_closed {
                                return true;
                            }
                            controller.phase = Phase::Transcribing;
                            while let Some(event) = hold.next_event() {
                                if controller.event(event) == Some(Action::Cancel) {
                                    cancel.cancel();
                                    bubbles.feedback(ui_id, Feedback::Cancelling);
                                    if let Err(error) = store.lock().unwrap().mark_cancelled(id) {
                                        tracing::warn!(record_id = id, "cancel marker: {error}");
                                    }
                                }
                            }
                            if cancel.is_cancelled() {
                                false
                            } else {
                                controller.phase = Phase::Inserting;
                                hold.cancel_active(false);
                                dispatch_closed = true;
                                true
                            }
                        };
                        let mut feedback = self.delivery.deliver_checked(
                            id,
                            text,
                            self.insertion,
                            &cancel,
                            &mut before_dispatch,
                        );
                        if cancel.is_cancelled() {
                            active.cancel_feedback = Some(feedback);
                            self.active = Some(active);
                            self.controller.phase = Phase::Cancelling;
                            return;
                        }
                        if let Some(mut report) = self.delivery.take_result() {
                            if let Err(error) = self
                                .recordings
                                .lock()
                                .unwrap()
                                .finish_insertion(active.id, &report)
                            {
                                tracing::warn!(
                                    record_id = active.id,
                                    "insertion status persistence: {error}"
                                );
                            }
                            if let Err(error) = self.recordings.lock().unwrap().promote_existing(id)
                            {
                                feedback = Feedback::Error {
                                    title: "Audio recovery unavailable".into(),
                                    advice: error.to_string(),
                                };
                            }
                            report.audio_available = self.recordings.lock().unwrap().has_audio(id);
                            self.delivery.publish(report);
                        }
                        self.outputs.bubbles.feedback(active.ui_id, feedback);
                    }
                    Ok(text) => {
                        if let Err(error) =
                            self.recordings.lock().unwrap().promote_existing(active.id)
                        {
                            tracing::warn!(record_id = active.id, "audio promotion: {error}");
                        }
                        self.delivery.recovered(active.id, text);
                        self.outputs
                            .bubbles
                            .feedback(active.ui_id, Feedback::Success);
                    }
                    Err(error) => {
                        if let Err(error) =
                            self.recordings.lock().unwrap().promote_existing(active.id)
                        {
                            tracing::warn!(record_id = active.id, "audio promotion: {error}");
                        }
                        let audio_available = self.recordings.lock().unwrap().has_audio(active.id);
                        self.delivery.failed(
                            active.id,
                            error.clone(),
                            active.retry,
                            audio_available,
                        );
                        self.outputs.bubbles.feedback(
                            active.ui_id,
                            Feedback::Error {
                                title: "Transcription unavailable".into(),
                                advice: if audio_available {
                                    format!("Retry from History. {error}")
                                } else {
                                    format!("Audio could not be retained. {error}")
                                },
                            },
                        );
                    }
                }
            }
        }
        // Drain presses accumulated during the native insertion before becoming Ready.
        self.controller.phase = Phase::Inserting;
        self.drain_keys();
        self.controller.finish(true, true, true);
        self.updates.busy.store(false, Ordering::Release);
    }
    fn cancel_before_delivery(&mut self, active: &mut Active) -> bool {
        while let Some(event) = self.hold.next_event() {
            if self.controller.event(event) == Some(Action::Cancel) {
                active.cancel.cancel();
                let _ = self.recordings.lock().unwrap().mark_cancelled(active.id);
            }
        }
        if !active.cancel.is_cancelled() {
            return false;
        }
        match self.recordings.lock().unwrap().delete(active.id) {
            Ok(()) => {
                self.outputs.bubbles.feedback(active.ui_id, Feedback::Empty);
                self.controller.finish(true, true, true);
                self.updates.busy.store(false, Ordering::Release);
            }
            Err(error) => {
                self.outputs.bubbles.feedback(
                    active.ui_id,
                    Feedback::Error {
                        title: "Audio deletion failed".into(),
                        advice: error.to_string(),
                    },
                );
                active.cleanup_retry_at = Some(Instant::now() + Duration::from_secs(1));
                // Ownership of the operation remains with the controller until cleanup succeeds.
                self.active = Some(Active {
                    id: active.id,
                    ui_id: active.ui_id,
                    cancel: active.cancel.clone(),
                    commands: None,
                    events: None,
                    capture_done: true,
                    cleanup_retry_at: active.cleanup_retry_at,
                    completion: active.completion.take(),
                    read_status: None,
                    record: active.record.take(),
                    error: None,
                    started: None,
                    last_speech: None,
                    mode: active.mode,
                    silence_shown: false,
                    limit_shown: false,
                    heard_speech: false,
                    retry: active.retry,
                    cancel_feedback: active.cancel_feedback.take(),
                });
                self.controller.phase = Phase::Cancelling;
            }
        }
        true
    }
    fn update_settings(&mut self) {
        while let Ok(engine) = self.updates.engine.try_recv() {
            self.engine = engine;
        }
        while let Ok(insertion) = self.updates.insertion.try_recv() {
            self.insertion = insertion;
        }
        let mut next = self.config.clone();
        let mut changed = false;
        while let Ok(hold) = self.updates.hold.try_recv() {
            if let Ok(spec) = crate::chords::ChordSpec::parse(&hold) {
                next.hold = spec;
                changed = true;
            }
        }
        while let Ok(toggle) = self.updates.toggle.try_recv() {
            if let Ok(config) = next.clone().with_toggle(toggle.as_deref()) {
                next = config;
                changed = true;
            }
        }
        if changed {
            match self.hold.register(&next) {
                Ok(()) => self.config = next,
                Err(e) => tracing::warn!("shortcut update rejected: {e}"),
            }
        }
    }
}
