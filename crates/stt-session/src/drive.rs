use std::sync::mpsc;
use std::time::Duration;

use stt_audio::Mic;
use stt_core::{AsrEngine, BoxError, Dictation, Edit, HotkeyEvent, Hypothesis, TextInjector};
use stt_overlay::Bubble;

use crate::capture::{self, AudioPump};
use crate::chords::{CancelGuard, Chord, ChordSpec};
use crate::target::{Target, TargetError};

/// Chord poll quantum while live. Hotkey handles are non-blocking;
/// hypotheses arrive over a channel with the same timeout so one loop
/// watches keys and engine together with no async runtime.
pub(crate) const POLL_QUANTUM: Duration = Duration::from_millis(5);

/// A rendered bubble pushed to the GPUI thread. The projection happens
/// HERE (`Bubble::from_dictation`), at the send boundary: the window
/// thread receives values, never a `Dictation` reference, so the overlay
/// cannot become a second state machine even by accident.
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

/// Cleanup-LLM extension point (v1: `NoopPostpass`). Runs between the
/// final transcript and the last inject as `replace_last(raw, cleaned)`.
/// Local-only like everything else; settings UI decides enablement later.
pub(crate) trait Postpass {
    fn clean(&self, transcript: &str) -> String;
}

pub(crate) struct NoopPostpass;

impl Postpass for NoopPostpass {
    fn clean(&self, transcript: &str) -> String {
        transcript.to_string()
    }
}

/// How a session ended. `Cancelled` carries no text by construction:
/// the partials died with the consumed `Dictation`.
#[derive(Debug)]
pub(crate) enum Outcome {
    Committed(String),
    EmptyRelease,
    Cancelled,
    /// Engine or injector failed mid-session. Already-injected text was
    /// retracted best-effort; the error is reported, never injected.
    Aborted(String),
}

enum Fold {
    Continue,
    Done(Outcome),
}

/// The hypothesis->`Dictation`->`Target` fold the live loop performs, one
/// hypothesis at a time. The `#[cfg(test)]` seams drive this same fold
/// with scripted hypotheses, so tests exercise the exact worker logic.
fn fold_hypothesis<I: TextInjector, P: Postpass>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
    hyp: Hypothesis,
    postpass: &P,
) -> Fold {
    let step = match dictation.on_hypothesis(hyp) {
        Ok(step) => step,
        // on_hypothesis only fails while Idle; after hold that means the key
        // went up before any partial arrived, so this is an empty release.
        Err(_) => return Fold::Done(Outcome::EmptyRelease),
    };
    if let Some(edit) = &step.edit {
        if let Err(err) = target.apply_edit(edit) {
            dictation.cancel();
            let _ = target.retract();
            return Fold::Done(Outcome::Aborted(describe_target_error(&err)));
        }
    }
    match step.transcript {
        Some(transcript) => {
            let raw = transcript.text;
            let cleaned = postpass.clean(&raw);
            if cleaned != raw {
                let edit = Edit::Replace {
                    old: raw,
                    new: cleaned.clone(),
                };
                if let Err(err) = target.apply_edit(&edit) {
                    return Fold::Done(Outcome::Aborted(describe_target_error(&err)));
                }
            }
            Fold::Done(Outcome::Committed(cleaned))
        }
        None => Fold::Continue,
    }
}

fn describe_target_error(err: &TargetError) -> String {
    format!("target: {err}")
}

/// Idle compositor: owns every persistent handle (hold chord, engine,
/// injector ledger, bubble sink). `run` never returns; each session is a
/// fresh `Dictation` plus a fresh thread scope inside `session_once`.
pub(crate) struct Idle<E: AsrEngine, I: TextInjector> {
    hold: Chord,
    cancel_spec: ChordSpec,
    engine: E,
    target: Target<I>,
    bubbles: BubbleSink,
}

impl<E: AsrEngine, I: TextInjector> Idle<E, I> {
    pub(crate) fn new(
        hold: Chord,
        cancel_spec: ChordSpec,
        engine: E,
        target: Target<I>,
        bubbles: BubbleSink,
    ) -> Self {
        Idle {
            hold,
            cancel_spec,
            engine,
            target,
            bubbles,
        }
    }
}

impl<E: AsrEngine + Sync, I: TextInjector> Idle<E, I> {
    /// Idle loop. The hold chord is the ONLY thing polled here, so hold
    /// outside Idle is not a checked no-op but an unrepresentable event:
    /// while live, `Pressed` is never read (only the hold's `Released`,
    /// the cancel guard, and hypotheses).
    pub(crate) fn run(mut self) -> ! {
        loop {
            match self.hold.next_event() {
                Some(HotkeyEvent::Pressed) => {
                    let outcome = self.session_once();
                    self.target.reset();
                    match &outcome {
                        Outcome::Committed(text) => {
                            eprintln!("stt-session: committed {} chars", text.len())
                        }
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

    /// One live session: exactly one `Dictation`, one engine `stream`
    /// call, one mic, one cancel guard. All four die here. The scope
    /// joins both worker threads before returning, so no hypothesis,
    /// chunk, or grab outlives the session. Next session starts clean.
    fn session_once(&mut self) -> Outcome {
        let mut dictation = Dictation::new();
        dictation.hold();
        self.bubbles.push_from(&dictation);
        let mic = match Mic::open() {
            Ok(mic) => mic,
            Err(err) => return Outcome::Aborted(format!("mic: {err}")),
        };
        let guard = match CancelGuard::arm(&self.cancel_spec) {
            Ok(guard) => guard,
            Err(err) => return Outcome::Aborted(format!("cancel guard: {err}")),
        };
        let (gate, pump, mic_pump) = capture::open_gate(mic);
        let (hyp_tx, hyp_rx) = mpsc::channel();
        let engine = &self.engine;
        let outcome = std::thread::scope(|scope| {
            scope.spawn(|| mic_pump.run());
            scope.spawn(|| {
                for hyp in engine.stream(gate.into_audio_stream()) {
                    if hyp_tx.send(hyp).is_err() {
                        break;
                    }
                }
            });
            Self::live_loop(
                &mut self.hold,
                &mut self.target,
                &self.bubbles,
                &mut dictation,
                pump,
                guard,
                hyp_rx,
            )
        });
        self.bubbles.push_from(&dictation);
        outcome
    }

    /// The live poll: keys and hypotheses in one loop. Consumes the pump
    /// (closing the gate -> EOF -> engine `Final`) on release AND on
    /// cancel, and on every terminal outcome. The gate has no third state.
    fn live_loop(
        hold: &mut Chord,
        target: &mut Target<I>,
        bubbles: &BubbleSink,
        dictation: &mut Dictation,
        pump: AudioPump,
        mut guard: CancelGuard,
        hyp_rx: mpsc::Receiver<Result<Hypothesis, BoxError>>,
    ) -> Outcome {
        let mut pump = Some(pump);
        let mut released = false;
        loop {
            if guard.cancelled() {
                dictation.cancel();
                if let Some(pump) = pump.take() {
                    pump.close();
                }
                drop(hyp_rx);
                let _ = target.retract();
                return Outcome::Cancelled;
            }
            if !released && matches!(hold.next_event(), Some(HotkeyEvent::Released)) {
                released = true;
                dictation.release();
                if let Some(pump) = pump.take() {
                    pump.close();
                }
                bubbles.push_from(dictation);
            }
            match hyp_rx.recv_timeout(POLL_QUANTUM) {
                Ok(Ok(hyp)) => match fold_hypothesis(dictation, target, hyp, &NoopPostpass) {
                    Fold::Continue => bubbles.push_from(dictation),
                    Fold::Done(outcome) => {
                        if let Some(pump) = pump.take() {
                            pump.close();
                        }
                        return outcome;
                    }
                },
                Ok(Err(err)) => {
                    dictation.cancel();
                    if let Some(pump) = pump.take() {
                        pump.close();
                    }
                    let _ = target.retract();
                    return Outcome::Aborted(err.to_string());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(pump) = pump.take() {
                        pump.close();
                    }
                    return Outcome::Aborted(
                        "decode thread ended without a final transcript".to_string(),
                    );
                }
            }
        }
    }
}

/// Pure-ish test seam: the hypothesis->`Dictation`->`Target` folding the
/// live loop performs, minus threads and keys. Unit-testable with a
/// scripted `Vec<Hypothesis>` and a recording injector.
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

/// Cancel path as a testable unit: consume, retract, report. No text out.
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
    use stt_core::{PartialHypothesis, Transcript};

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

    #[test]
    fn hypotheses_fold_into_inserts_replaces_and_commit() {
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
        assert_eq!(
            target.injector().ops(),
            &[
                "insert:bonj",
                "replace:bonj->bonjour",
                "replace:bonjour->Bonjour."
            ]
        );
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
        assert_eq!(
            target.injector().ops(),
            &["insert:hello", "replace:hello->hello", "replace:hello->HELLO"]
        );
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
    fn release_before_any_partial_maps_final_to_empty_release() {
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
        assert!(matches!(outcome, Outcome::EmptyRelease), "got: {outcome:?}");
        assert_eq!(target.inserted(), "");
        assert!(target.injector().ops().is_empty());
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
        assert_eq!(
            target.injector().ops(),
            &["insert:bonj", "replace:bonj->"]
        );
        let late = dictation.on_hypothesis(partial("bonjour")).unwrap();
        assert_eq!(late.edit, None);
        assert_eq!(late.transcript, None);
    }

    #[test]
    fn desync_aborts_and_consumes_dictation() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let folded = fold_hypothesis(&mut dictation, &mut target, partial("a"), &NoopPostpass);
        assert!(matches!(folded, Fold::Continue));
        target.reset();
        let folded = fold_hypothesis(&mut dictation, &mut target, partial("ab"), &NoopPostpass);
        assert!(matches!(folded, Fold::Done(Outcome::Aborted(_))),);
        let late = dictation.on_hypothesis(final_hyp("ab")).unwrap();
        assert_eq!(late.edit, None);
        assert_eq!(late.transcript, None);
    }
}
