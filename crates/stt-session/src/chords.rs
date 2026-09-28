use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};
use stt_hotkey::{global_hotkey, PlatformHotkey};

#[derive(Debug)]
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
        stt_hotkey::validate_shortcut(raw).map_err(|err| err.to_string())?;
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
    pub(crate) fn bind(spec: &ChordSpec) -> Result<Self, BoxError> {
        let mut hotkey = global_hotkey()?;
        hotkey.register(spec.as_str())?;
        Ok(Chord { hotkey })
    }

    pub(crate) fn next_event(&mut self) -> Option<HotkeyEvent> {
        self.hotkey.next_event()
    }
}

/// Windows installs one process-wide low-level hook, so a second
/// `PlatformHotkey` fails. `arm` returns a disarmed guard and `cancelled`
/// never fires. Linux opens two X11 connections, one per chord.
pub(crate) struct CancelGuard {
    #[cfg(not(windows))]
    hotkey: PlatformHotkey,
}

impl CancelGuard {
    pub(crate) fn arm(spec: &ChordSpec) -> Result<Self, BoxError> {
        #[cfg(windows)]
        {
            let _ = spec;
            use std::sync::Once;
            static LOG: Once = Once::new();
            LOG.call_once(|| {
                eprintln!(
                    "stt-session: Esc cancel is unavailable on Windows (one keyboard hook per process)"
                );
            });
            Ok(CancelGuard {})
        }
        #[cfg(not(windows))]
        {
            let mut hotkey = global_hotkey()?;
            hotkey.register(spec.as_str())?;
            Ok(CancelGuard { hotkey })
        }
    }

    pub(crate) fn cancelled(&mut self) -> bool {
        #[cfg(windows)]
        {
            false
        }
        #[cfg(not(windows))]
        {
            matches!(self.hotkey.next_event(), Some(HotkeyEvent::Pressed))
        }
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
