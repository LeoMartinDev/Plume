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

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ChordTracker {
    wanted: Shortcut,
    down: Down,
    engaged: bool,
}

#[allow(dead_code)]
impl ChordTracker {
    pub(crate) fn new(wanted: Shortcut) -> Self {
        ChordTracker {
            wanted,
            down: Down::default(),
            engaged: false,
        }
    }

    pub(crate) fn push(&mut self, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        apply_id(&mut self.down, self.wanted.trigger, id, edge);
        self.emit(id, edge)
    }

    pub(crate) fn push_macos(&mut self, mods: Mods, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        self.down.mods = mods;
        apply_id(&mut self.down, self.wanted.trigger, id, edge);
        // Flags describe every modifier, including one pressed before this
        // tracker was registered or whose flags-changed event was missed.
        let trigger_down = match self.wanted.trigger {
            Trigger::Ctrl => Some(mods.ctrl),
            Trigger::Alt => Some(mods.alt),
            Trigger::Shift => Some(mods.shift),
            Trigger::Super => Some(mods.super_key),
            Trigger::Fn => Some(mods.fn_key),
            _ => None,
        };
        if let Some(on) = trigger_down {
            self.down.trigger = on.then_some(self.wanted.trigger);
            if self.engaged && !on {
                self.engaged = false;
                return Some(HotkeyEvent::Released);
            }
        }
        self.emit(id, edge)
    }

    /// A capture begins only when the whole chord matches.  Once it has
    /// begun, however, its lifetime belongs to the trigger key.  Modifier
    /// state can be reported out of order by platform hooks (and may change
    /// while a chord is held); treating that as a release made a bubble flash
    /// on screen and stopped recording immediately.
    fn emit(&mut self, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        if self.engaged {
            if matches!(edge, Edge::Up) && key_is_trigger(id, self.wanted.trigger) {
                self.engaged = false;
                return Some(HotkeyEvent::Released);
            }
            return None;
        }
        if matches_shortcut(self.wanted, self.down) {
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

#[allow(dead_code)]
fn matches_shortcut(wanted: Shortcut, down: Down) -> bool {
    wanted.ctrl == down.mods.ctrl
        && wanted.alt == down.mods.alt
        && wanted.shift == down.mods.shift
        && wanted.super_key == down.mods.super_key
        && wanted.fn_key == down.mods.fn_key
        && down.trigger == Some(wanted.trigger)
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
