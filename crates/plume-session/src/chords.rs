use plume_core::BoxError;
use plume_hotkey::{global_hotkey, PlatformHotkey};

#[derive(Debug, Clone)]
pub(crate) struct ChordSpec {
    raw: String,
}

impl ChordSpec {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        if raw.trim().is_empty() {
            return Err("chord is empty".to_string());
        }
        if raw
            .split('+')
            .any(|part| part.trim().eq_ignore_ascii_case("fn"))
        {
            return Err("Fn is not grabbable: X11 refuses it, Windows never fires it".to_string());
        }
        plume_hotkey::validate_shortcut(raw).map_err(|err| err.to_string())?;
        Ok(ChordSpec {
            raw: raw.to_string(),
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.raw
    }
}

pub(crate) struct Chord {
    hotkey: PlatformHotkey,
}
impl Chord {
    pub fn bind(config: &crate::Config) -> Result<Self, BoxError> {
        let mut hotkey = global_hotkey()?;
        hotkey.register_bindings(&config.bindings())?;
        Ok(Self { hotkey })
    }
    pub fn register(&mut self, config: &crate::Config) -> Result<(), BoxError> {
        self.hotkey.register_bindings(&config.bindings())
    }
    pub fn next_event(&mut self) -> Option<plume_hotkey::BindingEvent> {
        self.hotkey.next_binding_event()
    }
    pub fn cancel_active(&mut self, active: bool) {
        self.hotkey.set_cancel_active(active);
    }
    pub fn passthrough(&mut self, active: bool) -> Result<(), BoxError> {
        self.hotkey.set_passthrough(active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_empty_and_fn() {
        assert!(ChordSpec::parse("").is_err());
        assert!(ChordSpec::parse("   ").is_err());
        assert!(ChordSpec::parse("Fn").is_err());
        assert!(ChordSpec::parse("Ctrl+Fn").is_err());
        assert_eq!(
            ChordSpec::parse("Ctrl+Space").unwrap().as_str(),
            "Ctrl+Space"
        );
        assert_eq!(ChordSpec::parse("Esc").unwrap().as_str(), "Esc");
    }

    #[test]
    fn parse_accepts_windows_key_and_caps_chords_at_three_keys() {
        assert_eq!(ChordSpec::parse("Super").unwrap().as_str(), "Super");
        assert_eq!(
            ChordSpec::parse("Ctrl+Super").unwrap().as_str(),
            "Ctrl+Super"
        );
        assert!(ChordSpec::parse("Ctrl+Alt+Shift").is_ok());
        assert!(ChordSpec::parse("Ctrl+Alt+Shift+Space").is_err());
    }
}
