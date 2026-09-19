use gpui::Modifiers;

/// Modifier bits copied off `gpui::Modifiers` so tests never build a `Keystroke`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModBits {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub fn_key: bool,
}

impl ModBits {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
        fn_key: false,
    };

    /// `platform` is Super. `function` is Fn.
    pub fn from_gpui(modifiers: Modifiers) -> Self {
        Self {
            ctrl: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            super_key: modifiers.platform,
            fn_key: modifiers.function,
        }
    }
}

/// Spelled chord. Only `classify_keydown` constructs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChordText(String);

impl ChordText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One keydown while Listening, already decoded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stroke {
    Escape,
    Repeat,
    ModifierOnly,
    Unknown,
    Chord(ChordText),
}

impl Stroke {
    pub fn chord_text(&self) -> Option<&str> {
        match self {
            Stroke::Chord(text) => Some(text.as_str()),
            _ => None,
        }
    }
}

// X11 emits mixed Keystroke.key names. Normalize case and aliases.
pub fn classify_keydown(is_held: bool, key: &str, mods: ModBits) -> Stroke {
    if is_held {
        return Stroke::Repeat;
    }
    let key = normalize_key(key);
    if is_escape(&key) {
        return Stroke::Escape;
    }
    if is_modifier_key(&key) {
        return Stroke::ModifierOnly;
    }
    match spell_trigger(&key) {
        Some(trigger) => Stroke::Chord(ChordText(spell_chord(mods, &trigger))),
        None => Stroke::Unknown,
    }
}

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase()
}

fn is_escape(key: &str) -> bool {
    matches!(key, "escape" | "esc")
}

fn is_modifier_key(key: &str) -> bool {
    let stem = key
        .strip_suffix("_l")
        .or_else(|| key.strip_suffix("_r"))
        .unwrap_or(key);
    matches!(
        stem,
        "control"
            | "ctrl"
            | "alt"
            | "option"
            | "shift"
            | "cmd"
            | "command"
            | "win"
            | "super"
            | "meta"
            | "platform"
            | "fn"
            | "function"
    )
}

fn spell_trigger(key: &str) -> Option<String> {
    match key {
        "space" | "spacebar" => Some("Space".to_string()),
        "tab" | "iso_left_tab" => Some("Tab".to_string()),
        "return" | "enter" => Some("Return".to_string()),
        other => {
            if let Some(n) = parse_function_key(other) {
                Some(format!("F{n}"))
            } else {
                let mut chars = other.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) if ch.is_ascii_lowercase() || ch.is_ascii_digit() => {
                        Some(ch.to_string())
                    }
                    _ => None,
                }
            }
        }
    }
}

fn parse_function_key(key: &str) -> Option<u8> {
    let rest = key.strip_prefix('f')?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u8 = rest.parse().ok()?;
    (1..=24).contains(&n).then_some(n)
}

fn spell_chord(mods: ModBits, trigger: &str) -> String {
    let mut parts = Vec::new();
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.super_key {
        parts.push("Super");
    }
    if mods.fn_key {
        parts.push("Fn");
    }
    parts.push(trigger);
    parts.join("+")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldPhase {
    Idle,
    Listening,
}

pub fn pill_label(phase: HoldPhase, committed_hold: &str) -> &str {
    match phase {
        HoldPhase::Listening => "Press keys\u{2026}",
        HoldPhase::Idle => committed_hold,
    }
}

pub fn pill_hint(phase: HoldPhase) -> &'static str {
    match phase {
        HoldPhase::Listening => "Esc cancels",
        HoldPhase::Idle => "Click the shortcut to change it.",
    }
}

/// What the GPUI shell must do after `HoldCapture::apply`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureEffect {
    None,
    StayListening,
    Cancelled,
    Offer(ChordText),
    Rejected(String),
}

/// Capture session. No focus handle, no prefs, no gpui types.
pub struct HoldCapture {
    phase: HoldPhase,
    reject: Option<String>,
}

impl HoldCapture {
    pub fn idle() -> Self {
        HoldCapture {
            phase: HoldPhase::Idle,
            reject: None,
        }
    }

    pub fn phase(&self) -> HoldPhase {
        self.phase
    }

    pub fn is_listening(&self) -> bool {
        matches!(self.phase, HoldPhase::Listening)
    }

    pub fn reject(&self) -> Option<&str> {
        self.reject.as_deref()
    }

    pub fn set_reject(&mut self, message: String) {
        self.reject = Some(message);
    }

    pub fn clear_reject(&mut self) {
        self.reject = None;
    }

    pub fn begin(&mut self) {
        self.phase = HoldPhase::Listening;
        self.clear_reject();
    }

    pub fn cancel(&mut self) {
        self.phase = HoldPhase::Idle;
        self.clear_reject();
    }

    pub fn apply(&mut self, stroke: Stroke) -> CaptureEffect {
        if !self.is_listening() {
            return CaptureEffect::None;
        }
        match stroke {
            Stroke::Repeat | Stroke::ModifierOnly => CaptureEffect::StayListening,
            Stroke::Escape => {
                self.cancel();
                CaptureEffect::Cancelled
            }
            Stroke::Unknown => {
                self.phase = HoldPhase::Idle;
                let message = "that key cannot be a hold trigger".to_string();
                self.set_reject(message.clone());
                CaptureEffect::Rejected(message)
            }
            Stroke::Chord(text) => {
                self.phase = HoldPhase::Idle;
                self.clear_reject();
                CaptureEffect::Offer(text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(ctrl: bool, alt: bool, shift: bool, super_key: bool, fn_key: bool) -> ModBits {
        ModBits {
            ctrl,
            alt,
            shift,
            super_key,
            fn_key,
        }
    }

    #[test]
    fn classify_table() {
        let cases = [
            (
                false,
                "space",
                bits(true, false, false, false, false),
                Stroke::Chord(ChordText("Ctrl+Space".into())),
            ),
            (
                false,
                "SPACE",
                bits(true, false, false, false, false),
                Stroke::Chord(ChordText("Ctrl+Space".into())),
            ),
            (
                false,
                "spacebar",
                bits(true, false, false, false, false),
                Stroke::Chord(ChordText("Ctrl+Space".into())),
            ),
            (
                false,
                "a",
                bits(false, true, false, false, false),
                Stroke::Chord(ChordText("Alt+a".into())),
            ),
            (
                false,
                "A",
                bits(false, true, false, false, false),
                Stroke::Chord(ChordText("Alt+a".into())),
            ),
            (
                false,
                "f9",
                ModBits::NONE,
                Stroke::Chord(ChordText("F9".into())),
            ),
            (
                false,
                "F9",
                ModBits::NONE,
                Stroke::Chord(ChordText("F9".into())),
            ),
            (
                false,
                "space",
                bits(false, false, false, false, true),
                Stroke::Chord(ChordText("Fn+Space".into())),
            ),
            (
                false,
                "tab",
                bits(false, false, false, true, false),
                Stroke::Chord(ChordText("Super+Tab".into())),
            ),
            (
                false,
                "enter",
                bits(true, true, true, true, true),
                Stroke::Chord(ChordText("Ctrl+Alt+Shift+Super+Fn+Return".into())),
            ),
            (false, "escape", ModBits::NONE, Stroke::Escape),
            (
                false,
                "ESC",
                bits(true, false, false, false, false),
                Stroke::Escape,
            ),
            (
                false,
                "esc",
                bits(false, true, false, false, false),
                Stroke::Escape,
            ),
            (
                true,
                "space",
                bits(true, false, false, false, false),
                Stroke::Repeat,
            ),
            (
                false,
                "control",
                bits(true, false, false, false, false),
                Stroke::ModifierOnly,
            ),
            (false, "ctrl", ModBits::NONE, Stroke::ModifierOnly),
            (false, "Control_L", ModBits::NONE, Stroke::ModifierOnly),
            (false, "function", ModBits::NONE, Stroke::ModifierOnly),
            (false, "platform", ModBits::NONE, Stroke::ModifierOnly),
            (false, "backspace", ModBits::NONE, Stroke::Unknown),
            (false, "left", ModBits::NONE, Stroke::Unknown),
            (false, "f0", ModBits::NONE, Stroke::Unknown),
            (false, "f25", ModBits::NONE, Stroke::Unknown),
        ];
        for (is_held, key, mods, want) in cases {
            let got = classify_keydown(is_held, key, mods);
            assert_eq!(got, want, "classify({is_held}, {key:?}, {mods:?})");
        }
    }

    #[test]
    fn ctrl_space_round_trips_the_prefs_default() {
        let stroke = classify_keydown(false, "space", bits(true, false, false, false, false));
        assert_eq!(stroke.chord_text(), Some("Ctrl+Space"));
    }

    #[test]
    fn alt_letter_is_lowercase() {
        let stroke = classify_keydown(false, "a", bits(false, true, false, false, false));
        assert_eq!(stroke.chord_text(), Some("Alt+a"));
    }

    #[test]
    fn f9_has_no_modifier() {
        assert_eq!(
            classify_keydown(false, "f9", ModBits::NONE).chord_text(),
            Some("F9")
        );
    }

    #[test]
    fn escape_cancels_even_with_ctrl() {
        assert_eq!(
            classify_keydown(false, "escape", bits(true, false, false, false, false)),
            Stroke::Escape
        );
    }

    #[test]
    fn fn_space_is_spelled_so_the_gate_can_reject_it() {
        let stroke = classify_keydown(false, "space", bits(false, false, false, false, true));
        assert_eq!(stroke.chord_text(), Some("Fn+Space"));
    }

    #[test]
    fn held_repeat_and_bare_ctrl_do_not_commit() {
        assert_eq!(
            classify_keydown(true, "space", bits(true, false, false, false, false)),
            Stroke::Repeat
        );
        assert_eq!(
            classify_keydown(false, "control", bits(true, false, false, false, false)),
            Stroke::ModifierOnly
        );
    }

    #[test]
    fn from_gpui_maps_platform_to_super_and_function_to_fn() {
        let bits = ModBits::from_gpui(Modifiers {
            control: true,
            platform: true,
            function: true,
            ..Modifiers::default()
        });
        assert_eq!(
            bits,
            ModBits {
                ctrl: true,
                alt: false,
                shift: false,
                super_key: true,
                fn_key: true,
            }
        );
    }

    #[test]
    fn pill_label_idle_versus_listening() {
        assert_eq!(pill_label(HoldPhase::Idle, "Ctrl+Space"), "Ctrl+Space");
        assert_eq!(
            pill_label(HoldPhase::Listening, "Ctrl+Space"),
            "Press keys\u{2026}"
        );
        assert_eq!(
            pill_hint(HoldPhase::Idle),
            "Click the shortcut to change it."
        );
        assert_eq!(pill_hint(HoldPhase::Listening), "Esc cancels");
    }

    #[test]
    fn machine_transitions() {
        let mut cap = HoldCapture::idle();
        assert!(!cap.is_listening());
        assert_eq!(cap.apply(Stroke::Escape), CaptureEffect::None);
        assert_eq!(cap.phase(), HoldPhase::Idle);

        cap.begin();
        assert!(cap.is_listening());
        cap.begin();
        assert!(cap.is_listening());
        assert_eq!(cap.reject(), None);

        assert_eq!(cap.apply(Stroke::Repeat), CaptureEffect::StayListening);
        assert!(cap.is_listening());
        assert_eq!(
            cap.apply(Stroke::ModifierOnly),
            CaptureEffect::StayListening
        );
        assert!(cap.is_listening());

        let chord = match classify_keydown(false, "space", bits(true, false, false, false, false)) {
            Stroke::Chord(text) => text,
            other => panic!("expected Chord, got {other:?}"),
        };
        match cap.apply(Stroke::Chord(chord)) {
            CaptureEffect::Offer(text) => assert_eq!(text.as_str(), "Ctrl+Space"),
            other => panic!("expected Offer, got {other:?}"),
        }
        assert!(!cap.is_listening());

        cap.begin();
        match cap.apply(Stroke::Unknown) {
            CaptureEffect::Rejected(message) => {
                assert_eq!(message, "that key cannot be a hold trigger");
                assert_eq!(cap.reject(), Some(message.as_str()));
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert!(!cap.is_listening());

        cap.begin();
        assert_eq!(cap.reject(), None);
        assert_eq!(cap.apply(Stroke::Escape), CaptureEffect::Cancelled);
        assert!(!cap.is_listening());
        assert_eq!(cap.reject(), None);
        assert_eq!(pill_label(cap.phase(), "Ctrl+Space"), "Ctrl+Space");
    }

    #[test]
    fn toggle_cancel_returns_to_idle() {
        let mut cap = HoldCapture::idle();
        cap.begin();
        cap.set_reject("stale".into());
        cap.begin();
        assert_eq!(cap.reject(), None);
        cap.cancel();
        assert!(!cap.is_listening());
        assert_eq!(cap.reject(), None);
        cap.cancel();
        assert!(!cap.is_listening());
    }
}
