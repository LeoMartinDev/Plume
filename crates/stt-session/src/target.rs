use stt_core::{BoxError, Edit, TextInjector};

pub struct Target<I: TextInjector> {
    injector: I,
    inserted: String,
}

impl<I: TextInjector> Target<I> {
    pub fn new(injector: I) -> Self {
        Self {
            injector,
            inserted: String::new(),
        }
    }

    pub fn apply_edit(&mut self, edit: &Edit) -> Result<(), TargetError> {
        match edit {
            Edit::Insert(text) => {
                self.injector.insert(text).map_err(TargetError::Inject)?;
                self.inserted.push_str(text);
                Ok(())
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
                let keep = self.inserted.len() - old.len();
                self.inserted.truncate(keep);
                self.inserted.push_str(new);
                Ok(())
            }
        }
    }

    pub fn retract(&mut self) -> Result<(), BoxError> {
        if self.inserted.is_empty() {
            return Ok(());
        }
        let old = std::mem::take(&mut self.inserted);
        self.injector.replace_last(&old, "")
    }

    pub fn inserted(&self) -> &str {
        &self.inserted
    }

    pub fn reset(&mut self) {
        self.inserted.clear();
    }

    #[cfg(test)]
    pub(crate) fn injector(&self) -> &I {
        &self.injector
    }
}

#[derive(Debug)]
pub enum TargetError {
    Desync { expected_suffix: String },
    Inject(BoxError),
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetError::Desync { expected_suffix } => {
                write!(f, "target desync: expected suffix {expected_suffix:?}")
            }
            TargetError::Inject(err) => write!(f, "{err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::{Call, RecordingInjector};

    #[test]
    fn insert_then_replace_suffix_tracks_the_ledger() {
        let mut target = Target::new(RecordingInjector::new());
        target
            .apply_edit(&Edit::Insert("bonj".into()))
            .expect("insert");
        target
            .apply_edit(&Edit::Replace {
                old: "bonj".into(),
                new: "bonjour".into(),
            })
            .expect("replace");
        assert_eq!(target.inserted(), "bonjour");
        assert_eq!(
            target.injector().calls,
            vec![
                Call::Insert("bonj".into()),
                Call::ReplaceLast("bonj".into(), "bonjour".into()),
            ]
        );
    }

    #[test]
    fn replace_with_the_wrong_suffix_is_a_desync() {
        let mut target = Target::new(RecordingInjector::new());
        target
            .apply_edit(&Edit::Insert("bonj".into()))
            .expect("insert");
        let err = target
            .apply_edit(&Edit::Replace {
                old: "xyz".into(),
                new: "bonjour".into(),
            })
            .unwrap_err();
        assert!(matches!(err, TargetError::Desync { .. }));
        assert_eq!(target.inserted(), "bonj");
        assert_eq!(target.injector().calls, vec![Call::Insert("bonj".into())]);
    }

    #[test]
    fn retract_replaces_the_ledger_with_empty() {
        let mut target = Target::new(RecordingInjector::new());
        target
            .apply_edit(&Edit::Insert("bonjour".into()))
            .expect("insert");
        target.retract().expect("retract");
        assert_eq!(target.inserted(), "");
        assert_eq!(
            target.injector().calls,
            vec![
                Call::Insert("bonjour".into()),
                Call::ReplaceLast("bonjour".into(), "".into()),
            ]
        );
    }
}
