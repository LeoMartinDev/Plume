use plume_core::{BoxError, HotkeyEvent};

use crate::chord::{ChordTracker, Edge, Mods};
use crate::shortcut::{KeyId, Shortcut};
use crate::{BindingEvent, HotkeyAction, HotkeyBinding};

/// How often a backend compares its tracked keys with the OS key state.
#[allow(dead_code)]
pub(crate) const RECONCILE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// The bindings behind one keyboard hook (Windows and macOS). Each physical
/// edge goes through every tracker; the hook hides the triggers it owns.
#[derive(Default)]
pub(crate) struct BindingSet {
    entries: Vec<Entry>,
    pub(crate) cancel_active: bool,
    pub(crate) passthrough: bool,
    /// The previous physical reading and the edge count when it was taken.
    reading: Option<(u64, Physical)>,
}

struct Entry {
    action: HotkeyAction,
    tracker: ChordTracker,
    /// The trigger whose press this binding took from applications. Its
    /// repeats and its release are hidden too.
    swallowed: Option<KeyId>,
}

/// The keyboard as the OS reports it, read outside the event stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Physical {
    pub mods: Mods,
    /// Modifiers whose reading reflects the physical key.
    pub trusted: Mods,
    /// Engaged triggers that are not held.
    pub released: Vec<KeyId>,
}

impl BindingSet {
    pub(crate) fn register(&mut self, bindings: &[HotkeyBinding]) -> Result<(), BoxError> {
        let mut entries = Vec::new();
        for b in bindings {
            entries.push(Entry {
                action: b.action,
                tracker: ChordTracker::new(Shortcut::parse(&b.shortcut)?),
                swallowed: None,
            });
        }
        self.entries = entries;
        self.reading = None;
        Ok(())
    }

    fn enabled(&self, action: HotkeyAction) -> bool {
        action != HotkeyAction::Cancel || self.cancel_active
    }

    /// One physical edge. `mods` is the modifier snapshot carried by the
    /// event where the platform provides one (macOS flags).
    pub(crate) fn push(
        &mut self,
        mods: Option<Mods>,
        id: KeyId,
        edge: Edge,
    ) -> (Vec<BindingEvent>, bool) {
        // Esc pressed while the dictation chord is still down carries that
        // chord's modifiers. They must not turn the cancel into another
        // shortcut (Ctrl+Shift+Esc opens Task Manager on Windows).
        let held_chords = self
            .entries
            .iter()
            .filter(|e| e.action != HotkeyAction::Cancel && e.tracker.engaged())
            .fold(Mods::default(), |all, e| all.union(e.tracker.chord_mods()));
        let mut events = Vec::new();
        let mut swallow = false;
        for i in 0..self.entries.len() {
            let enabled = self.enabled(self.entries[i].action);
            let passthrough = self.passthrough;
            let entry = &mut self.entries[i];
            if entry.action == HotkeyAction::Cancel {
                entry.tracker.tolerate(held_chords);
            }
            let signal = match mods {
                Some(mods) => entry.tracker.push_macos(mods, id, edge),
                None => entry.tracker.push(id, edge),
            };
            // Modifier edges always reach applications; dropping one leaves
            // their keyboard state out of sync with the physical keys.
            let trigger = matches!(id, KeyId::Trigger(_)) && entry.tracker.owns_trigger(id);
            if enabled && trigger && signal == Some(HotkeyEvent::Pressed) && !passthrough {
                entry.swallowed = Some(id);
            }
            swallow |= entry.swallowed == Some(id);
            if passthrough || trigger && edge == Edge::Up {
                entry.swallowed = None;
            }
            if let Some(edge) = signal.filter(|_| enabled) {
                events.push(BindingEvent {
                    action: entry.action,
                    edge,
                });
            }
        }
        (events, swallow && !self.passthrough)
    }

    /// Non-modifier triggers of the chords currently held.
    pub(crate) fn engaged_triggers(&self) -> Vec<KeyId> {
        self.entries
            .iter()
            .filter(|e| e.tracker.engaged())
            .map(|e| e.tracker.trigger_key())
            .filter(|id| matches!(id, KeyId::Trigger(_)))
            .collect()
    }

    /// Apply a reading of the physical keyboard. A reading only overrides
    /// the tracked state once it is stable across two polls with no edge in
    /// between (`edges` counts every hook callback), so it never races an
    /// edge that is still being delivered.
    pub(crate) fn reconcile(&mut self, edges: u64, physical: Physical) -> Vec<BindingEvent> {
        let stable = self.reading.as_ref() == Some(&(edges, physical.clone()));
        self.reading = Some((edges, physical.clone()));
        if !stable {
            return Vec::new();
        }
        let mut released = Vec::new();
        for entry in &mut self.entries {
            let mut signal = entry.tracker.sync_mods(physical.mods, physical.trusted);
            if entry.tracker.engaged() && physical.released.contains(&entry.tracker.trigger_key()) {
                signal = entry.tracker.release();
            }
            if signal.is_some() {
                entry.swallowed = None;
            }
            released.extend(signal.map(|edge| (entry.action, edge)));
        }
        self.events(released)
    }

    /// The hook stopped observing the keyboard: every key it believes held
    /// may have been released unseen. Held chords end now.
    pub(crate) fn forget(&mut self) -> Vec<BindingEvent> {
        let mut released = Vec::new();
        for entry in &mut self.entries {
            entry.swallowed = None;
            released.extend(entry.tracker.forget().map(|edge| (entry.action, edge)));
        }
        self.reading = None;
        self.events(released)
    }

    fn events(&self, signals: Vec<(HotkeyAction, HotkeyEvent)>) -> Vec<BindingEvent> {
        signals
            .into_iter()
            .filter(|(action, _)| self.enabled(*action))
            .map(|(action, edge)| BindingEvent { action, edge })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::Trigger;
    use HotkeyAction::{Cancel, Hold, Toggle};
    use HotkeyEvent::{Pressed, Released};

    const CTRL: KeyId = KeyId::Ctrl;
    const SHIFT: KeyId = KeyId::Shift;
    const ESC: KeyId = KeyId::Trigger(Trigger::Escape);
    const I: KeyId = KeyId::Trigger(Trigger::Char('i'));
    const SPACE: KeyId = KeyId::Trigger(Trigger::Space);

    fn set(bindings: &[(HotkeyAction, &str)]) -> BindingSet {
        let mut set = BindingSet::default();
        let bindings: Vec<_> = bindings
            .iter()
            .map(|(action, shortcut)| HotkeyBinding {
                action: *action,
                shortcut: (*shortcut).into(),
            })
            .collect();
        set.register(&bindings).unwrap();
        set
    }
    fn ev(action: HotkeyAction, edge: HotkeyEvent) -> BindingEvent {
        BindingEvent { action, edge }
    }
    fn press(set: &mut BindingSet, keys: &[KeyId]) -> (Vec<BindingEvent>, bool) {
        let mut last = (Vec::new(), false);
        for key in keys {
            last = set.push(None, *key, Edge::Down);
        }
        last
    }
    fn trusted() -> Mods {
        Mods {
            ctrl: true,
            alt: true,
            shift: true,
            super_key: true,
            fn_key: false,
        }
    }

    #[test]
    fn esc_cancels_while_the_hold_chord_is_down_and_never_reaches_the_os() {
        let mut set = set(&[(Hold, "Ctrl+Shift+i"), (Cancel, "Esc")]);
        assert_eq!(
            press(&mut set, &[CTRL, SHIFT, I]),
            (vec![ev(Hold, Pressed)], true)
        );
        set.cancel_active = true;
        assert_eq!(press(&mut set, &[ESC]), (vec![ev(Cancel, Pressed)], true));
        assert_eq!(
            set.push(None, ESC, Edge::Up),
            (vec![ev(Cancel, Released)], true)
        );
        assert_eq!(
            set.push(None, I, Edge::Up),
            (vec![ev(Hold, Released)], true)
        );
    }

    #[test]
    fn ctrl_shift_esc_still_reaches_the_os_outside_a_held_chord() {
        let mut set = set(&[(Toggle, "Ctrl+Space"), (Cancel, "Esc")]);
        set.cancel_active = true;
        assert_eq!(press(&mut set, &[CTRL, SHIFT, ESC]), (Vec::new(), false));
        // Toggle recording: its chord is not held between the two presses.
        set.push(None, ESC, Edge::Up);
        set.push(None, SHIFT, Edge::Up);
        assert_eq!(press(&mut set, &[SPACE]), (vec![ev(Toggle, Pressed)], true));
        set.push(None, SPACE, Edge::Up);
        assert_eq!(press(&mut set, &[ESC]), (Vec::new(), false));
        set.push(None, CTRL, Edge::Up);
        set.push(None, ESC, Edge::Up);
        assert_eq!(press(&mut set, &[ESC]), (vec![ev(Cancel, Pressed)], true));
    }

    #[test]
    fn an_inactive_cancel_is_neither_reported_nor_swallowed() {
        let mut set = set(&[(Hold, "Ctrl+Space"), (Cancel, "Esc")]);
        press(&mut set, &[CTRL, SPACE]);
        assert_eq!(press(&mut set, &[ESC]), (Vec::new(), false));
    }

    #[test]
    fn a_toggle_on_the_cancel_key_stops_rather_than_cancels() {
        let mut set = set(&[(Cancel, "Esc"), (Toggle, "Ctrl+Esc")]);
        set.cancel_active = true;
        assert_eq!(
            press(&mut set, &[CTRL, ESC]),
            (vec![ev(Toggle, Pressed)], true)
        );
    }

    #[test]
    fn missed_modifier_releases_are_repaired_by_a_stable_reading() {
        let mut set = set(&[(Hold, "Ctrl+Shift+i"), (Cancel, "Esc")]);
        set.cancel_active = true;
        // Ctrl and Shift go down; their releases are never observed.
        press(&mut set, &[CTRL, SHIFT]);
        let idle = Physical {
            trusted: trusted(),
            ..Physical::default()
        };
        assert!(set.reconcile(2, idle.clone()).is_empty());
        assert_eq!(press(&mut set, &[ESC]), (Vec::new(), false));
        set.push(None, ESC, Edge::Up);
        // A single reading, or one interrupted by an edge, is not enough.
        assert!(set.reconcile(4, idle.clone()).is_empty());
        assert!(set.reconcile(5, idle.clone()).is_empty());
        assert!(set.reconcile(5, idle).is_empty());
        assert_eq!(press(&mut set, &[ESC]), (vec![ev(Cancel, Pressed)], true));
    }

    #[test]
    fn a_physically_released_trigger_ends_the_hold() {
        let mut set = set(&[(Hold, "Ctrl+Space")]);
        press(&mut set, &[CTRL, SPACE]);
        assert_eq!(set.engaged_triggers(), [SPACE]);
        let held = Physical {
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
            trusted: trusted(),
            released: Vec::new(),
        };
        assert!(set.reconcile(2, held.clone()).is_empty());
        assert!(set.reconcile(2, held.clone()).is_empty());
        let released = Physical {
            released: vec![SPACE],
            ..held
        };
        assert!(set.reconcile(2, released.clone()).is_empty());
        assert_eq!(set.reconcile(2, released), [ev(Hold, Released)]);
        assert!(set.engaged_triggers().is_empty());
        // The late release is no longer hidden, and the next press is fresh.
        assert_eq!(set.push(None, SPACE, Edge::Up), (Vec::new(), false));
        assert_eq!(press(&mut set, &[SPACE]), (vec![ev(Hold, Pressed)], true));
    }

    #[test]
    fn a_blind_hook_ends_the_hold_and_stops_hiding_its_trigger() {
        let mut set = set(&[(Hold, "Ctrl+Shift+i"), (Cancel, "Esc")]);
        press(&mut set, &[CTRL, SHIFT, I]);
        assert_eq!(set.forget(), [ev(Hold, Released)]);
        assert!(set.forget().is_empty());
        assert_eq!(set.push(None, I, Edge::Down), (Vec::new(), false));
        set.push(None, I, Edge::Up);
        set.cancel_active = true;
        assert_eq!(press(&mut set, &[ESC]), (vec![ev(Cancel, Pressed)], true));
    }

    #[test]
    fn shortcut_editing_reports_nothing_hidden() {
        let mut set = set(&[(Hold, "Ctrl+Space")]);
        set.passthrough = true;
        assert_eq!(press(&mut set, &[CTRL, SPACE]).1, false);
        set.passthrough = false;
        assert_eq!(set.push(None, SPACE, Edge::Down), (Vec::new(), false));
        assert_eq!(set.push(None, SPACE, Edge::Up).1, false);
        assert_eq!(press(&mut set, &[SPACE]), (vec![ev(Hold, Pressed)], true));
    }
}
