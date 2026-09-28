use std::sync::mpsc;
use std::time::Duration;

use stt_audio::Mic;
use stt_core::{AsrEngine, Dictation, Edit, HotkeyEvent, Hypothesis, TextInjector};
use stt_overlay::Bubble;

use crate::capture::{self, AudioPump};
use crate::chords::{CancelGuard, Chord, ChordSpec};
use crate::target::{Target, TargetError};

pub(crate) const POLL_QUANTUM: Duration = Duration::from_millis(5);

pub(crate) struct BubbleSink {
    tx: mpsc::Sender<Bubble>,
}

impl BubbleSink {
    pub(crate) fn new(tx: mpsc::Sender<Bubble>) -> Self {
        BubbleSink { tx }
    }

    pub(crate) fn push_from(&self, dictation: &Dictation) {
        let _ = self.tx.send(Bubble::from_dictation(dictation));
    }
}

#[cfg(test)]
pub(crate) trait Postpass {
    fn clean(&self, transcript: &str) -> String;
}

#[cfg(test)]
pub(crate) struct NoopPostpass;

#[cfg(test)]
impl Postpass for NoopPostpass {
    fn clean(&self, transcript: &str) -> String {
        transcript.to_string()
    }
}

#[derive(Debug)]
pub(crate) enum Outcome {
    #[cfg(test)]
    Committed(String),
    Released,
    #[cfg(test)]
    EmptyRelease,
    Cancelled,
    Aborted(String),
}

#[cfg(test)]
enum Fold {
    Continue,
    Done(Outcome),
}

#[cfg(test)]
fn fold_hypothesis<I: TextInjector, P: Postpass>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
    hyp: Hypothesis,
    postpass: &P,
) -> Fold {
    let step = match dictation.on_hypothesis(hyp) {
        Ok(step) => step,
        Err(_) => return Fold::Done(Outcome::EmptyRelease),
    };
    match step.transcript {
        Some(transcript) => {
            let raw = transcript.text;
            let cleaned = postpass.clean(&raw);
            if cleaned.is_empty() {
                return Fold::Done(Outcome::EmptyRelease);
            }
            if let Err(err) = target.apply_edit(&Edit::Insert(cleaned.clone())) {
                dictation.cancel();
                return Fold::Done(Outcome::Aborted(describe_target_error(&err)));
            }
            Fold::Done(Outcome::Committed(cleaned))
        }
        None => Fold::Continue,
    }
}

fn describe_target_error(err: &TargetError) -> String {
    format!("target: {err}")
}

pub(crate) struct Idle<E: AsrEngine, I: TextInjector> {
    hold: Option<Chord>,
    hold_raw: String,
    hold_rx: mpsc::Receiver<String>,
    cancel_spec: ChordSpec,
    engine: E,
    engine_rx: mpsc::Receiver<E>,
    completion_tx: mpsc::Sender<Result<String, String>>,
    completion_rx: mpsc::Receiver<Result<String, String>>,
    target: Target<I>,
    bubbles: BubbleSink,
    levels: mpsc::SyncSender<f32>,
}

impl<E: AsrEngine, I: TextInjector> Idle<E, I> {
    pub(crate) fn new(
        hold: Chord,
        hold_raw: String,
        hold_rx: mpsc::Receiver<String>,
        cancel_spec: ChordSpec,
        engine: E,
        engine_rx: mpsc::Receiver<E>,
        completion_tx: mpsc::Sender<Result<String, String>>,
        completion_rx: mpsc::Receiver<Result<String, String>>,
        target: Target<I>,
        bubbles: BubbleSink,
        levels: mpsc::SyncSender<f32>,
    ) -> Self {
        Idle {
            hold: Some(hold),
            hold_raw,
            hold_rx,
            cancel_spec,
            engine,
            engine_rx,
            completion_tx,
            completion_rx,
            target,
            bubbles,
            levels,
        }
    }

    fn retarget_hold(&mut self) {
        let Some(raw) = drain_latest(&self.hold_rx) else {
            return;
        };
        if raw == self.hold_raw {
            return;
        }
        let spec = match ChordSpec::parse(&raw) {
            Ok(spec) => spec,
            Err(err) => {
                eprintln!("stt-session: hold rejected: {err}");
                return;
            }
        };
        self.hold = None;
        match Chord::bind(&spec) {
            Ok(chord) => {
                eprintln!("stt-session: hold is {}", spec.as_str());
                self.hold_raw = spec.as_str().to_string();
                self.hold = Some(chord);
            }
            Err(err) => {
                eprintln!("stt-session: hold bind failed: {err}");
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
            Err(err) => eprintln!("stt-session: hold restore failed: {err}"),
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

impl<E: AsrEngine + Sync + Send + Clone + 'static, I: TextInjector> Idle<E, I> {
    pub(crate) fn run(mut self) -> ! {
        loop {
            while let Ok(engine) = self.engine_rx.try_recv() {
                self.engine = engine;
            }
            self.drain_completions();
            self.retarget_hold();
            match self.hold.as_mut().and_then(|hold| hold.next_event()) {
                Some(HotkeyEvent::Pressed) => {
                    let outcome = self.session_once();
                    self.target.reset();
                    match &outcome {
                        #[cfg(test)]
                        Outcome::Committed(text) => {
                            eprintln!("stt-session: committed {} chars", text.len())
                        }
                        Outcome::Released => {}
                        #[cfg(test)]
                        Outcome::EmptyRelease => eprintln!("stt-session: empty release"),
                        Outcome::Cancelled => eprintln!("stt-session: cancelled"),
                        Outcome::Aborted(reason) => {
                            eprintln!("stt-session: aborted: {reason}")
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
        self.bubbles.push_from(&dictation);
        let mic = match Mic::open() {
            Ok(mic) => mic,
            Err(err) => {
                dictation.cancel();
                self.bubbles.push_from(&dictation);
                self.hold = Some(hold);
                return Outcome::Aborted(format!("mic: {err}"));
            }
        };
        let guard = match CancelGuard::arm(&self.cancel_spec) {
            Ok(guard) => guard,
            Err(err) => {
                dictation.cancel();
                self.bubbles.push_from(&dictation);
                self.hold = Some(hold);
                return Outcome::Aborted(format!("cancel guard: {err}"));
            }
        };
        let (gate, pump, mic_pump) = capture::open_gate(mic, self.levels.clone());
        std::thread::spawn(move || mic_pump.run());
        let engine = self.engine.clone();
        let completion_tx = self.completion_tx.clone();
        std::thread::spawn(move || {
            let result = decode_final(&engine, gate.into_audio_stream());
            let _ = completion_tx.send(result);
        });
        let outcome = Self::live_loop(&mut hold, &self.bubbles, &mut dictation, pump, guard);
        self.hold = Some(hold);
        outcome
    }

    fn drain_completions(&mut self) {
        while let Ok(completion) = self.completion_rx.try_recv() {
            match completion {
                Ok(text) if text.is_empty() => eprintln!("stt-session: empty release"),
                Ok(text) => match self.target.apply_edit(&Edit::Insert(text.clone())) {
                    Ok(()) => {
                        self.target.reset();
                        eprintln!("stt-session: committed {} chars", text.len());
                    }
                    Err(err) => eprintln!("stt-session: aborted: {}", describe_target_error(&err)),
                },
                Err(err) => eprintln!("stt-session: aborted: {err}"),
            }
        }
    }

    fn live_loop(
        hold: &mut Chord,
        bubbles: &BubbleSink,
        dictation: &mut Dictation,
        pump: AudioPump,
        mut guard: CancelGuard,
    ) -> Outcome {
        let mut pump = Some(pump);
        loop {
            if guard.cancelled() {
                dictation.cancel();
                if let Some(pump) = pump.take() {
                    pump.close();
                }
                bubbles.push_from(dictation);
                return Outcome::Cancelled;
            }
            if matches!(hold.next_event(), Some(HotkeyEvent::Released)) {
                dictation.release();
                if let Some(pump) = pump.take() {
                    pump.close();
                }
                bubbles.push_from(dictation);
                return Outcome::Released;
            }
            std::thread::sleep(POLL_QUANTUM);
        }
    }
}

fn decode_final<E: AsrEngine>(engine: &E, audio: stt_core::AudioStream) -> Result<String, String> {
    for item in engine.stream(audio) {
        match item.map_err(|err| err.to_string())? {
            Hypothesis::Final(transcript) => return Ok(transcript.text),
            Hypothesis::Partial(_) => {}
        }
    }
    Err("decode thread ended without a final transcript".to_string())
}

#[cfg(test)]
pub(crate) fn drive_hypotheses<I: TextInjector, P: Postpass>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
    hyps: Vec<Hypothesis>,
    postpass: &P,
) -> Outcome {
    for hyp in hyps {
        match fold_hypothesis(dictation, target, hyp, postpass) {
            Fold::Continue => {}
            Fold::Done(outcome) => return outcome,
        }
    }
    Outcome::Aborted("hypotheses ended without a final transcript".to_string())
}

#[cfg(test)]
pub(crate) fn drive_cancel<I: TextInjector>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
) -> Outcome {
    dictation.cancel();
    let _ = target.retract();
    Outcome::Cancelled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::RecordingInjector;
    use stt_core::{AudioChunk, AudioStream, HypothesisStream, PartialHypothesis, Transcript};

    fn partial(text: &str) -> Hypothesis {
        Hypothesis::Partial(PartialHypothesis {
            text: text.to_string(),
        })
    }

    fn final_hyp(text: &str) -> Hypothesis {
        Hypothesis::Final(Transcript {
            text: text.to_string(),
        })
    }

    struct UpperPostpass;

    impl Postpass for UpperPostpass {
        fn clean(&self, transcript: &str) -> String {
            transcript.to_uppercase()
        }
    }

    struct FinalEngine;

    impl AsrEngine for FinalEngine {
        fn stream(&self, _audio: AudioStream) -> HypothesisStream {
            Box::new(std::iter::once(Ok(final_hyp("done"))))
        }
    }

    #[test]
    fn background_decode_returns_the_final_transcript() {
        let audio: AudioStream = Box::new(std::iter::once(AudioChunk {
            samples: vec![0.0; 16],
            sample_rate: 16_000,
        }));
        assert_eq!(decode_final(&FinalEngine, audio).as_deref(), Ok("done"));
    }

    #[test]
    fn hypotheses_insert_only_the_final_transcript() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let outcome = drive_hypotheses(
            &mut dictation,
            &mut target,
            vec![partial("bonj"), partial("bonjour"), final_hyp("Bonjour.")],
            &NoopPostpass,
        );
        assert!(
            matches!(outcome, Outcome::Committed(ref text) if text == "Bonjour."),
            "got: {outcome:?}"
        );
        assert_eq!(target.inserted(), "Bonjour.");
        assert_eq!(target.injector().ops(), &["insert:Bonjour."]);
    }

    #[test]
    fn postpass_rewrite_replaces_raw_transcript() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let outcome = drive_hypotheses(
            &mut dictation,
            &mut target,
            vec![partial("hello"), final_hyp("hello")],
            &UpperPostpass,
        );
        assert!(
            matches!(outcome, Outcome::Committed(ref text) if text == "HELLO"),
            "got: {outcome:?}"
        );
        assert_eq!(target.inserted(), "HELLO");
        assert_eq!(target.injector().ops(), &["insert:HELLO"]);
    }

    #[test]
    fn final_before_any_partial_commits_single_insert() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let outcome = drive_hypotheses(
            &mut dictation,
            &mut target,
            vec![final_hyp("Hi.")],
            &NoopPostpass,
        );
        assert!(
            matches!(outcome, Outcome::Committed(ref text) if text == "Hi."),
            "got: {outcome:?}"
        );
        assert_eq!(target.inserted(), "Hi.");
        assert_eq!(target.injector().ops(), &["insert:Hi."]);
    }

    #[test]
    fn release_before_any_partial_still_commits_the_final() {
        let mut dictation = Dictation::new();
        dictation.hold();
        dictation.release();
        let mut target = Target::new(RecordingInjector::new());
        let outcome = drive_hypotheses(
            &mut dictation,
            &mut target,
            vec![final_hyp("late")],
            &NoopPostpass,
        );
        assert!(
            matches!(outcome, Outcome::Committed(ref text) if text == "late"),
            "got: {outcome:?}"
        );
        assert_eq!(target.inserted(), "late");
        assert_eq!(target.injector().ops(), &["insert:late"]);
    }

    #[test]
    fn cancel_after_partial_retracts_and_swallows_late_output() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let folded = fold_hypothesis(&mut dictation, &mut target, partial("bonj"), &NoopPostpass);
        assert!(matches!(folded, Fold::Continue));
        let outcome = drive_cancel(&mut dictation, &mut target);
        assert!(matches!(outcome, Outcome::Cancelled), "got: {outcome:?}");
        assert!(target.injector().ops().is_empty());
        let late = dictation.on_hypothesis(partial("bonjour")).unwrap();
        assert_eq!(late.edit, None);
        assert_eq!(late.transcript, None);
    }

    #[test]
    fn partials_never_touch_the_target() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let folded = fold_hypothesis(&mut dictation, &mut target, partial("a"), &NoopPostpass);
        assert!(matches!(folded, Fold::Continue));
        let folded = fold_hypothesis(&mut dictation, &mut target, partial("ab"), &NoopPostpass);
        assert!(matches!(folded, Fold::Continue));
        assert!(target.injector().ops().is_empty());
    }

    #[test]
    fn drain_latest_keeps_the_last_chord() {
        let (tx, rx) = mpsc::channel();
        tx.send("Ctrl+Space".to_string()).unwrap();
        tx.send("Ctrl+m".to_string()).unwrap();
        assert_eq!(drain_latest(&rx).as_deref(), Some("Ctrl+m"));
        assert_eq!(drain_latest(&rx), None);
    }
}
