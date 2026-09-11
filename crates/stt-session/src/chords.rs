use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};
use stt_hotkey::{global_hotkey, PlatformHotkey};

#[derive(Debug)]
pub struct ChordSpec {
    raw: String,
}

impl ChordSpec {
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw
            .split('+')
            .any(|token| token.trim().eq_ignore_ascii_case("fn"))
        {
            return Err("Fn is not grabbable (X11 refuses it, Windows swallows it)".into());
        }
        stt_hotkey::validate_shortcut(raw).map_err(|err| err.to_string())?;
        Ok(Self {
            raw: raw.to_string(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

pub struct Chord {
    hotkey: PlatformHotkey,
}

impl Chord {
    pub fn bind(spec: &ChordSpec) -> Result<Self, BoxError> {
        let mut hotkey = global_hotkey()?;
        hotkey.register(spec.as_str())?;
        Ok(Chord { hotkey })
    }

    pub fn next_event(&mut self) -> Option<HotkeyEvent> {
        self.hotkey.next_event()
    }
}

pub struct CancelGuard {
    hotkey: PlatformHotkey,
}

impl CancelGuard {
    pub fn arm(spec: &ChordSpec) -> Result<Self, BoxError> {
        let mut hotkey = global_hotkey()?;
        hotkey.register(spec.as_str())?;
        Ok(CancelGuard { hotkey })
    }

    pub fn cancelled(&mut self) -> bool {
        matches!(self.hotkey.next_event(), Some(HotkeyEvent::Pressed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse() {
        assert_eq!(ChordSpec::parse("Ctrl+Space").unwrap().as_str(), "Ctrl+Space");
        assert_eq!(ChordSpec::parse("Esc").unwrap().as_str(), "Esc");
    }

    #[test]
    fn fn_is_rejected_alone_and_in_any_position() {
        for raw in ["Fn", "fn", "Ctrl+Fn", "Fn+Space", "Ctrl+Fn+Space"] {
            assert!(ChordSpec::parse(raw).is_err(), "must reject {raw}");
        }
    }

    #[test]
    fn malformed_chords_are_rejected() {
        for raw in ["", "Ctrl+", "Ctrl+Space+A", "Ctrl+Alt", "F99"] {
            assert!(ChordSpec::parse(raw).is_err(), "must reject {raw:?}");
        }
    }
}
