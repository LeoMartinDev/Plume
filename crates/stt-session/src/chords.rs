use stt_core::{BoxError, GlobalHotkey, HotkeyEvent};
use stt_hotkey::{global_hotkey, PlatformHotkey};

/// A shortcut string validated at the config boundary: non-empty, parses
/// per the `stt-hotkey` grammar, and never `Fn`.
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

/// One persistent OS grab. Exists because `GlobalHotkey::register` takes
/// a single string and `HotkeyEvent` carries no key identity: two chords
/// cannot share one handle and stay distinguishable.
pub(crate) struct Chord {
    hotkey: PlatformHotkey,
}

impl Chord {
    /// Connect a fresh OS handle and register exactly this chord.
    pub(crate) fn bind(spec: &ChordSpec) -> Result<Self, BoxError> {
        let mut hotkey = global_hotkey()?;
        hotkey.register(spec.as_str())?;
        Ok(Chord { hotkey })
    }

    /// Non-blocking; `None` means no edge since the last call.
    /// Auto-repeat was already dropped at the hotkey boundary.
    pub(crate) fn next_event(&mut self) -> Option<HotkeyEvent> {
        self.hotkey.next_event()
    }
}

/// The cancel chord (`Esc`), armed per session. Constructed on hold,
/// dropped at settle; dropping closes the OS handle, which releases the
/// grab. `Esc` is therefore stealable by no idle compositor, and no
/// second `register` call can collide with the hold grab.
///
/// Windows installs a single process-wide low-level hook (a second
/// `PlatformHotkey` fails), so there `arm` returns a disarmed guard and
/// `cancelled` never fires: v1 has no Esc-cancel on Windows. Linux opens
/// two X11 connections, one per chord.
pub(crate) struct CancelGuard {
    #[cfg(not(windows))]
    hotkey: PlatformHotkey,
}

impl CancelGuard {
    pub(crate) fn arm(spec: &ChordSpec) -> Result<Self, BoxError> {
        #[cfg(windows)]
        {
            let _ = spec;
            Ok(CancelGuard {})
        }
        #[cfg(not(windows))]
        {
            let mut hotkey = global_hotkey()?;
            hotkey.register(spec.as_str())?;
            Ok(CancelGuard { hotkey })
        }
    }

    /// Non-blocking. Only `Pressed` matters; the guard never reads Release.
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
