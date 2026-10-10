use plume_core::{BoxError, InsertionMethod, InsertionMode};
/// Drives exactly one insertion. Snapshot failures may choose typing; once a
/// paste is dispatched, every error returns to recovery without a second insertion.
/// With nothing focused, the text is left on the clipboard instead.
pub(crate) trait Target {
    type Snapshot;
    fn snapshot(&mut self) -> Result<Self::Snapshot, BoxError>;
    /// The text to dispatch, or `None` when no control has the focus.
    fn prepare(&mut self, text: &str) -> Result<Option<String>, BoxError> {
        Ok(Some(text.to_owned()))
    }
    fn type_text(&mut self, text: &str) -> Result<(), BoxError>;
    fn stage(&mut self, text: &str) -> Result<(), BoxError>;
    fn version(&self) -> Result<u64, BoxError>;
    fn paste(&mut self) -> Result<(), BoxError>;
    fn allow_dispatch(&mut self) -> bool {
        true
    }
    fn consumed(&mut self);
    fn restore(&mut self, snapshot: Self::Snapshot, token: u64) -> Result<(), BoxError>;
}
pub(crate) fn deliver<T: Target>(
    target: &mut T,
    text: &str,
    mode: InsertionMode,
) -> Result<InsertionMethod, BoxError> {
    if mode == InsertionMode::Typing {
        let Some(prepared) = target.prepare(text)? else {
            return copy(target, text);
        };
        if !target.allow_dispatch() {
            return Err("dictation cancelled".into());
        }
        target.type_text(&prepared)?;
        return Ok(InsertionMethod::Typing);
    }
    let snapshot = match target.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) if mode == InsertionMode::Auto => {
            tracing::warn!("clipboard snapshot unavailable: {error}");
            let Some(prepared) = target.prepare(text)? else {
                return copy(target, text);
            };
            if !target.allow_dispatch() {
                return Err("dictation cancelled".into());
            }
            target.type_text(&prepared)?;
            return Ok(InsertionMethod::Typing);
        }
        Err(error) => return Err(error),
    };
    // The previous clipboard is not restored when copying: the text must
    // stay there for the user to paste.
    let Some(text) = target.prepare(text)? else {
        return copy(target, text);
    };
    target.stage(&text)?;
    let token = target.version()?;
    let allowed = target.allow_dispatch();
    let dispatched = if allowed {
        target.paste()
    } else {
        Err("dictation cancelled".into())
    };
    if allowed {
        target.consumed();
    }
    let restored = if target.version().is_ok_and(|current| current == token) {
        target.restore(snapshot, token)
    } else {
        Ok(())
    };
    if !allowed {
        // Report a failed clipboard restoration even when dispatch was cancelled.
        restored?;
        return Err("dictation cancelled".into());
    }
    dispatched?;
    restored?;
    Ok(InsertionMethod::Clipboard)
}
/// Nothing can take the text: leave it on the clipboard.
fn copy<T: Target>(target: &mut T, text: &str) -> Result<InsertionMethod, BoxError> {
    if !target.allow_dispatch() {
        return Err("dictation cancelled".into());
    }
    target.stage(text)?;
    Ok(InsertionMethod::Copied)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake {
        version: u64,
        typed: usize,
        staged: usize,
        pasted: usize,
        restored: usize,
        unsupported: bool,
        user_copy: bool,
        paste_error: bool,
        restore_error: bool,
        cancel_before_dispatch: bool,
        no_focus: bool,
    }
    impl Target for Fake {
        type Snapshot = ();
        fn snapshot(&mut self) -> Result<(), BoxError> {
            if self.unsupported {
                Err("unsupported snapshot".into())
            } else {
                Ok(())
            }
        }
        fn prepare(&mut self, text: &str) -> Result<Option<String>, BoxError> {
            Ok((!self.no_focus).then(|| text.to_owned()))
        }
        fn type_text(&mut self, _: &str) -> Result<(), BoxError> {
            self.typed += 1;
            Ok(())
        }
        fn stage(&mut self, _: &str) -> Result<(), BoxError> {
            self.staged += 1;
            self.version += 1;
            Ok(())
        }
        fn version(&self) -> Result<u64, BoxError> {
            Ok(self.version)
        }
        fn paste(&mut self) -> Result<(), BoxError> {
            self.pasted += 1;
            if self.paste_error {
                Err("dispatch failed".into())
            } else {
                Ok(())
            }
        }
        fn allow_dispatch(&mut self) -> bool {
            !self.cancel_before_dispatch
        }
        fn consumed(&mut self) {
            if self.user_copy {
                self.version += 1;
            }
        }
        fn restore(&mut self, _: (), _token: u64) -> Result<(), BoxError> {
            self.restored += 1;
            if self.restore_error {
                Err("restore failed".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn cancellation_before_dispatch_restores_clipboard_without_paste_or_typing() {
        for mode in [
            InsertionMode::Clipboard,
            InsertionMode::Auto,
            InsertionMode::Typing,
        ] {
            let mut target = Fake {
                cancel_before_dispatch: true,
                ..Default::default()
            };
            assert!(deliver(&mut target, "words", mode).is_err());
            assert_eq!((target.pasted, target.typed), (0, 0));
            assert_eq!(target.restored, usize::from(mode != InsertionMode::Typing));
        }
        let mut target = Fake {
            cancel_before_dispatch: true,
            restore_error: true,
            ..Default::default()
        };
        let error = deliver(&mut target, "words", InsertionMode::Clipboard).unwrap_err();
        assert_eq!(error.to_string(), "restore failed");
        assert_eq!((target.pasted, target.typed, target.restored), (0, 0, 1));
    }
    #[test]
    fn snapshot_failure_selects_typing_before_any_clipboard_change() {
        let mut target = Fake {
            unsupported: true,
            ..Default::default()
        };
        assert_eq!(
            deliver(&mut target, "words", InsertionMode::Auto).unwrap(),
            InsertionMethod::Typing
        );
        assert_eq!((target.typed, target.staged, target.pasted), (1, 0, 0));
        assert!(deliver(&mut target, "words", InsertionMode::Clipboard).is_err());
        assert_eq!(target.staged, 0);
    }
    #[test]
    fn paste_and_restore_errors_never_type_a_duplicate() {
        for (paste_error, restore_error) in [(true, false), (false, true)] {
            let mut target = Fake {
                paste_error,
                restore_error,
                ..Default::default()
            };
            assert!(deliver(&mut target, "words", InsertionMode::Auto).is_err());
            assert_eq!((target.typed, target.pasted, target.restored), (0, 1, 1));
        }
    }
    #[test]
    fn nothing_focused_leaves_the_text_on_the_clipboard() {
        for (mode, unsupported) in [
            (InsertionMode::Auto, false),
            (InsertionMode::Auto, true),
            (InsertionMode::Clipboard, false),
            (InsertionMode::Typing, false),
        ] {
            let mut target = Fake {
                no_focus: true,
                unsupported,
                ..Default::default()
            };
            assert_eq!(
                deliver(&mut target, "words", mode).unwrap(),
                InsertionMethod::Copied
            );
            assert_eq!(
                (target.staged, target.pasted, target.typed, target.restored),
                (1, 0, 0, 0)
            );
        }
        let mut target = Fake {
            no_focus: true,
            cancel_before_dispatch: true,
            ..Default::default()
        };
        assert!(deliver(&mut target, "words", InsertionMode::Auto).is_err());
        assert_eq!(target.staged, 0);
    }
    #[test]
    fn user_copy_is_preserved_and_normal_transactions_restore() {
        for user_copy in [false, true] {
            let mut target = Fake {
                user_copy,
                ..Default::default()
            };
            deliver(&mut target, "words", InsertionMode::Clipboard).unwrap();
            assert_eq!(target.restored, usize::from(!user_copy));
        }
    }
}
