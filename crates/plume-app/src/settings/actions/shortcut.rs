use super::super::SettingsView;
use crate::prefs::DEFAULT_HOLD;
use crate::shortcut_capture::{classify_keydown, CaptureEffect, ChordText, ShortcutModifiers};
use gpui::{Context, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, Window};

impl SettingsView {
    pub fn toggle_hold_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.capture.is_listening() {
            self.capture.cancel();
            window.blur();
            cx.notify();
            return;
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
    }

    pub(in crate::settings) fn reset_hold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture.cancel();
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(DEFAULT_HOLD) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == DEFAULT_HOLD => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                self.save_prefs();
                self.retarget_hold(DEFAULT_HOLD);
                window.blur();
                cx.notify();
            }
        }
    }

    pub(in crate::settings) fn commit_hold(
        &mut self,
        chord: ChordText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.phase.prefs().hold().to_string();
        match self.phase.prefs_mut().try_set_hold(chord.as_str()) {
            Err(err) => {
                self.capture.set_reject(err.to_string());
                window.blur();
                cx.notify();
            }
            Ok(()) if previous == chord.as_str() => {
                window.blur();
                cx.notify();
            }
            Ok(()) => {
                self.save_prefs();
                self.retarget_hold(chord.as_str());
                window.blur();
                cx.notify();
            }
        }
    }

    pub(in crate::settings) fn retarget_hold(&self, hold: &str) {
        if let Some(target) = &self.hold_target {
            target.set(hold);
        }
    }
}
