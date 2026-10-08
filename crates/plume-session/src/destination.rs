#[cfg(test)]
use plume_core::Edit;
use plume_core::{BoxError, InjectionReport, InsertionMode, TextInjector};

pub(crate) struct TextDestination<I: TextInjector> {
    injector: I,
    inserted: String,
}

impl<I: TextInjector> TextDestination<I> {
    pub(crate) fn new(injector: I) -> Self {
        TextDestination {
            injector,
            inserted: String::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn apply_edit(&mut self, edit: &Edit) -> Result<(), InsertionError> {
        self.apply_edit_with_mode(edit, InsertionMode::Typing)
            .map(|_| ())
    }

    #[cfg(test)]
    pub(crate) fn apply_edit_with_mode(
        &mut self,
        edit: &Edit,
        mode: InsertionMode,
    ) -> Result<Option<InjectionReport>, InsertionError> {
        match edit {
            Edit::Insert(text) => {
                let report = self
                    .injector
                    .insert_with_mode(text, mode)
                    .map_err(InsertionError::Inject)?;
                self.inserted.push_str(text);
                return Ok(Some(report));
            }
            Edit::Replace { old, new } => {
                if !self.inserted.ends_with(old.as_str()) {
                    return Err(InsertionError::Desync {
                        expected_suffix: old.clone(),
                    });
                }
                self.injector
                    .replace_last(old, new)
                    .map_err(InsertionError::Inject)?;
                self.inserted.truncate(self.inserted.len() - old.len());
                self.inserted.push_str(new);
            }
        }
        Ok(None)
    }

    pub(crate) fn copy_text(&mut self, text: &str) -> Result<(), BoxError> {
        self.injector.copy_text(text)
    }
    pub(crate) fn insert_checked(
        &mut self,
        text: &str,
        mode: InsertionMode,
        before_dispatch: &mut dyn FnMut() -> bool,
    ) -> Result<InjectionReport, BoxError> {
        self.injector.insert_checked(text, mode, before_dispatch)
    }

    #[cfg(test)]
    pub(crate) fn retract(&mut self) -> Result<(), BoxError> {
        if self.inserted.is_empty() {
            return Ok(());
        }
        let inserted = std::mem::take(&mut self.inserted);
        self.injector.replace_last(&inserted, "")
    }

    #[cfg(test)]
    pub(crate) fn inserted(&self) -> &str {
        &self.inserted
    }

    pub(crate) fn reset(&mut self) {
        self.inserted.clear();
    }
}

#[cfg(test)]
impl<I: TextInjector> TextDestination<I> {
    pub(crate) fn injector(&self) -> &I {
        &self.injector
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) enum InsertionError {
    Desync { expected_suffix: String },
    Inject(BoxError),
}

#[cfg(test)]
impl std::fmt::Display for InsertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsertionError::Desync { expected_suffix } => {
                write!(
                    f,
                    "target desync: {expected_suffix:?} is not a suffix of inserted text"
                )
            }
            InsertionError::Inject(err) => write!(f, "inject failed: {err}"),
        }
    }
}

#[cfg(test)]
impl std::error::Error for InsertionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InsertionError::Desync { .. } => None,
            InsertionError::Inject(err) => Some(err.as_ref()),
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
        let mut target = TextDestination::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        assert_eq!(target.inserted(), "bonj");
        assert_eq!(target.injector().ops(), &["insert:bonj"]);
    }

    #[test]
    fn replace_matching_suffix_swaps_ledger() {
        let mut target = TextDestination::new(RecordingInjector::new());
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
        let mut target = TextDestination::new(RecordingInjector::new());
        target.apply_edit(&Edit::Insert("bonj".into())).unwrap();
        let err = target
            .apply_edit(&Edit::Replace {
                old: "xyz".into(),
                new: "xyzzy".into(),
            })
            .unwrap_err();
        assert!(matches!(err, InsertionError::Desync { .. }), "got: {err}");
        assert_eq!(target.inserted(), "bonj");
        assert_eq!(target.injector().ops(), &["insert:bonj"]);
    }

    #[test]
    fn retract_replaces_inserted_with_empty() {
        let mut target = TextDestination::new(RecordingInjector::new());
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
        let mut target = TextDestination::new(RecordingInjector::new());
        target.retract().unwrap();
        assert!(target.injector().ops().is_empty());
    }
}
