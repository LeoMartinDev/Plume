use stt_core::HotkeyEvent;

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
pub(crate) struct RepeatFilter {
    held: bool,
}

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
        emit(&mut self.engaged, matches_shortcut(self.wanted, self.down))
    }

    pub(crate) fn push_macos(&mut self, mods: Mods, id: KeyId, edge: Edge) -> Option<HotkeyEvent> {
        self.down.mods = mods;
        apply_id(&mut self.down, self.wanted.trigger, id, edge);
        emit(&mut self.engaged, matches_shortcut(self.wanted, self.down))
    }
}

#[allow(dead_code)]
fn apply_id(down: &mut Down, wanted_trigger: Trigger, id: KeyId, edge: Edge) {
    let on = matches!(edge, Edge::Down);
    match id {
        KeyId::Ctrl => down.mods.ctrl = on,
        KeyId::Alt => down.mods.alt = on,
        KeyId::Shift => down.mods.shift = on,
        KeyId::Super => down.mods.super_key = on,
        KeyId::Fn => {
            down.mods.fn_key = on;
            if wanted_trigger == Trigger::Fn {
                down.trigger = if on { Some(Trigger::Fn) } else { None };
            }
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

#[allow(dead_code)]
fn matches_shortcut(wanted: Shortcut, down: Down) -> bool {
    let fn_ok = if wanted.trigger == Trigger::Fn {
        true
    } else {
        wanted.fn_key == down.mods.fn_key
    };
    wanted.ctrl == down.mods.ctrl
        && wanted.alt == down.mods.alt
        && wanted.shift == down.mods.shift
        && wanted.super_key == down.mods.super_key
        && fn_ok
        && down.trigger == Some(wanted.trigger)
}

#[allow(dead_code)]
fn emit(engaged: &mut bool, now: bool) -> Option<HotkeyEvent> {
    match (*engaged, now) {
        (false, true) => {
            *engaged = true;
            Some(HotkeyEvent::Pressed)
        }
        (true, false) => {
            *engaged = false;
            Some(HotkeyEvent::Released)
        }
        _ => None,
    }
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
