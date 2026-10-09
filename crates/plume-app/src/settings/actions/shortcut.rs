use super::super::SettingsView;
use crate::prefs::DEFAULT_HOLD;
use crate::shortcut_capture::{classify_keydown, CaptureEffect, ChordText, ShortcutModifiers};
use gpui::{Context, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, Window};

impl SettingsView {
    pub fn toggle_hold_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.capture.is_listening() {
            self.capture.cancel();
            self.shortcut_edit.take();
            window.blur();
            cx.notify();
            return;
        }
        if let Some(control) = &self.session_control {
            match control.reserve_shortcut_edit() {
                Ok(guard) => self.shortcut_edit = Some(guard),
                Err(error) => {
                    self.capture.set_reject(error);
                    cx.notify();
                    return;
                }
            }
        }
        self.capture.begin();
        window.focus(&self.hold_focus);
        cx.notify();
    }

    pub fn on_hold_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let stroke = classify_keydown(
            event.is_held,
            event.keystroke.key.as_str(),
            ShortcutModifiers::from_gpui(event.keystroke.modifiers),
        );
        let effect = self.capture.apply(stroke);
        self.handle_hold_capture(effect, window, cx);
    }

    pub fn on_hold_modifiers(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let effect = self
            .capture
            .apply_modifiers(ShortcutModifiers::from_gpui(event.modifiers));
        self.handle_hold_capture(effect, window, cx);
    }

    pub fn on_hold_key_up(
        &mut self,
        event: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let effect = self.capture.apply_key_up(event.keystroke.key.as_str());
        self.handle_hold_capture(effect, window, cx);
    }

    pub(in crate::settings) fn handle_hold_capture(
        &mut self,
        effect: CaptureEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            CaptureEffect::None => {}
            CaptureEffect::StayListening => cx.notify(),
            CaptureEffect::Cancelled | CaptureEffect::Rejected(_) => {
                window.blur();
                cx.notify();
            }
            CaptureEffect::Offer(chord) => self.commit_hold(chord, window, cx),
        }
        if !self.capture.is_listening() {
            self.shortcut_edit.take();
        }
    }

    pub(in crate::settings) fn reset_hold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture.cancel();
        let shortcut = if self.capture_toggle {
            "Ctrl+Shift+Space"
        } else {
            DEFAULT_HOLD
        };
        self.apply_shortcut(shortcut, window, cx);
        self.shortcut_edit.take();
    }
    pub(in crate::settings) fn commit_hold(
        &mut self,
        chord: ChordText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_shortcut(chord.as_str(), window, cx);
    }
    fn apply_shortcut(&mut self, shortcut: &str, window: &mut Window, cx: &mut Context<Self>) {
        let result = if self.capture_toggle {
            self.phase.prefs_mut().try_set_toggle(Some(shortcut))
        } else {
            self.phase.prefs_mut().try_set_hold(shortcut)
        };
        match result {
            Err(err) => self.capture.set_reject(err.to_string()),
            Ok(()) => {
                self.save_prefs();
                if self.capture_toggle {
                    if let Some(control) = &self.session_control {
                        control.set_toggle(Some(shortcut.to_string()));
                    }
                } else {
                    self.retarget_hold(shortcut);
                }
            }
        }
        window.blur();
        cx.notify();
    }

    pub(in crate::settings) fn retarget_hold(&self, hold: &str) {
        if let Some(target) = &self.hold_target {
            target.set(hold);
        }
    }
}
