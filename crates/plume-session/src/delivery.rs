use crate::destination::TextDestination;
use crate::startup::{DictationResult, InsertionConfig};
use plume_core::{CancellationToken, TextInjector};
use plume_overlay::Feedback;
use std::sync::mpsc;

pub(crate) struct TranscriptDelivery<I: TextInjector> {
    pub target: TextDestination<I>,
    last_result: Option<DictationResult>,
    result_tx: mpsc::Sender<DictationResult>,
}
impl<I: TextInjector> TranscriptDelivery<I> {
    pub fn new(injector: I, result_tx: mpsc::Sender<DictationResult>) -> Self {
        Self {
            target: TextDestination::new(injector),
            result_tx,
            last_result: None,
        }
    }
    #[cfg(test)]
    pub fn deliver(&mut self, id: u64, text: String, config: InsertionConfig) -> Feedback {
        let feedback =
            self.deliver_checked(id, text, config, &CancellationToken::default(), &mut || {
                true
            });
        if let Some(result) = self.take_result() {
            self.publish(result);
        }
        feedback
    }
    pub fn deliver_checked(
        &mut self,
        id: u64,
        text: String,
        config: InsertionConfig,
        cancel: &CancellationToken,
        before_dispatch: &mut dyn FnMut() -> bool,
    ) -> Feedback {
        self.last_result = None;
        let injection = self
            .target
            .insert_checked(&text, config.mode, before_dispatch);
        if injection.is_err() {
            before_dispatch();
        }
        if cancel.is_cancelled() {
            return match injection {
                Err(error) if error.to_string() != "dictation cancelled" => Feedback::Error {
                    title: "Dictation cancelled".into(),
                    advice: error.to_string(),
                },
                _ => Feedback::Empty,
            };
        }
        let (injection, copied) = match injection {
            Ok(report) => (Ok(report), false),
            Err(error) => {
                let copied = config.copy_on_failure && self.target.copy_text(&text).is_ok();
                (Err(error.to_string()), copied)
            }
        };
        self.target.reset();
        let feedback = if injection.is_ok() {
            Feedback::Success
        } else {
            Feedback::InsertionFailed {
                text: text.clone(),
                copied,
            }
        };
        let result = DictationResult {
            text,
            injection,
            copied_on_failure: copied,
            record_id: Some(id),
            audio_available: true,
            transcription_error: None,
            recovered: false,
        };
        self.last_result = Some(result);
        feedback
    }
    pub fn publish(&self, result: DictationResult) {
        let _ = self.result_tx.send(result);
    }
    pub fn take_result(&mut self) -> Option<DictationResult> {
        self.last_result.take()
    }
    pub fn failed(&self, id: u64, error: String, recovered: bool, audio_available: bool) {
        let _ = self.result_tx.send(DictationResult {
            text: String::new(),
            injection: Err(error.clone()),
            copied_on_failure: false,
            record_id: Some(id),
            audio_available,
            transcription_error: Some(error),
            recovered,
        });
    }
    pub fn recovered(&self, id: u64, text: String) {
        let _ = self.result_tx.send(DictationResult {
            text,
            injection: Err("Recovered in History; not inserted".into()),
            copied_on_failure: false,
            record_id: Some(id),
            audio_available: true,
            transcription_error: None,
            recovered: true,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use plume_core::BoxError;
    struct Failed;
    impl TextInjector for Failed {
        fn insert(&mut self, _: &str) -> Result<(), BoxError> {
            Err("failed".into())
        }
        fn replace_last(&mut self, _: &str, _: &str) -> Result<(), BoxError> {
            unreachable!()
        }
        fn copy_text(&mut self, _: &str) -> Result<(), BoxError> {
            Ok(())
        }
    }
    #[test]
    fn pre_dispatch_cancellation_never_copies_or_publishes_the_text() {
        let (tx, rx) = mpsc::channel();
        let mut delivery = TranscriptDelivery::new(Failed, tx);
        let token = CancellationToken::default();
        let cancel = token.clone();
        let mut guard = move || {
            cancel.cancel();
            false
        };
        assert_eq!(
            delivery.deliver_checked(
                7,
                "text".into(),
                InsertionConfig::default(),
                &token,
                &mut guard
            ),
            Feedback::Empty
        );
        assert!(delivery.take_result().is_none());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn insertion_failure_and_recovery_are_distinct() {
        let (tx, rx) = mpsc::channel();
        let mut delivery = TranscriptDelivery::new(Failed, tx);
        assert!(matches!(
            delivery.deliver(1, "text".into(), InsertionConfig::default()),
            Feedback::InsertionFailed { copied: true, .. }
        ));
        assert!(rx.recv().unwrap().copied_on_failure);
        delivery.recovered(1, "new".into());
        let result = rx.recv().unwrap();
        assert!(result.recovered);
        assert_eq!(result.text, "new");
    }
}
