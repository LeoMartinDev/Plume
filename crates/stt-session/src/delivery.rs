use crate::decoder::Completion;
use crate::destination::{InsertionError, TextDestination};
use crate::startup::{DictationResult, InsertionConfig};
use std::collections::BTreeMap;
use std::sync::mpsc;
use stt_core::{Edit, TextInjector};

pub(crate) struct TranscriptDelivery<I: TextInjector> {
    pub(crate) next_commit_id: u64,
    pending: BTreeMap<u64, Completion>,
    group_has_text: bool,
    pub(crate) target: TextDestination<I>,
    result_tx: mpsc::Sender<DictationResult>,
}

impl<I: TextInjector> TranscriptDelivery<I> {
    pub(crate) fn new(injector: I, result_tx: mpsc::Sender<DictationResult>) -> Self {
        Self {
            next_commit_id: 0,
            pending: BTreeMap::new(),
            group_has_text: false,
            target: TextDestination::new(injector),
            result_tx,
        }
    }
    pub(crate) fn drain(
        &mut self,
        completions: &mpsc::Receiver<Completion>,
        insertion: InsertionConfig,
    ) {
        while let Ok(completion) = completions.try_recv() {
            self.pending.insert(completion.id, completion);
        }
        while let Some(completion) =
            take_next_completion(&mut self.pending, &mut self.next_commit_id)
        {
            if !completion.joins_previous {
                self.group_has_text = false;
            }
            match completion.result {
                Ok(text) if text.is_empty() => eprintln!("stt-session: empty release"),
                Ok(text) => {
                    let text = prepare_insertion(
                        text,
                        completion.joins_previous,
                        &mut self.group_has_text,
                    );
                    match self
                        .target
                        .apply_edit_with_mode(&Edit::Insert(text.clone()), insertion.mode)
                    {
                        Ok(Some(report)) => {
                            self.target.reset();
                            eprintln!("stt-session: committed {} chars", text.len());
                            let _ = self.result_tx.send(DictationResult {
                                text,
                                injection: Ok(report),
                                copied_on_failure: false,
                            });
                        }
                        Ok(None) => unreachable!("insert must return an injection report"),
                        Err(err) => {
                            let reason = describe_target_error(&err);
                            let copied =
                                insertion.copy_on_failure && self.target.copy_text(&text).is_ok();
                            eprintln!("stt-session: aborted: {reason}");
                            let _ = self.result_tx.send(DictationResult {
                                text,
                                injection: Err(reason),
                                copied_on_failure: copied,
                            });
                        }
                    }
                }
                Err(err) => eprintln!("stt-session: aborted: {err}"),
            }
        }
    }
}

fn take_next_completion(
    pending: &mut BTreeMap<u64, Completion>,
    next_commit_id: &mut u64,
) -> Option<Completion> {
    let completion = pending.remove(next_commit_id)?;
    *next_commit_id += 1;
    Some(completion)
}

fn prepare_insertion(mut text: String, joins_previous: bool, group_has_text: &mut bool) -> String {
    if !joins_previous {
        *group_has_text = false;
    }
    if *group_has_text && !text.starts_with(char::is_whitespace) {
        text.insert(0, ' ');
    }
    *group_has_text = true;
    text
}

fn describe_target_error(err: &InsertionError) -> String {
    format!("target: {err}")
}
#[cfg(test)]
mod tests {
    use super::*;
    use stt_core::{BoxError, InjectionReport, InsertionMethod, InsertionMode, TargetAssessment};

    #[derive(Default)]
    struct FakeInjector {
        inserted: Vec<(String, InsertionMode)>,
        copied: Vec<String>,
        fail_insert: bool,
        fail_copy: bool,
    }
    impl TextInjector for FakeInjector {
        fn insert(&mut self, _: &str) -> Result<(), BoxError> {
            unreachable!()
        }
        fn replace_last(&mut self, _: &str, _: &str) -> Result<(), BoxError> {
            unreachable!()
        }
        fn insert_with_mode(
            &mut self,
            text: &str,
            mode: InsertionMode,
        ) -> Result<InjectionReport, BoxError> {
            self.inserted.push((text.into(), mode));
            if self.fail_insert {
                return Err("insert failed".into());
            }
            Ok(InjectionReport {
                method: InsertionMethod::Clipboard,
                application: Some("Editor".into()),
                target: TargetAssessment::Editable,
            })
        }
        fn copy_text(&mut self, text: &str) -> Result<(), BoxError> {
            self.copied.push(text.into());
            if self.fail_copy {
                Err("copy failed".into())
            } else {
                Ok(())
            }
        }
    }
    fn completion(id: u64, joins_previous: bool, text: &str) -> Completion {
        Completion {
            id,
            joins_previous,
            result: Ok(text.into()),
        }
    }
    #[test]
    fn out_of_order_results_wait_and_joined_groups_get_one_separator() {
        let (tx, rx) = mpsc::channel();
        let (result_tx, results) = mpsc::channel();
        let mut delivery = TranscriptDelivery::new(FakeInjector::default(), result_tx);
        tx.send(completion(1, true, "Second.")).unwrap();
        delivery.drain(&rx, InsertionConfig::default());
        assert!(results.try_recv().is_err());
        tx.send(completion(0, false, "First.")).unwrap();
        tx.send(completion(2, true, "\nThird.")).unwrap();
        tx.send(completion(3, false, "New field.")).unwrap();
        delivery.drain(&rx, InsertionConfig::default());
        let texts: Vec<_> = results.try_iter().map(|result| result.text).collect();
        assert_eq!(texts, ["First.", " Second.", "\nThird.", "New field."]);
        assert_eq!(delivery.next_commit_id, 4);
        assert_eq!(delivery.target.injector().inserted.len(), 4);
    }
    #[test]
    fn empty_and_failed_decode_results_do_not_insert_or_block_later_results() {
        let (tx, rx) = mpsc::channel();
        let (result_tx, results) = mpsc::channel();
        let mut delivery = TranscriptDelivery::new(FakeInjector::default(), result_tx);
        tx.send(completion(0, false, "")).unwrap();
        tx.send(Completion {
            id: 1,
            joins_previous: true,
            result: Err("decode failed".into()),
        })
        .unwrap();
        tx.send(completion(2, true, "Text")).unwrap();
        delivery.drain(&rx, InsertionConfig::default());
        assert_eq!(
            results
                .try_iter()
                .map(|result| result.text)
                .collect::<Vec<_>>(),
            ["Text"]
        );
        assert_eq!(delivery.target.injector().inserted.len(), 1);
    }
    #[test]
    fn insertion_mode_and_copy_fallback_are_applied_by_production_delivery() {
        for (fail_insert, copy_on_failure, fail_copy, expected_copied, copy_attempts) in [
            (false, true, false, false, 0),
            (true, true, false, true, 1),
            (true, true, true, false, 1),
            (true, false, false, false, 0),
        ] {
            let (tx, rx) = mpsc::channel();
            let (result_tx, results) = mpsc::channel();
            let injector = FakeInjector {
                fail_insert,
                fail_copy,
                ..Default::default()
            };
            let mut delivery = TranscriptDelivery::new(injector, result_tx);
            tx.send(completion(0, false, "Final")).unwrap();
            delivery.drain(
                &rx,
                InsertionConfig {
                    mode: InsertionMode::Clipboard,
                    copy_on_failure,
                },
            );
            let result = results.recv().unwrap();
            assert_eq!(result.injection.is_err(), fail_insert);
            assert_eq!(result.copied_on_failure, expected_copied);
            assert_eq!(delivery.target.injector().copied.len(), copy_attempts);
            assert_eq!(
                delivery.target.injector().inserted,
                [("Final".into(), InsertionMode::Clipboard)]
            );
        }
    }
}
