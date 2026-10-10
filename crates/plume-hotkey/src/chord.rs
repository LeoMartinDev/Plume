use plume_core::HotkeyEvent;

use crate::shortcut::{KeyId, Shortcut, Trigger};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub fn_key: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
struct Down {
    mods: Mods,
    trigger: Option<Trigger>,
}

#[derive(Debug)]
#[cfg(any(test, target_os = "linux"))]
pub(crate) struct RepeatFilter {
    held: bool,
}

#[cfg(any(test, target_os = "linux"))]
impl RepeatFilter {
    pub(crate) fn new() -> Self {
        RepeatFilter { held: false }
    }

    pub(crate) fn push(&mut self, edge: Edge) -> Option<HotkeyEvent> {
        match (self.held, edge) {
            (false, Edge::Down) => {
                self.held = true;
                Some(HotkeyEvent::Pressed)
            }
            (true, Edge::Down) => None,
            (true, Edge::Up) => {
                self.held = false;
                Some(HotkeyEvent::Released)
            }
            (false, Edge::Up) => None,
        }
    }
}

impl Mods {
    fn all() -> Self {
        Mods {
            ctrl: true,
            alt: true,
            shift: true,
            super_key: true,
            fn_key: true,
        }
    }

    pub(crate) fn union(self, other: Mods) -> Mods {
        Mods {
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            super_key: self.super_key || other.super_key,
            fn_key: self.fn_key || other.fn_key,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ChordTracker {
    wanted: Shortcut,
    down: Down,
    engaged: bool,
    trigger_down: bool,
    /// Modifiers that may be held on top of the chord when it starts.
    tolerated: Mods,
}

#[allow(dead_code)]
impl ChordTracker {
    pub(crate) fn new(wanted: Shortcut) -> Self {
        ChordTracker {
            wanted,
            down: Down::default(),
            engaged: false,
            trigger_down: false,
            tolerated: Mods::default(),
        }
    }

    pub(crate) fn owns_trigger(&self, id: KeyId) -> bool {
        key_is_trigger(id, self.wanted.trigger)
    }
    pub(crate) fn engaged(&self) -> bool {
        self.engaged
    }
    /// The key whose release ends this chord.
    pub(crate) fn trigger_key(&self) -> KeyId {
        trigger_key(self.wanted.trigger)
    }
    /// Every modifier the chord holds down, its trigger included.
    pub(crate) fn chord_mods(&self) -> Mods {
        Mods {
            ctrl: self.wanted.ctrl,
            alt: self.wanted.alt,
            shift: self.wanted.shift,
            super_key: self.wanted.super_key,
            fn_key: self.wanted.fn_key,
        }
    }
    pub(crate) fn tolerate(&mut self, extra: Mods) {
        self.tolerated = extra;
    }

    pub(crate) fn push(&mut self, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        apply_id(&mut self.down, self.wanted.trigger, id, edge);
        self.emit(id, edge)
    }

    pub(crate) fn push_macos(&mut self, mods: Mods, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        apply_id(&mut self.down, self.wanted.trigger, id, edge);
        // Flags describe every modifier, including one pressed before this
        // tracker was registered or whose flags-changed event was missed.
        if let Some(released) = self.sync_mods(mods, Mods::all()) {
            return Some(released);
        }
        self.emit(id, edge)
    }

    /// Overwrite the `trusted` modifiers with a reading of the physical
    /// keyboard. A reading never starts a chord, but it ends one whose
    /// trigger modifier is no longer held.
    pub(crate) fn sync_mods(&mut self, mods: Mods, trusted: Mods) -> Option<HotkeyEvent> {
        let held = &mut self.down.mods;
        for (field, value, trust) in [
            (&mut held.ctrl, mods.ctrl, trusted.ctrl),
            (&mut held.alt, mods.alt, trusted.alt),
            (&mut held.shift, mods.shift, trusted.shift),
            (&mut held.super_key, mods.super_key, trusted.super_key),
            (&mut held.fn_key, mods.fn_key, trusted.fn_key),
        ] {
            if trust {
                *field = value;
            }
        }
        let trigger_down = match self.wanted.trigger {
            Trigger::Ctrl if trusted.ctrl => Some(mods.ctrl),
            Trigger::Alt if trusted.alt => Some(mods.alt),
            Trigger::Shift if trusted.shift => Some(mods.shift),
            Trigger::Super if trusted.super_key => Some(mods.super_key),
            Trigger::Fn if trusted.fn_key => Some(mods.fn_key),
            _ => None,
        };
        let on = trigger_down?;
        self.down.trigger = on.then_some(self.wanted.trigger);
        if self.engaged && !on {
            self.engaged = false;
            return Some(HotkeyEvent::Released);
        }
        None
    }

    /// The trigger is known (or must be assumed) to be up although its
    /// release never arrived. The next press of the trigger starts afresh.
    pub(crate) fn release(&mut self) -> Option<HotkeyEvent> {
        let key = self.trigger_key();
        apply_id(&mut self.down, self.wanted.trigger, key, Edge::Up);
        self.trigger_down = false;
        if self.engaged {
            self.engaged = false;
            Some(HotkeyEvent::Released)
        } else {
            None
        }
    }

    /// Forget every key, as after a period in which no edge was observed.
    pub(crate) fn forget(&mut self) -> Option<HotkeyEvent> {
        let released = self.release();
        self.down = Down::default();
        released
    }

    /// A capture begins only when the whole chord matches.  Once it has
    /// begun, however, its lifetime belongs to the trigger key.  Modifier
    /// state can be reported out of order by platform hooks (and may change
    /// while a chord is held); treating that as a release made a bubble flash
    /// on screen and stopped recording immediately.
    fn emit(&mut self, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        let is_trigger = key_is_trigger(id, self.wanted.trigger);
        let fresh_trigger = is_trigger && edge == Edge::Down && !self.trigger_down;
        if is_trigger {
            self.trigger_down = edge == Edge::Down;
        }
        if self.engaged {
            if matches!(edge, Edge::Up) && key_is_trigger(id, self.wanted.trigger) {
                self.engaged = false;
                return Some(HotkeyEvent::Released);
            }
            return None;
        }
        let modifier_trigger = matches!(
            self.wanted.trigger,
            Trigger::Ctrl | Trigger::Alt | Trigger::Shift | Trigger::Super | Trigger::Fn
        );
        let modifier_edge = matches!(
            id,
            KeyId::Ctrl | KeyId::Alt | KeyId::Shift | KeyId::Super | KeyId::Fn
        );
        if (fresh_trigger || modifier_trigger && modifier_edge && edge == Edge::Down)
            && matches_shortcut(self.wanted, self.down, self.tolerated)
        {
            self.engaged = true;
            Some(HotkeyEvent::Pressed)
        } else {
            None
        }
    }
}

#[allow(dead_code)]
fn apply_id(down: &mut Down, wanted_trigger: Trigger, id: KeyId, edge: Edge) {
    let on = matches!(edge, Edge::Down);
    match id {
        KeyId::Ctrl => {
            down.mods.ctrl = on;
            apply_modifier_trigger(down, wanted_trigger, Trigger::Ctrl, on);
        }
        KeyId::Alt => {
            down.mods.alt = on;
            apply_modifier_trigger(down, wanted_trigger, Trigger::Alt, on);
        }
        KeyId::Shift => {
            down.mods.shift = on;
            apply_modifier_trigger(down, wanted_trigger, Trigger::Shift, on);
        }
        KeyId::Super => {
            down.mods.super_key = on;
            apply_modifier_trigger(down, wanted_trigger, Trigger::Super, on);
        }
        KeyId::Fn => {
            down.mods.fn_key = on;
            apply_modifier_trigger(down, wanted_trigger, Trigger::Fn, on);
        }
        KeyId::Trigger(trigger) => {
            if on {
                if trigger == wanted_trigger {
                    down.trigger = Some(trigger);
                }
            } else if down.trigger == Some(trigger) {
                down.trigger = None;
            }
        }
    }
}

fn apply_modifier_trigger(down: &mut Down, wanted: Trigger, actual: Trigger, on: bool) {
    if wanted == actual {
        down.trigger = if on { Some(actual) } else { None };
    }
}

/// Every modifier of the chord is held, and any other held modifier is tolerated.
#[allow(dead_code)]
fn matches_shortcut(wanted: Shortcut, down: Down, tolerated: Mods) -> bool {
    let modifier = |want: bool, held: bool, extra: bool| want == held || held && extra;
    modifier(wanted.ctrl, down.mods.ctrl, tolerated.ctrl)
        && modifier(wanted.alt, down.mods.alt, tolerated.alt)
        && modifier(wanted.shift, down.mods.shift, tolerated.shift)
        && modifier(wanted.super_key, down.mods.super_key, tolerated.super_key)
        && modifier(wanted.fn_key, down.mods.fn_key, tolerated.fn_key)
        && down.trigger == Some(wanted.trigger)
}

fn trigger_key(trigger: Trigger) -> KeyId {
    match trigger {
        Trigger::Ctrl => KeyId::Ctrl,
        Trigger::Alt => KeyId::Alt,
        Trigger::Shift => KeyId::Shift,
        Trigger::Super => KeyId::Super,
        Trigger::Fn => KeyId::Fn,
        other => KeyId::Trigger(other),
    }
}

#[allow(dead_code)]
fn key_is_trigger(id: KeyId, trigger: Trigger) -> bool {
    matches!(
        (id, trigger),
        (KeyId::Ctrl, Trigger::Ctrl)
            | (KeyId::Alt, Trigger::Alt)
            | (KeyId::Shift, Trigger::Shift)
            | (KeyId::Super, Trigger::Super)
            | (KeyId::Fn, Trigger::Fn)
    ) || matches!(id, KeyId::Trigger(actual) if actual == trigger)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_space() -> Shortcut {
        Shortcut::parse("Ctrl+Space").unwrap()
    }

    #[test]
    fn repeat_filter_one_pressed_per_hold() {
        let mut filter = RepeatFilter::new();
        assert_eq!(filter.push(Edge::Down), Some(HotkeyEvent::Pressed));
        assert_eq!(filter.push(Edge::Down), None);
        assert_eq!(filter.push(Edge::Down), None);
        assert_eq!(filter.push(Edge::Up), Some(HotkeyEvent::Released));
        assert_eq!(filter.push(Edge::Up), None);
    }

    #[test]
    fn chord_ctrl_space_filters_auto_repeat() {
        let mut tracker = ChordTracker::new(ctrl_space());
        assert_eq!(tracker.push(KeyId::Ctrl, Edge::Down), None);
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Down),
            None
        );
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Up),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn chord_stays_engaged_until_its_trigger_is_released() {
        let mut tracker = ChordTracker::new(ctrl_space());
        assert_eq!(tracker.push(KeyId::Ctrl, Edge::Down), None);
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        // A modifier event after capture begins must not end the session.
        assert_eq!(tracker.push(KeyId::Ctrl, Edge::Up), None);
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Up),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn extra_shift_blocks_ctrl_space() {
        let mut tracker = ChordTracker::new(ctrl_space());
        tracker.push(KeyId::Ctrl, Edge::Down);
        tracker.push(KeyId::Shift, Edge::Down);
        assert_eq!(
            tracker.push(KeyId::Trigger(Trigger::Space), Edge::Down),
            None
        );
    }

    #[test]
    fn fn_alone_uses_fn_as_trigger() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Fn").unwrap());
        assert_eq!(
            tracker.push(KeyId::Fn, Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(tracker.push(KeyId::Fn, Edge::Down), None);
        assert_eq!(
            tracker.push(KeyId::Fn, Edge::Up),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn super_alone_and_ctrl_super_release_on_the_trigger() {
        let mut tracker = ChordTracker::new(Shortcut::parse("Win").unwrap());
        assert_eq!(
            tracker.push(KeyId::Super, Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(
            tracker.push(KeyId::Super, Edge::Up),
            Some(HotkeyEvent::Released)
        );

        let mut tracker = ChordTracker::new(Shortcut::parse("Ctrl+Win").unwrap());
        assert_eq!(tracker.push(KeyId::Ctrl, Edge::Down), None);
        assert_eq!(
            tracker.push(KeyId::Super, Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(tracker.push(KeyId::Ctrl, Edge::Up), None);
        assert_eq!(
            tracker.push(KeyId::Super, Edge::Up),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn macos_flags_snapshot_matches_ctrl_space() {
        let mut tracker = ChordTracker::new(ctrl_space());
        let mods = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert_eq!(
            tracker.push_macos(mods, KeyId::Trigger(Trigger::Space), Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(
            tracker.push_macos(mods, KeyId::Trigger(Trigger::Space), Edge::Down),
            None
        );
        assert_eq!(
            tracker.push_macos(mods, KeyId::Trigger(Trigger::Space), Edge::Up),
            Some(HotkeyEvent::Released)
        );
    }
}

#[cfg(test)]
mod fresh_press_tests {
    use super::*;
    #[test]
    fn ordinary_key_cannot_start_an_already_held_modifier_shortcut() {
        let mut t = ChordTracker::new(Shortcut::parse("Alt").unwrap());
        assert_eq!(t.push(KeyId::Ctrl, Edge::Down), None);
        assert_eq!(t.push(KeyId::Alt, Edge::Down), None);
        assert_eq!(t.push(KeyId::Ctrl, Edge::Up), None);
        assert_eq!(t.push(KeyId::Trigger(Trigger::Char('a')), Edge::Down), None);
        t.push(KeyId::Alt, Edge::Up);
        assert_eq!(t.push(KeyId::Alt, Edge::Down), Some(HotkeyEvent::Pressed));
    }
    #[test]
    fn tolerated_modifiers_may_be_held_but_required_ones_must_be() {
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let mut esc = ChordTracker::new(Shortcut::parse("Esc").unwrap());
        esc.push(KeyId::Ctrl, Edge::Down);
        assert_eq!(esc.push(KeyId::Trigger(Trigger::Escape), Edge::Down), None);
        esc.push(KeyId::Trigger(Trigger::Escape), Edge::Up);
        esc.tolerate(ctrl);
        assert_eq!(
            esc.push(KeyId::Trigger(Trigger::Escape), Edge::Down),
            Some(HotkeyEvent::Pressed)
        );

        let mut ctrl_q = ChordTracker::new(Shortcut::parse("Ctrl+q").unwrap());
        ctrl_q.tolerate(ctrl);
        assert_eq!(t_char(&mut ctrl_q, 'q', Edge::Down), None);
    }
    fn t_char(t: &mut ChordTracker, ch: char, edge: Edge) -> Option<HotkeyEvent> {
        t.push(KeyId::Trigger(Trigger::Char(ch)), edge)
    }
    #[test]
    fn modifier_reading_releases_but_never_presses() {
        let mut t = ChordTracker::new(Shortcut::parse("Ctrl+Alt").unwrap());
        let both = Mods {
            ctrl: true,
            alt: true,
            ..Mods::default()
        };
        assert_eq!(t.sync_mods(both, Mods::all()), None);
        assert!(!t.engaged());
        let mut t = ChordTracker::new(Shortcut::parse("Ctrl+Alt").unwrap());
        t.push(KeyId::Ctrl, Edge::Down);
        assert_eq!(t.push(KeyId::Alt, Edge::Down), Some(HotkeyEvent::Pressed));
        // An untrusted modifier is left alone.
        let untrusted_alt = Mods {
            alt: false,
            ..Mods::all()
        };
        assert_eq!(t.sync_mods(Mods::default(), untrusted_alt), None);
        assert!(t.engaged());
        assert_eq!(
            t.sync_mods(Mods::default(), Mods::all()),
            Some(HotkeyEvent::Released)
        );
    }
    #[test]
    fn a_lost_release_is_recovered_and_the_next_press_is_fresh() {
        let mut t = ChordTracker::new(Shortcut::parse("Ctrl+Shift+i").unwrap());
        t.push(KeyId::Ctrl, Edge::Down);
        t.push(KeyId::Shift, Edge::Down);
        assert_eq!(t_char(&mut t, 'i', Edge::Down), Some(HotkeyEvent::Pressed));
        // Every release went to a window the hook cannot observe.
        assert_eq!(t.forget(), Some(HotkeyEvent::Released));
        assert_eq!(t.forget(), None);
        t.push(KeyId::Ctrl, Edge::Down);
        t.push(KeyId::Shift, Edge::Down);
        assert_eq!(t_char(&mut t, 'i', Edge::Down), Some(HotkeyEvent::Pressed));
        assert_eq!(t_char(&mut t, 'i', Edge::Up), Some(HotkeyEvent::Released));

        assert_eq!(t_char(&mut t, 'i', Edge::Down), Some(HotkeyEvent::Pressed));
        assert_eq!(t.release(), Some(HotkeyEvent::Released));
        assert_eq!(t_char(&mut t, 'i', Edge::Down), Some(HotkeyEvent::Pressed));
    }
    #[test]
    fn completing_modifiers_on_a_held_trigger_does_not_start() {
        let mut t = ChordTracker::new(Shortcut::parse("Ctrl+Space").unwrap());
        assert_eq!(t.push(KeyId::Trigger(Trigger::Space), Edge::Down), None);
        assert_eq!(t.push(KeyId::Ctrl, Edge::Down), None);
        assert_eq!(t.push(KeyId::Trigger(Trigger::Space), Edge::Down), None);
        t.push(KeyId::Trigger(Trigger::Space), Edge::Up);
        assert_eq!(
            t.push(KeyId::Trigger(Trigger::Space), Edge::Down),
            Some(HotkeyEvent::Pressed)
        );
    }
}
