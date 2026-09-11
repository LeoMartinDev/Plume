use std::sync::mpsc;
use std::time::Duration;

use stt_audio::Mic;
use stt_core::{
    AsrEngine, BoxError, Dictation, Edit, HotkeyEvent, Hypothesis, TextInjector,
};
use stt_overlay::Bubble;

use super::capture::{self, AudioPump};
use super::chords::{CancelGuard, Chord, ChordSpec};
use super::target::Target;

pub const POLL_QUANTUM: Duration = Duration::from_millis(5);

pub struct BubbleSink {
    tx: mpsc::Sender<Bubble>,
}

impl BubbleSink {
    pub fn new(tx: mpsc::Sender<Bubble>) -> Self {
        Self { tx }
    }

    pub fn push_from(&self, dictation: &Dictation) {
        let _ = self.tx.send(Bubble::from_dictation(dictation));
    }
}

pub trait Postpass {
    fn clean(&self, transcript: &str) -> String;
}

pub struct NoopPostpass;

impl Postpass for NoopPostpass {
    fn clean(&self, transcript: &str) -> String {
        transcript.to_string()
    }
}

pub enum Outcome {
    Committed(String),
    EmptyRelease,
    Cancelled,
    Aborted(String),
}

enum Fold {
    Continue,
    Done(Outcome),
}

fn fold_hypothesis<I: TextInjector, P: Postpass>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
    hypothesis: Hypothesis,
    postpass: &P,
) -> Fold {
    let step = match dictation.on_hypothesis(hypothesis) {
        Ok(step) => step,
        Err(_) => return Fold::Done(Outcome::EmptyRelease),
    };
    if let Some(edit) = step.edit {
        if let Err(err) = target.apply_edit(&edit) {
            dictation.cancel();
            let _ = target.retract();
            return Fold::Done(Outcome::Aborted(err.to_string()));
        }
    }
    match step.transcript {
        Some(transcript) => Fold::Done(commit(target, transcript.text, postpass)),
        None => Fold::Continue,
    }
}

fn commit<I: TextInjector, P: Postpass>(
    target: &mut Target<I>,
    raw: String,
    postpass: &P,
) -> Outcome {
    let cleaned = postpass.clean(&raw);
    if cleaned == raw {
        return Outcome::Committed(raw);
    }
    match target.apply_edit(&Edit::Replace {
        old: raw,
        new: cleaned.clone(),
    }) {
        Ok(()) => Outcome::Committed(cleaned),
        Err(err) => Outcome::Aborted(err.to_string()),
    }
}

pub struct Idle<E: AsrEngine, I: TextInjector> {
    pub hold: Chord,
    pub cancel_spec: ChordSpec,
    pub engine: E,
    pub target: Target<I>,
    pub bubbles: BubbleSink,
}

impl<E: AsrEngine + Sync, I: TextInjector> Idle<E, I> {
    pub fn run(mut self) -> ! {
        loop {
            match self.hold.next_event() {
                Some(HotkeyEvent::Pressed) => {
                    let outcome = self.session_once();
                    self.target.reset();
                    report(&outcome);
                }
                _ => std::thread::sleep(POLL_QUANTUM),
            }
        }
    }

    fn session_once(&mut self) -> Outcome {
        let mut dictation = Dictation::new();
        dictation.hold();
        self.bubbles.push_from(&dictation);
        let mic = match Mic::open() {
            Ok(mic) => mic,
            Err(err) => return Outcome::Aborted(err.to_string()),
        };
        let guard = match CancelGuard::arm(&self.cancel_spec) {
            Ok(guard) => Some(guard),
            Err(err) => {
                #[cfg(windows)]
                {
                    eprintln!(
                        "stt-session: second hotkey handle failed, Esc cancel disabled: {err}"
                    );
                    None
                }
                #[cfg(not(windows))]
                {
                    return Outcome::Aborted(err.to_string());
                }
            }
        };
        let (gate, pump) = capture::open_gate();
        let pump_tx = pump.clone();
        let (hyp_tx, hyp_rx) = mpsc::channel();
        let outcome = std::thread::scope(|scope| {
            scope.spawn(move || capture::MicPump::new(mic, pump_tx).run());
            scope.spawn(|| {
                for hypothesis in self.engine.stream(gate.into_audio_stream()) {
                    if hyp_tx.send(hypothesis).is_err() {
                        break;
                    }
                }
            });
            Self::live_loop(
                &mut self.hold,
                &mut dictation,
                pump,
                guard,
                hyp_rx,
                &mut self.target,
                &self.bubbles,
            )
        });
        self.bubbles.push_from(&dictation);
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    fn live_loop(
        hold: &mut Chord,
        dictation: &mut Dictation,
        pump: AudioPump,
        mut guard: Option<CancelGuard>,
        hyp_rx: mpsc::Receiver<Result<Hypothesis, BoxError>>,
        target: &mut Target<I>,
        bubbles: &BubbleSink,
    ) -> Outcome {
        let mut pump = Some(pump);
        let mut released = false;
        let postpass = NoopPostpass;
        loop {
            if guard.as_mut().is_some_and(CancelGuard::cancelled) {
                dictation.cancel();
                drop(pump.take());
                let _ = target.retract();
                return Outcome::Cancelled;
            }
            if !released && matches!(hold.next_event(), Some(HotkeyEvent::Released)) {
                released = true;
                dictation.release();
                drop(pump.take());
                bubbles.push_from(dictation);
            }
            match hyp_rx.recv_timeout(POLL_QUANTUM) {
                Ok(Ok(hypothesis)) => {
                    match fold_hypothesis(dictation, target, hypothesis, &postpass) {
                        Fold::Continue => bubbles.push_from(dictation),
                        Fold::Done(outcome) => return outcome,
                    }
                }
                Ok(Err(err)) => {
                    dictation.cancel();
                    drop(pump.take());
                    let _ = target.retract();
                    return Outcome::Aborted(err.to_string());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    dictation.cancel();
                    drop(pump.take());
                    return Outcome::Aborted(
                        "decode thread ended without a final transcript".into(),
                    );
                }
            }
        }
    }
}

fn report(outcome: &Outcome) {
    match outcome {
        Outcome::Committed(text) => eprintln!("stt-session: committed {text:?}"),
        Outcome::EmptyRelease => eprintln!("stt-session: empty release"),
        Outcome::Cancelled => eprintln!("stt-session: cancelled"),
        Outcome::Aborted(err) => eprintln!("stt-session: aborted: {err}"),
    }
}

#[cfg(test)]
pub(crate) fn drive_hypotheses<I: TextInjector, P: Postpass>(
    dictation: &mut Dictation,
    target: &mut Target<I>,
    hypotheses: Vec<Hypothesis>,
    postpass: &P,
) -> Outcome {
    for hypothesis in hypotheses {
        match fold_hypothesis(dictation, target, hypothesis, postpass) {
            Fold::Continue => {}
            Fold::Done(outcome) => return outcome,
        }
    }
    Outcome::Aborted("scripted hypotheses ended without a final transcript".into())
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
    use crate::fakes::{Call, RecordingInjector};
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

    #[test]
    fn drive_hypotheses_commits_bonjour() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let outcome = drive_hypotheses(
            &mut dictation,
            &mut target,
            vec![
                partial("bonj"),
                partial("bonjour"),
                final_hyp("Bonjour."),
            ],
            &NoopPostpass,
        );
        assert!(
            matches!(outcome, Outcome::Committed(ref text) if text == "Bonjour."),
            "must commit the final transcript"
        );
        assert_eq!(target.inserted(), "Bonjour.");
        assert_eq!(
            target.injector().calls,
            vec![
                Call::Insert("bonj".into()),
                Call::ReplaceLast("bonj".into(), "bonjour".into()),
                Call::ReplaceLast("bonjour".into(), "Bonjour.".into()),
            ]
        );
    }

    #[test]
    fn drive_cancel_retracts_and_swallows_a_late_final() {
        let mut dictation = Dictation::new();
        dictation.hold();
        let mut target = Target::new(RecordingInjector::new());
        let step = dictation.on_hypothesis(partial("bonj")).unwrap();
        target.apply_edit(&step.edit.unwrap()).unwrap();
        let outcome = drive_cancel(&mut dictation, &mut target);
        assert!(matches!(outcome, Outcome::Cancelled));
        assert_eq!(target.inserted(), "");
        assert_eq!(
            target.injector().calls,
            vec![
                Call::Insert("bonj".into()),
                Call::ReplaceLast("bonj".into(), "".into()),
            ]
        );

        let late = dictation.on_hypothesis(final_hyp("Bonjour.")).unwrap();
        assert_eq!(late.edit, None);
        assert_eq!(late.transcript, None);
        assert_eq!(target.injector().calls.len(), 2);
    }
}
