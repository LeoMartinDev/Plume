use gpui::Modifiers;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShortcutModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub fn_key: bool,
}

impl ShortcutModifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
        fn_key: false,
    };

    pub fn from_gpui(modifiers: Modifiers) -> Self {
        Self {
            ctrl: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            super_key: modifiers.platform,
            fn_key: modifiers.function,
        }
    }

    fn count(self) -> usize {
        self.ctrl as usize
            + self.alt as usize
            + self.shift as usize
            + self.super_key as usize
            + self.fn_key as usize
    }

    fn contains(self, other: Self) -> bool {
        (!other.ctrl || self.ctrl)
            && (!other.alt || self.alt)
            && (!other.shift || self.shift)
            && (!other.super_key || self.super_key)
            && (!other.fn_key || self.fn_key)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChordText(String);

impl ChordText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stroke {
    Escape,
    Repeat,
    ModifierOnly,
    TooManyKeys,
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

pub fn classify_keydown(is_held: bool, key: &str, mods: ShortcutModifiers) -> Stroke {
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
    if mods.count() >= 3 {
        return Stroke::TooManyKeys;
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

fn spell_chord(mods: ShortcutModifiers, trigger: &str) -> String {
    let mut parts = spell_modifiers(mods);
    parts.push(trigger);
    parts.join("+")
}

fn spell_modifiers(mods: ShortcutModifiers) -> Vec<&'static str> {
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
    parts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutCaptureState {
    Idle,
    Listening,
}

pub fn pill_label(phase: ShortcutCaptureState, committed_hold: &str) -> &str {
    match phase {
        ShortcutCaptureState::Listening => "Press keys\u{2026}",
        ShortcutCaptureState::Idle => committed_hold,
    }
}

pub fn pill_hint(phase: ShortcutCaptureState) -> &'static str {
    match phase {
        ShortcutCaptureState::Listening => "Esc cancels",
        ShortcutCaptureState::Idle => "Click the shortcut to change it.",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureEffect {
    None,
    StayListening,
    Cancelled,
    Offer(ChordText),
    Rejected(String),
}

pub struct ShortcutCapture {
    phase: ShortcutCaptureState,
    reject: Option<String>,
    pending_modifiers: ShortcutModifiers,
    pending_chord: Option<ChordText>,
    preview: Option<ChordText>,
}

impl ShortcutCapture {
    pub fn idle() -> Self {
        ShortcutCapture {
            phase: ShortcutCaptureState::Idle,
            reject: None,
            pending_modifiers: ShortcutModifiers::NONE,
            pending_chord: None,
            preview: None,
        }
    }

    pub fn phase(&self) -> ShortcutCaptureState {
        self.phase
    }

    pub fn is_listening(&self) -> bool {
        matches!(self.phase, ShortcutCaptureState::Listening)
    }

    pub fn reject(&self) -> Option<&str> {
        self.reject.as_deref()
    }

    pub fn preview(&self) -> Option<&str> {
        self.preview.as_ref().map(ChordText::as_str)
    }

    pub fn set_reject(&mut self, message: String) {
        self.reject = Some(message);
    }

    pub fn clear_reject(&mut self) {
        self.reject = None;
    }

    pub fn begin(&mut self) {
        self.phase = ShortcutCaptureState::Listening;
        self.pending_modifiers = ShortcutModifiers::NONE;
        self.pending_chord = None;
        self.preview = None;
        self.clear_reject();
    }

    pub fn cancel(&mut self) {
        self.phase = ShortcutCaptureState::Idle;
        self.pending_modifiers = ShortcutModifiers::NONE;
        self.pending_chord = None;
        self.preview = None;
        self.clear_reject();
    }

    pub fn apply_modifiers(&mut self, modifiers: ShortcutModifiers) -> CaptureEffect {
        if !self.is_listening() {
            return CaptureEffect::None;
        }
        if modifiers.count() + usize::from(self.pending_chord.is_some()) > 3 {
            return self.reject_too_many_keys();
        }

        let previous = self.pending_modifiers;
        if modifiers.contains(previous) {
            self.pending_modifiers = modifiers;
            if let Some(chord) = self.pending_chord.as_mut() {
                let trigger = chord.as_str().rsplit('+').next().unwrap().to_string();
                *chord = ChordText(spell_chord(modifiers, &trigger));
                self.preview = Some(chord.clone());
                return CaptureEffect::StayListening;
            }
            let parts = spell_modifiers(modifiers);
            self.preview = (!parts.is_empty()).then(|| ChordText(parts.join("+")));
            return CaptureEffect::StayListening;
        }
        if previous.count() == 0 {
            return CaptureEffect::StayListening;
        }

        let chord = self
            .pending_chord
            .take()
            .unwrap_or_else(|| ChordText(spell_modifiers(previous).join("+")));
        self.cancel();
        CaptureEffect::Offer(chord)
    }

    fn reject_too_many_keys(&mut self) -> CaptureEffect {
        self.phase = ShortcutCaptureState::Idle;
        self.pending_modifiers = ShortcutModifiers::NONE;
        self.pending_chord = None;
        self.preview = None;
        let message = "shortcut can contain at most 3 keys".to_string();
        self.set_reject(message.clone());
        CaptureEffect::Rejected(message)
    }

    pub fn apply(&mut self, stroke: Stroke) -> CaptureEffect {
        if !self.is_listening() {
            return CaptureEffect::None;
        }
        match stroke {
            Stroke::Repeat | Stroke::ModifierOnly => CaptureEffect::StayListening,
            Stroke::TooManyKeys => self.reject_too_many_keys(),
            Stroke::Escape => {
                self.cancel();
                CaptureEffect::Cancelled
            }
            Stroke::Unknown => {
                self.cancel();
                let message = "that key cannot be a hold trigger".to_string();
                self.set_reject(message.clone());
                CaptureEffect::Rejected(message)
            }
            Stroke::Chord(text) => {
                // The keydown snapshot is authoritative even when no preceding
                // modifiers-changed event reached the focused input.
                let parts: Vec<_> = text.as_str().split('+').collect();
                let modifiers = &parts[..parts.len() - 1];
                self.pending_modifiers = ShortcutModifiers {
                    ctrl: modifiers.contains(&"Ctrl"),
                    alt: modifiers.contains(&"Alt"),
                    shift: modifiers.contains(&"Shift"),
                    super_key: modifiers.contains(&"Super"),
                    fn_key: modifiers.contains(&"Fn"),
                };
                if self.pending_chord.as_ref().is_some_and(|pending| {
                    pending.as_str().rsplit('+').next() != text.as_str().rsplit('+').next()
                }) {
                    self.cancel();
                    let message = "shortcut can contain only one non-modifier key".to_string();
                    self.set_reject(message.clone());
                    return CaptureEffect::Rejected(message);
                }
                self.preview = Some(text.clone());
                self.pending_chord = Some(text);
                CaptureEffect::StayListening
            }
        }
    }

    pub fn apply_key_up(&mut self, key: &str) -> CaptureEffect {
        if !self.is_listening() {
            return CaptureEffect::None;
        }
        let trigger = spell_trigger(&normalize_key(key));
        if self
            .pending_chord
            .as_ref()
            .is_none_or(|chord| chord.as_str().rsplit('+').next() != trigger.as_deref())
        {
            return CaptureEffect::StayListening;
        }
        let Some(chord) = self.pending_chord.take() else {
            return CaptureEffect::StayListening;
        };
        self.phase = ShortcutCaptureState::Idle;
        self.pending_modifiers = ShortcutModifiers::NONE;
        self.preview = None;
        self.clear_reject();
        CaptureEffect::Offer(chord)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(
        ctrl: bool,
        alt: bool,
        shift: bool,
        super_key: bool,
        fn_key: bool,
    ) -> ShortcutModifiers {
        ShortcutModifiers {
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
                ShortcutModifiers::NONE,
                Stroke::Chord(ChordText("F9".into())),
            ),
            (
                false,
                "F9",
                ShortcutModifiers::NONE,
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
                Stroke::TooManyKeys,
            ),
            (false, "escape", ShortcutModifiers::NONE, Stroke::Escape),
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
            (false, "ctrl", ShortcutModifiers::NONE, Stroke::ModifierOnly),
            (
                false,
                "Control_L",
                ShortcutModifiers::NONE,
                Stroke::ModifierOnly,
            ),
            (
                false,
                "function",
                ShortcutModifiers::NONE,
                Stroke::ModifierOnly,
            ),
            (
                false,
                "platform",
                ShortcutModifiers::NONE,
                Stroke::ModifierOnly,
            ),
            (false, "backspace", ShortcutModifiers::NONE, Stroke::Unknown),
            (false, "left", ShortcutModifiers::NONE, Stroke::Unknown),
            (false, "f0", ShortcutModifiers::NONE, Stroke::Unknown),
            (false, "f25", ShortcutModifiers::NONE, Stroke::Unknown),
        ];
        for (is_held, key, mods, want) in cases {
            let got = classify_keydown(is_held, key, mods);
            assert_eq!(got, want, "classify({is_held}, {key:?}, {mods:?})");
        }
    }

    #[test]
    fn rejects_more_than_three_keys() {
        assert_eq!(
            classify_keydown(false, "space", bits(true, true, true, false, false)),
            Stroke::TooManyKeys
        );
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
            classify_keydown(false, "f9", ShortcutModifiers::NONE).chord_text(),
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
        let bits = ShortcutModifiers::from_gpui(Modifiers {
            control: true,
            platform: true,
            function: true,
            ..Modifiers::default()
        });
        assert_eq!(
            bits,
            ShortcutModifiers {
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
        assert_eq!(
            pill_label(ShortcutCaptureState::Idle, "Ctrl+Space"),
            "Ctrl+Space"
        );
        assert_eq!(
            pill_label(ShortcutCaptureState::Listening, "Ctrl+Space"),
            "Press keys\u{2026}"
        );
        assert_eq!(
            pill_hint(ShortcutCaptureState::Idle),
            "Click the shortcut to change it."
        );
        assert_eq!(pill_hint(ShortcutCaptureState::Listening), "Esc cancels");
    }

    #[test]
    fn machine_transitions() {
        let mut cap = ShortcutCapture::idle();
        assert!(!cap.is_listening());
        assert_eq!(cap.apply(Stroke::Escape), CaptureEffect::None);
        assert_eq!(cap.phase(), ShortcutCaptureState::Idle);

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
        assert_eq!(
            cap.apply(Stroke::Chord(chord)),
            CaptureEffect::StayListening
        );
        assert_eq!(cap.preview(), Some("Ctrl+Space"));
        assert!(cap.is_listening());
        match cap.apply_key_up("space") {
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
        let mut cap = ShortcutCapture::idle();
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

    #[test]
    fn modifier_only_capture_commits_on_first_release() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        assert_eq!(
            cap.apply_modifiers(bits(false, false, false, true, false)),
            CaptureEffect::StayListening
        );
        assert_eq!(cap.preview(), Some("Super"));
        assert_eq!(
            cap.apply_modifiers(bits(true, false, false, true, false)),
            CaptureEffect::StayListening
        );
        assert_eq!(cap.preview(), Some("Ctrl+Super"));
        match cap.apply_modifiers(bits(false, false, false, true, false)) {
            CaptureEffect::Offer(text) => assert_eq!(text.as_str(), "Ctrl+Super"),
            other => panic!("expected Offer, got {other:?}"),
        }
        assert!(!cap.is_listening());
    }

    #[test]
    fn windows_key_alone_is_captured() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply_modifiers(bits(false, false, false, true, false));
        assert_eq!(cap.preview(), Some("Super"));
        match cap.apply_modifiers(ShortcutModifiers::NONE) {
            CaptureEffect::Offer(text) => assert_eq!(text.as_str(), "Super"),
            other => panic!("expected Offer, got {other:?}"),
        }
    }

    #[test]
    fn chord_commits_when_a_modifier_is_released_before_the_trigger() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply(classify_keydown(
            false,
            "space",
            bits(true, false, true, false, false),
        ));
        assert_eq!(cap.preview(), Some("Ctrl+Shift+Space"));
        assert_eq!(
            cap.apply_modifiers(bits(true, false, false, false, false)),
            CaptureEffect::Offer(ChordText("Ctrl+Shift+Space".into()))
        );
        assert_eq!(cap.apply_key_up("space"), CaptureEffect::None);
    }

    #[test]
    fn modifiers_pressed_after_the_trigger_complete_the_chord() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply(classify_keydown(false, "space", ShortcutModifiers::NONE));
        cap.apply_modifiers(bits(true, false, false, false, false));
        cap.apply_modifiers(bits(true, false, true, false, false));
        assert_eq!(cap.preview(), Some("Ctrl+Shift+Space"));
        assert_eq!(cap.apply_key_up("shift"), CaptureEffect::StayListening);
        assert_eq!(
            cap.apply_key_up("space"),
            CaptureEffect::Offer(ChordText("Ctrl+Shift+Space".into()))
        );
    }

    #[test]
    fn adding_a_fourth_key_after_the_trigger_is_rejected() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply(classify_keydown(
            false,
            "a",
            bits(true, true, false, false, false),
        ));
        assert!(matches!(
            cap.apply_modifiers(bits(true, true, true, false, false)),
            CaptureEffect::Rejected(_)
        ));
    }

    #[test]
    fn a_second_non_modifier_key_cannot_replace_the_first() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply(classify_keydown(false, "a", ShortcutModifiers::NONE));
        assert!(matches!(
            cap.apply(classify_keydown(false, "b", ShortcutModifiers::NONE)),
            CaptureEffect::Rejected(_)
        ));
        assert_eq!(cap.preview(), None);
    }

    #[test]
    fn changing_modifiers_with_the_same_count_commits_the_original_chord() {
        let mut cap = ShortcutCapture::idle();
        cap.begin();
        cap.apply_modifiers(bits(true, false, false, false, false));
        assert_eq!(
            cap.apply_modifiers(bits(false, true, false, false, false)),
            CaptureEffect::Offer(ChordText("Ctrl".into()))
        );
    }
}
