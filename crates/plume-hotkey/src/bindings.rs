use crate::shortcut::Shortcut;
use crate::{BoxError, HotkeyError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    Hold,
    Toggle,
    Cancel,
}
#[derive(Clone, Debug)]
pub struct HotkeyBinding {
    pub action: HotkeyAction,
    pub shortcut: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingEvent {
    pub action: HotkeyAction,
    pub edge: plume_core::HotkeyEvent,
}

pub fn validate_bindings(bindings: &[HotkeyBinding]) -> Result<(), BoxError> {
    let mut seen = Vec::new();
    for binding in bindings {
        let key = Shortcut::parse(&binding.shortcut)?;
        if seen
            .iter()
            .any(|(action, shortcut)| *action == binding.action || same_keys(*shortcut, key))
        {
            return Err(HotkeyError::InvalidShortcut("dictation shortcuts overlap".into()).into());
        }
        seen.push((binding.action, key));
    }
    Ok(())
}

fn same_keys(a: Shortcut, b: Shortcut) -> bool {
    // For modifier-only bindings the order determines the trigger, not the chord identity.
    let modifier = |t| {
        matches!(
            t,
            crate::shortcut::Trigger::Ctrl
                | crate::shortcut::Trigger::Alt
                | crate::shortcut::Trigger::Shift
                | crate::shortcut::Trigger::Super
                | crate::shortcut::Trigger::Fn
        )
    };
    a.ctrl == b.ctrl
        && a.alt == b.alt
        && a.shift == b.shift
        && a.super_key == b.super_key
        && a.fn_key == b.fn_key
        && (a.trigger == b.trigger || (modifier(a.trigger) && modifier(b.trigger)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_modifier_order_collide() {
        for (a, b) in [("Ctrl+Space", "control+space"), ("Ctrl+Alt", "Alt+Ctrl")] {
            assert!(validate_bindings(&[
                HotkeyBinding {
                    action: HotkeyAction::Hold,
                    shortcut: a.into()
                },
                HotkeyBinding {
                    action: HotkeyAction::Toggle,
                    shortcut: b.into()
                }
            ])
            .is_err());
        }
    }
}
