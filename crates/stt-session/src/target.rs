use stt_core::{BoxError, Edit, TextInjector};

/// Owns the answer to "what did we put in the focused app?" so cancel
/// can retract it without asking `Dictation` (which deliberately drops
/// the partial on cancel). Per-actor state: the injector's ledger is the
/// merge point, not a second copy of session state.
pub(crate) struct Target<I: TextInjector> {
    injector: I,
    /// Exact text currently attributed to this session in the target app.
    inserted: String,
}

impl<I: TextInjector> Target<I> {
    pub(crate) fn new(injector: I) -> Self {
        Target {
            injector,
            inserted: String::new(),
        }
    }

    /// Apply one `Step.edit`, updating the ledger. `Insert` appends;
    /// `Replace{old, new}` requires `inserted` to end with `old` and
    /// swaps the suffix. A mismatch is a fatal desync, not a guess.
    pub(crate) fn apply_edit(&mut self, edit: &Edit) -> Result<(), TargetError> {
        match edit {
            Edit::Insert(text) => {
                self.injector.insert(text).map_err(TargetError::Inject)?;
                self.inserted.push_str(text);
            }
            Edit::Replace { old, new } => {
                if !self.inserted.ends_with(old.as_str()) {
                    return Err(TargetError::Desync {
                        expected_suffix: old.clone(),
                    });
                }
                self.injector
                    .replace_last(old, new)
                    .map_err(TargetError::Inject)?;
                self.inserted.truncate(self.inserted.len() - old.len());
                self.inserted.push_str(new);
            }
        }
        Ok(())
    }

    /// Best-effort retract for the cancel path: backspace exactly what
    /// this session inserted. Implemented as `replace_last(inserted, "")`
    /// so no injector change is needed. Where the target app disallows it,
    /// the backspaces land nowhere: that IS the spec's "where the target
    /// app allows it".
    pub(crate) fn retract(&mut self) -> Result<(), BoxError> {
        if self.inserted.is_empty() {
            return Ok(());
        }
        let inserted = std::mem::take(&mut self.inserted);
        self.injector.replace_last(&inserted, "")
    }

    /// The ledger, for tests.
    #[cfg(test)]
    pub(crate) fn inserted(&self) -> &str {
        &self.inserted
    }

    /// Forget the ledger between sessions. Called exactly once per
    /// settle; a session never inherits another session's text.
    pub(crate) fn reset(&mut self) {
        self.inserted.clear();
    }
}

#[cfg(test)]
impl<I: TextInjector> Target<I> {
    pub(crate) fn injector(&self) -> &I {
        &self.injector
    }
}

#[derive(Debug)]
pub(crate) enum TargetError {
    /// The app moved under us (or an earlier inject silently failed).
    /// Fatal for this session: cancel, retract what we can, settle.
    Desync {
        expected_suffix: String,
    },
    Inject(BoxError),
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetError::Desync { expected_suffix } => {
                write!(
                    f,
                    "target desync: {expected_suffix:?} is not a suffix of inserted text"
                )
            }
            TargetError::Inject(err) => write!(f, "inject failed: {err}"),
        }
    }
}

impl std::error::Error for TargetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TargetError::Desync { .. } => None,
            TargetError::Inject(err) => Some(err.as_ref()),
        }
    }
}

#[cfg(test)]
pub(crate) struct RecordingInjector {
    ops: Vec<String>,
}

#[cfg(test)]
impl RecordingInjector {
    pub(crate) fn new() -> Self {
        RecordingInjector { ops: Vec::new() }
    }

    pub(crate) fn ops(&self) -> &[String] {
        &self.ops
    }
}

#[cfg(test)]
impl TextInjector for RecordingInjector {
    fn insert(&mut self, text: &str) -> Result<(), BoxError> {
        self.ops.push(format!("insert:{text}"));
        Ok(())
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), BoxError> {
        self.ops.push(format!("replace:{old}->{new}"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_appends_to_ledger_and_injector() {
        let mut target = Target::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        assert_eq!(target.inserted(), "bonj");
        assert_eq!(target.injector().ops(), &["insert:bonj"]);
    }

    #[test]
    fn replace_matching_suffix_swaps_ledger() {
        let mut target = Target::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        target
            .apply_edit(&Edit::Replace {
                old: "bonj".into(),
                new: "bonjour".into(),
            })
            .unwrap();
        assert_eq!(target.inserted(), "bonjour");
        assert_eq!(
            target.injector().ops(),
            &["insert:bonj", "replace:bonj->bonjour"]
        );
    }

    #[test]
    fn replace_mismatching_suffix_is_desync_without_injecting() {
        let mut target = Target::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        let err = target
            .apply_edit(&Edit::Replace {
                old: "xyz".into(),
                new: "xyzzy".into(),
            })
            .unwrap_err();
        assert!(matches!(err, TargetError::Desync { .. }), "got: {err}");
        assert_eq!(target.inserted(), "bonj");
        assert_eq!(target.injector().ops(), &["insert:bonj"]);
    }

    #[test]
    fn retract_replaces_inserted_with_empty() {
        let mut target = Target::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        target
            .apply_edit(&Edit::Replace {
                old: "bonj".into(),
                new: "bonjour".into(),
            })
            .unwrap();
        target.retract().unwrap();
        assert_eq!(target.inserted(), "");
        assert_eq!(
            target.injector().ops(),
            &["insert:bonj", "replace:bonj->bonjour", "replace:bonjour->"]
        );
    }

    #[test]
    fn retract_empty_injects_nothing() {
        let mut target = Target::new(RecordingInjector::new());
        target.retract().unwrap();
        assert!(target.injector().ops().is_empty());
    }
}
