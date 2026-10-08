use plume_core::HotkeyEvent;
use plume_hotkey::{BindingEvent, HotkeyAction};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Hold,
    Toggle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Ready,
    Starting(Mode),
    Recording(Mode),
    Transcribing,
    Inserting,
    Cancelling,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Start(Mode),
    Stop,
    Cancel,
}
pub(crate) struct Controller {
    pub phase: Phase,
    pressed: [bool; 2],
}
impl Controller {
    pub fn new() -> Self {
        Self {
            phase: Phase::Ready,
            pressed: [false; 2],
        }
    }
    pub fn finish(
        &mut self,
        microphone_closed: bool,
        worker_returned: bool,
        audio_cleanup: bool,
    ) -> bool {
        if microphone_closed && worker_returned && audio_cleanup {
            self.phase = Phase::Ready;
            true
        } else {
            false
        }
    }
    pub fn event(&mut self, event: BindingEvent) -> Option<Action> {
        use HotkeyAction::*;
        use HotkeyEvent::*;
        if let Some(index) = match event.action {
            Hold => Some(0),
            Toggle => Some(1),
            Cancel => None,
        } {
            let down = event.edge == Pressed;
            let fresh = down && !self.pressed[index];
            self.pressed[index] = down;
            if down && !fresh {
                return None;
            }
        }
        let action = match (self.phase, event.action, event.edge) {
            (Phase::Ready, Hold, Pressed) => Some(Action::Start(Mode::Hold)),
            (Phase::Ready, Toggle, Pressed) => Some(Action::Start(Mode::Toggle)),
            (Phase::Starting(Mode::Hold) | Phase::Recording(Mode::Hold), Hold, Released) => {
                Some(Action::Stop)
            }
            (Phase::Starting(Mode::Toggle) | Phase::Recording(Mode::Toggle), Toggle, Pressed) => {
                Some(Action::Stop)
            }
            (Phase::Starting(_) | Phase::Recording(_) | Phase::Transcribing, Cancel, Pressed) => {
                Some(Action::Cancel)
            }
            _ => None,
        };
        match action {
            Some(Action::Start(mode)) => self.phase = Phase::Starting(mode),
            Some(Action::Stop) => self.phase = Phase::Transcribing,
            Some(Action::Cancel) => self.phase = Phase::Cancelling,
            None => {}
        }
        action
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_second_capture_or_toggle_from_busy_press() {
        let mut c = Controller::new();
        let event = |action, edge| BindingEvent { action, edge };
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            Some(Action::Start(Mode::Hold))
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Released)),
            Some(Action::Stop)
        );
        assert_eq!(
            c.event(event(HotkeyAction::Toggle, HotkeyEvent::Pressed)),
            None
        );
        c.phase = Phase::Ready;
        assert_eq!(
            c.event(event(HotkeyAction::Toggle, HotkeyEvent::Released)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Toggle, HotkeyEvent::Pressed)),
            Some(Action::Start(Mode::Toggle))
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Released)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Cancel, HotkeyEvent::Pressed)),
            Some(Action::Cancel)
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            None
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Timing {
    pub silence: bool,
    pub warning: bool,
    pub limit: bool,
}
pub(crate) fn timing(
    mode: Mode,
    elapsed: std::time::Duration,
    silent_for: std::time::Duration,
) -> Timing {
    use std::time::Duration;
    Timing {
        silence: mode == Mode::Toggle && silent_for >= Duration::from_secs(30),
        warning: elapsed >= Duration::from_secs(540),
        limit: elapsed >= Duration::from_secs(600),
    }
}
#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::time::Duration;
    fn event(action: HotkeyAction, edge: HotkeyEvent) -> BindingEvent {
        BindingEvent { action, edge }
    }
    #[test]
    fn held_busy_press_requires_a_new_physical_press() {
        let mut c = Controller::new();
        c.phase = Phase::Transcribing;
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            None
        );
        assert!(c.finish(true, true, true));
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Released)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            Some(Action::Start(Mode::Hold))
        );
    }
    #[test]
    fn cancellation_is_busy_until_every_native_confirmation() {
        for phase in [
            Phase::Starting(Mode::Hold),
            Phase::Recording(Mode::Toggle),
            Phase::Transcribing,
        ] {
            let mut c = Controller::new();
            c.phase = phase;
            assert_eq!(
                c.event(event(HotkeyAction::Cancel, HotkeyEvent::Pressed)),
                Some(Action::Cancel)
            );
            for confirms in [
                (false, false, false),
                (true, false, true),
                (false, true, true),
                (true, true, false),
            ] {
                assert!(!c.finish(confirms.0, confirms.1, confirms.2));
                assert_eq!(c.phase, Phase::Cancelling);
            }
            assert!(c.finish(true, true, true));
            assert_eq!(c.phase, Phase::Ready);
        }
    }
    #[test]
    fn toggle_owns_its_stop_and_other_gestures_are_consumed() {
        let mut c = Controller::new();
        assert_eq!(
            c.event(event(HotkeyAction::Toggle, HotkeyEvent::Pressed)),
            Some(Action::Start(Mode::Toggle))
        );
        c.phase = Phase::Recording(Mode::Toggle);
        c.event(event(HotkeyAction::Toggle, HotkeyEvent::Released));
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Pressed)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Hold, HotkeyEvent::Released)),
            None
        );
        assert_eq!(
            c.event(event(HotkeyAction::Toggle, HotkeyEvent::Pressed)),
            Some(Action::Stop)
        );
    }
    #[test]
    fn monotonic_thresholds_and_speech_clear_the_reminder() {
        let t = |seconds, silence| {
            timing(
                Mode::Toggle,
                Duration::from_secs(seconds),
                Duration::from_secs(silence),
            )
        };
        assert!(!t(29, 29).silence);
        assert!(t(30, 30).silence);
        assert!(!t(31, 0).silence);
        assert!(!t(539, 0).warning);
        assert!(t(540, 0).warning);
        assert!(!t(599, 0).limit);
        assert!(t(600, 0).limit);
        assert!(!timing(Mode::Hold, Duration::from_secs(40), Duration::from_secs(40)).silence);
    }
}
