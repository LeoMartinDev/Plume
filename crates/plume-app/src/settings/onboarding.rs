use gpui::{
    div, prelude::*, px, relative, svg, AnyElement, Context, FontWeight, SharedString, Window,
};
use plume_engine::Engine;
use plume_overlay::Feedback;
use plume_session::{PreviewEvent, SessionMode};
use plume_ui::Tokens;
use std::time::Duration;

use super::SettingsView;
use crate::catalog::ModelId;
use crate::permissions::{self, Permission, Permissions};
use crate::phase::{AppPhase, OnboardStatus};
use crate::prefs::{OnboardingStep, Prefs, ONBOARDING_VERSION};

pub(super) const CHOICES: [ModelId; 3] = [
    ModelId::WhisperBase,
    ModelId::Nemotron35Compact,
    ModelId::WhisperLargeV3Turbo,
];

pub(super) struct OnboardingState {
    pub engine: Option<Engine>,
    pub model_ready: bool,
    permissions: Permissions,
    microphone_ready: bool,
    microphone_checking: bool,
    pub session_starting: bool,
    pub session_error: Option<String>,
    feedback: Feedback,
    transcript: String,
    finished: bool,
    visible: bool,
}

impl OnboardingState {
    pub fn new(_prefs: &Prefs) -> Self {
        Self {
            engine: None,
            model_ready: false,
            permissions: permissions::snapshot(),
            microphone_ready: false,
            microphone_checking: false,
            session_starting: false,
            session_error: None,
            feedback: Feedback::Empty,
            transcript: String::new(),
            finished: false,
            visible: true,
        }
    }
}

fn model_busy(phase: &AppPhase) -> bool {
    matches!(
        phase,
        AppPhase::Onboarding {
            status: OnboardStatus::Fetching { .. } | OnboardStatus::Activating,
            ..
        }
    )
}

impl SettingsView {
    pub(super) fn onboarding_model_loaded(
        &mut self,
        engine: Engine,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(setup) = &mut self.onboarding else {
            return false;
        };
        setup.engine = Some(engine.clone());
        setup.model_ready = true;
        setup.session_starting = false;
        setup.session_error = None;
        self.downloads.clear();
        if let Some(target) = &self.engine_target {
            target.set(engine);
            self.active_model = Some(self.phase.prefs().model);
        }
        self.phase = AppPhase::Onboarding {
            prefs: self.phase.prefs().clone(),
            status: OnboardStatus::Idle,
            warning: None,
        };
        cx.notify();
        true
    }

    pub(super) fn onboarding_model_failed(&mut self) {
        if let Some(setup) = &mut self.onboarding {
            setup.model_ready = false;
            setup.engine = None;
            self.phase.prefs_mut().onboarding_step = OnboardingStep::Model;
            self.save_prefs();
        }
    }

    fn select_model(&mut self, id: ModelId, cx: &mut Context<Self>) {
        if model_busy(&self.phase) || self.is_busy() {
            return;
        }
        if self.phase.prefs().model == id {
            return;
        }
        if let Some(setup) = &mut self.onboarding {
            setup.engine = None;
            setup.model_ready = false;
        }
        self.active_model = None;
        self.phase.prefs_mut().model = id;
        self.phase = AppPhase::Onboarding {
            prefs: self.phase.prefs().clone(),
            status: OnboardStatus::Idle,
            warning: None,
        };
        self.save_prefs();
        cx.notify();
    }

    fn download_model(&mut self, cx: &mut Context<Self>) {
        if model_busy(&self.phase) || self.is_busy() || self.settings_preview {
            return;
        }
        self.save_prefs();
        if self.save_error.is_some() {
            cx.notify();
            return;
        }
        let prefs = self.phase.prefs().clone();
        let request = self.begin_download(prefs.clone(), cx);
        cx.defer(move |cx| crate::begin_pack(cx, prefs, request));
    }

    fn is_busy(&self) -> bool {
        self.session_control
            .as_ref()
            .is_some_and(|control| control.is_busy())
    }

    fn can_finish(&self) -> bool {
        self.onboarding.as_ref().is_some_and(|setup| {
            setup.model_ready
                && setup.permissions.granted()
                && setup.microphone_ready
                && !setup.finished
                && self.session_control.is_some()
                && setup.session_error.is_none()
        }) && !self.is_busy()
            && self.save_error.is_none()
    }

    fn change_step(&mut self, step: OnboardingStep, cx: &mut Context<Self>) {
        if self.is_busy() || model_busy(&self.phase) {
            return;
        }
        self.reset_for_phase();
        let previous_mode = self.session_control.as_ref().map(|control| control.mode());
        if let Some(control) = &self.session_control {
            if step == OnboardingStep::Permissions {
                let prefs = self.phase.prefs();
                let result = plume_session::Config::from_prefs(
                    prefs.hold(),
                    prefs.cancel(),
                    prefs.model.data_dir(),
                )
                .and_then(|config| config.with_toggle(prefs.toggle()))
                .map_err(|error| error.to_string())
                .and_then(|config| control.configure_shortcuts(config));
                if let Err(error) = result {
                    self.onboarding.as_mut().unwrap().session_error = Some(error);
                    cx.notify();
                    return;
                }
                self.onboarding.as_mut().unwrap().session_error = None;
            }
            let mode = if step == OnboardingStep::Permissions {
                SessionMode::Preview
            } else {
                SessionMode::Suspended
            };
            if let Err(error) = control.set_mode(mode) {
                self.onboarding.as_mut().unwrap().session_error = Some(error);
                cx.notify();
                return;
            }
        }
        let previous = self.phase.prefs().onboarding_step;
        self.phase.prefs_mut().onboarding_step = step;
        self.save_prefs();
        if self.save_error.is_some() {
            self.phase.prefs_mut().onboarding_step = previous;
            if let (Some(control), Some(mode)) = (&self.session_control, previous_mode) {
                if let Err(error) = control.set_mode(mode) {
                    self.onboarding.as_mut().unwrap().session_error = Some(error);
                }
            }
        }
        cx.notify();
    }

    fn finish_setup(&mut self, cx: &mut Context<Self>) {
        if !self.can_finish() {
            return;
        }
        let mut completed = self.phase.prefs().clone();
        completed.onboarding_version = ONBOARDING_VERSION;
        let result =
            self.session_control
                .as_ref()
                .unwrap()
                .set_mode_after(SessionMode::System, || {
                    crate::prefs::save_at(&self.prefs_path, &completed)
                        .map_err(|error| error.to_string())
                });
        if let Err(error) = result {
            self.save_prefs();
            self.onboarding.as_mut().unwrap().session_error = Some(error);
            cx.notify();
            return;
        }
        *self.phase.prefs_mut() = completed;
        self.phase = AppPhase::Live {
            prefs: self.phase.prefs().clone(),
        };
        self.onboarding.as_mut().unwrap().finished = true;
        cx.notify();
        cx.defer(super::window::finish_onboarding_window);
    }

    pub(super) fn onboarding_preview_event(&mut self, event: PreviewEvent, cx: &mut Context<Self>) {
        if let Some(setup) = &mut self.onboarding {
            match event {
                PreviewEvent::Transcript(text) => setup.transcript = text,
                PreviewEvent::Feedback(feedback) => {
                    if feedback == Feedback::Starting {
                        setup.transcript.clear();
                    }
                    setup.feedback = feedback;
                }
            }
            cx.notify();
        }
    }

    pub(super) fn onboarding_visibility(&mut self, visible: bool, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.onboarding else {
            return;
        };
        setup.visible = visible;
        if !setup.finished && !self.is_busy() {
            if let Some(control) = &self.session_control {
                let mode = if visible
                    && self.phase.prefs().onboarding_step == OnboardingStep::Permissions
                {
                    SessionMode::Preview
                } else {
                    SessionMode::Suspended
                };
                let _ = control.set_mode(mode);
            }
        }
        cx.notify();
    }

    pub(super) fn poll_onboarding(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let keep_polling = this
                .update(cx, |view, cx| view.refresh_onboarding(cx))
                .unwrap_or(false);
            if !keep_polling {
                break;
            }
        })
        .detach();
    }

    fn refresh_onboarding(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(setup) = &mut self.onboarding else {
            return false;
        };
        if setup.finished {
            return false;
        }
        setup.permissions = permissions::snapshot();
        if !setup.permissions.granted() {
            setup.microphone_ready = false;
        }
        let visible = setup.visible;
        if !self.is_busy() {
            if let Some(control) = &self.session_control {
                let setup = self.onboarding.as_ref().unwrap();
                let mode = if visible
                    && setup.permissions.granted()
                    && self.phase.prefs().onboarding_step == OnboardingStep::Permissions
                {
                    SessionMode::Preview
                } else {
                    SessionMode::Suspended
                };
                if control.mode() != mode {
                    let _ = control.set_mode(mode);
                }
            }
        }
        let setup = self.onboarding.as_mut().unwrap();
        if !self.settings_preview
            && visible
            && self.phase.prefs().onboarding_step == OnboardingStep::Permissions
            && setup.model_ready
            && setup.permissions.granted()
        {
            if !setup.microphone_ready
                && !setup.microphone_checking
                && setup.session_error.is_none()
            {
                setup.microphone_checking = true;
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async {
                            let microphone =
                                plume_audio::Mic::open().map_err(|error| error.to_string())?;
                            drop(microphone);
                            Ok::<_, String>(())
                        })
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        if let Some(setup) = &mut view.onboarding {
                            setup.microphone_checking = false;
                            match result {
                                Ok(()) => {
                                    setup.microphone_ready = true;
                                }
                                Err(error) => {
                                    setup.session_error =
                                        Some(format!("Microphone unavailable: {error}"))
                                }
                            }
                            cx.notify();
                        }
                    });
                })
                .detach();
            } else if setup.microphone_ready
                && !setup.session_starting
                && setup.session_error.is_none()
                && self.session_control.is_none()
            {
                if let Some(engine) = setup.engine.clone() {
                    setup.session_starting = true;
                    let prefs = self.phase.prefs().clone();
                    cx.defer(move |cx| {
                        crate::start_session(cx, prefs, engine, SessionMode::Preview)
                    });
                }
            }
        }
        cx.notify();
        true
    }
}

pub(super) fn render(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let setup = view.onboarding.as_ref().unwrap();
    let step = view.phase.prefs().onboarding_step;
    let mut shell = div()
        .id("onboarding-shell")
        .tab_group()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(tokens.chrome)
        .text_color(tokens.text)
        .when(!view.capture.is_listening(), |el| {
            el.track_focus(&view.hold_focus)
        })
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            if !view.capture.is_listening() && event.keystroke.key == "tab" {
                cx.stop_propagation();
                window.prevent_default();
                if event.keystroke.modifiers.shift {
                    window.focus_prev();
                } else {
                    window.focus_next();
                }
            }
        }))
        .when(view.settings_preview, |el| {
            el.on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if !view.capture.is_listening() && event.keystroke.modifiers.control {
                    if event.keystroke.key == "4" {
                        cx.stop_propagation();
                        window.prevent_default();
                        cx.defer(super::window::finish_onboarding_window);
                        return;
                    }
                    let step = match event.keystroke.key.as_str() {
                        "1" => Some(OnboardingStep::Model),
                        "2" => Some(OnboardingStep::Shortcuts),
                        "3" => Some(OnboardingStep::Permissions),
                        _ => None,
                    };
                    if let Some(step) = step {
                        cx.stop_propagation();
                        window.prevent_default();
                        view.phase.prefs_mut().onboarding_step = step;
                        cx.notify();
                    }
                }
            }))
        });
    #[cfg(target_os = "macos")]
    {
        shell = shell.child(super::view::macos_titlebar(tokens));
    }
    let mut content = div()
        .id("onboarding-panel")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(12.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.content);
    let body = match step {
        OnboardingStep::Model => model_screen(view, tokens, cx),
        OnboardingStep::Shortcuts => shortcuts_screen(view, tokens, cx),
        OnboardingStep::Permissions => permissions_screen(view, tokens, cx),
    };
    let busy = model_busy(&view.phase);
    let next_enabled = match step {
        OnboardingStep::Model => setup.model_ready || (!busy && !view.settings_preview),
        OnboardingStep::Shortcuts => {
            !view.capture.is_listening() && view.capture.reject().is_none()
        }
        OnboardingStep::Permissions => view.can_finish(),
    } && !view.is_busy()
        && view.save_error.is_none();
    let next_label = match step {
        OnboardingStep::Model if busy => "Preparing…",
        OnboardingStep::Model if !setup.model_ready => {
            if crate::catalog::is_complete(
                view.phase.prefs().model,
                &view.phase.prefs().model.data_dir(),
            ) {
                "Load model"
            } else {
                "Download"
            }
        }
        OnboardingStep::Permissions => "Finish",
        _ => "Next",
    };
    content = content
        .child(
            div()
                .id("onboarding-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .px(px(28.))
                .py(px(28.))
                .child(
                    div()
                        .flex_1()
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .child(body),
                )
                .children(
                    view.capture
                        .reject()
                        .map(|error| super::view::error_text(tokens, error.to_string())),
                )
                .children(view.save_error.clone().map(|error| {
                    div()
                        .mt(px(12.))
                        .child(super::view::error_text(tokens, error))
                        .child(button(
                            tokens,
                            "onboarding-save-retry",
                            "Retry saving",
                            true,
                            false,
                            cx,
                            |view, _, cx| {
                                view.save_prefs();
                                cx.notify();
                            },
                        ))
                })),
        )
        .child(
            div()
                .h(px(58.))
                .flex_shrink_0()
                .px(px(28.))
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .children((step != OnboardingStep::Model).then(|| {
                            button(
                                tokens,
                                "onboarding-back",
                                "Back",
                                !view.is_busy() && !setup.session_starting,
                                false,
                                cx,
                                move |view, _, cx| {
                                    view.change_step(
                                        if step == OnboardingStep::Permissions {
                                            OnboardingStep::Shortcuts
                                        } else {
                                            OnboardingStep::Model
                                        },
                                        cx,
                                    )
                                },
                            )
                        })),
                )
                .child(progress_indicator(step, tokens))
                .child(div().flex_1().flex().justify_end().child(button(
                    tokens,
                    "onboarding-next",
                    next_label,
                    next_enabled,
                    true,
                    cx,
                    move |view, _, cx| match step {
                        OnboardingStep::Model if !view.onboarding.as_ref().unwrap().model_ready => {
                            view.download_model(cx)
                        }
                        OnboardingStep::Model => view.change_step(OnboardingStep::Shortcuts, cx),
                        OnboardingStep::Shortcuts => {
                            view.change_step(OnboardingStep::Permissions, cx)
                        }
                        OnboardingStep::Permissions => view.finish_setup(cx),
                    },
                ))),
        );
    shell
        .child(div().flex_1().min_h_0().p(px(8.)).child(content))
        .into_any_element()
}

fn progress_indicator(step: OnboardingStep, tokens: &Tokens) -> AnyElement {
    div()
        .id("onboarding-progress")
        .flex()
        .items_center()
        .gap(px(6.))
        .children(
            [
                OnboardingStep::Model,
                OnboardingStep::Shortcuts,
                OnboardingStep::Permissions,
            ]
            .into_iter()
            .map(|item| {
                let active = item == step;
                div()
                    .h(px(6.))
                    .w(px(if active { 18. } else { 6. }))
                    .rounded_full()
                    .bg(if active {
                        tokens.accent
                    } else {
                        tokens.hairline
                    })
            }),
        )
        .into_any_element()
}

fn model_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let selected = view.phase.prefs().model;
    let busy = model_busy(&view.phase);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .flex()
                .gap(px(12.))
                .children(CHOICES.into_iter().enumerate().map(|(index, id)| {
                    let entry = id.entry();
                    let chosen = selected == id;
                    let nvidia = id == ModelId::Nemotron35Compact;
                    div()
                        .id(SharedString::from(format!(
                            "onboarding-model-{}",
                            id.as_str()
                        )))
                        .flex_1()
                        .min_w_0()
                        .h(px(292.))
                        .p(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(18.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(if chosen {
                            tokens.accent
                        } else {
                            tokens.hairline
                        })
                        .bg(if chosen {
                            tokens.accent_soft
                        } else {
                            tokens.group
                        })
                        .when(!busy, |el| {
                            el.cursor_pointer()
                                .tab_index(0)
                                .hover(move |style| {
                                    style.border_color(if chosen {
                                        tokens.accent
                                    } else {
                                        tokens.muted
                                    })
                                })
                                .on_click(
                                    cx.listener(move |view, _, _, cx| view.select_model(id, cx)),
                                )
                                .on_key_down(cx.listener(
                                    move |view, event: &gpui::KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                            && !event.is_held
                                        {
                                            cx.stop_propagation();
                                            window.prevent_default();
                                            view.select_model(id, cx);
                                        }
                                    },
                                ))
                        })
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(["Small", "Medium", "Large"][index]),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            svg()
                                                .path(if nvidia {
                                                    "labs/nvidia.svg"
                                                } else {
                                                    "labs/openai.svg"
                                                })
                                                .size(px(24.))
                                                .text_color(tokens.text),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(tokens.muted)
                                                .child(if nvidia { "NVIDIA" } else { "OpenAI" }),
                                        ),
                                )
                                .child(
                                    div()
                                        .h(px(40.))
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(entry.name),
                                )
                                .child(div().text_xs().text_color(tokens.muted).child(entry.size)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(super::model::spec_meter("Speed", entry.speed, tokens))
                                .child(super::model::spec_meter(
                                    "Accuracy",
                                    entry.accuracy,
                                    tokens,
                                )),
                        )
                        .child(
                            div()
                                .h(px(16.))
                                .text_xs()
                                .text_color(tokens.muted)
                                .child(if nvidia { "Recommended" } else { "" }),
                        )
                })),
        );
    if !CHOICES.contains(&selected) {
        body = body.child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child(format!("Current model: {}", selected.entry().name)),
        );
    }
    if let AppPhase::Onboarding {
        status: OnboardStatus::Fetching { last },
        ..
    } = &view.phase
    {
        let fraction = last
            .total
            .filter(|total| *total > 0)
            .map(|total| (last.bytes as f32 / total as f32).clamp(0., 1.))
            .unwrap_or(0.);
        let status =
            super::model::download_status(last).unwrap_or_else(|| "Preparing download…".into());
        body = body
            .child(div().text_xs().text_color(tokens.muted).child(format!(
                "Downloading · {status} · {:.1} MB received",
                last.bytes as f64 / 1_048_576.
            )))
            .child(
                div().h(px(3.)).rounded(px(2.)).bg(tokens.fill).child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .bg(tokens.accent)
                        .rounded(px(2.)),
                ),
            );
    } else if busy {
        body = body.child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Loading model…"),
        );
    }
    if let AppPhase::Onboarding {
        status: OnboardStatus::Failed { reason },
        ..
    } = &view.phase
    {
        body = body.child(super::view::error_text(tokens, reason.clone()));
    }
    if let AppPhase::Onboarding {
        warning: Some(warning),
        ..
    } = &view.phase
    {
        body = body.child(super::view::error_text(tokens, warning.clone()));
    }
    body.into_any_element()
}

fn shortcuts_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .flex()
                .gap(px(16.))
                .child(shortcut_card(view, tokens, false, cx))
                .child(shortcut_card(view, tokens, true, cx)),
        )
        .children(
            view.onboarding
                .as_ref()
                .and_then(|setup| setup.session_error.clone())
                .map(|error| super::view::error_text(tokens, error)),
        )
        .into_any_element()
}

fn shortcut_card(
    view: &SettingsView,
    tokens: &Tokens,
    toggle: bool,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let listening = view.capture.is_listening() && view.capture_toggle == toggle;
    let shortcut = if toggle {
        view.phase.prefs().toggle().unwrap_or("Disabled")
    } else {
        view.phase.prefs().hold()
    };
    let label = if listening {
        view.capture.preview().unwrap_or("Press keys…").to_string()
    } else {
        pretty(shortcut)
    };
    div()
        .id(if toggle {
            "onboarding-toggle-card"
        } else {
            "onboarding-hold-card"
        })
        .flex_1()
        .min_w_0()
        .h(px(232.))
        .p(px(24.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .rounded(px(10.))
        .border_1()
        .border_color(if listening {
            tokens.accent
        } else {
            tokens.hairline
        })
        .bg(tokens.group)
        .when(listening, |el| {
            el.track_focus(&view.hold_focus)
                .on_key_down(cx.listener(SettingsView::on_hold_key))
                .on_key_up(cx.listener(SettingsView::on_hold_key_up))
                .on_modifiers_changed(cx.listener(SettingsView::on_hold_modifiers))
        })
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(if toggle { "Hands free" } else { "Push to talk" }),
        )
        .child(div().text_xs().text_color(tokens.muted).child(if toggle {
            "Press to start and stop"
        } else {
            "Hold to speak"
        }))
        .child(div().flex_1())
        .child(
            div()
                .id(if toggle {
                    "onboarding-toggle-change"
                } else {
                    "onboarding-hold-change"
                })
                .w_full()
                .min_h(px(64.))
                .px(px(4.))
                .py(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if listening {
                    tokens.accent
                } else {
                    tokens.group
                })
                .bg(tokens.group)
                .text_sm()
                .text_center()
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.edit_onboarding_shortcut(toggle, window, cx);
                }))
                .when(!view.capture.is_listening(), |el| {
                    el.tab_index(0).on_key_down(cx.listener(
                        move |view, event: &gpui::KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                && !event.is_held
                            {
                                cx.stop_propagation();
                                window.prevent_default();
                                view.edit_onboarding_shortcut(toggle, window, cx);
                            }
                        },
                    ))
                })
                .child(if listening {
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child(SharedString::from(label))
                        .into_any_element()
                } else {
                    shortcut_keys(shortcut, tokens)
                }),
        )
        .child(div().flex_1())
        .child(
            div()
                .id(if toggle {
                    "onboarding-toggle-reset"
                } else {
                    "onboarding-hold-reset"
                })
                .h(px(24.))
                .px(px(8.))
                .flex()
                .items_center()
                .text_xs()
                .text_color(tokens.muted)
                .cursor_pointer()
                .tab_index(0)
                .hover(|style| style.text_color(tokens.text))
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.capture_toggle = toggle;
                    view.reset_hold(window, cx);
                }))
                .on_key_down(
                    cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && !event.is_held
                        {
                            cx.stop_propagation();
                            window.prevent_default();
                            view.capture_toggle = toggle;
                            view.reset_hold(window, cx);
                        }
                    }),
                )
                .child("Reset"),
        )
        .into_any_element()
}

fn shortcut_keys(shortcut: &str, tokens: &Tokens) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .justify_center()
        .items_center()
        .gap(px(6.))
        .children(shortcut.split('+').map(|key| {
            let label = match key.trim() {
                "Super" if cfg!(target_os = "macos") => "⌘",
                "Ctrl" if cfg!(target_os = "macos") => "⌃",
                "Alt" if cfg!(target_os = "macos") => "⌥",
                "Shift" if cfg!(target_os = "macos") => "⇧",
                key => key,
            };
            div()
                .min_w(px(34.))
                .h(px(38.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.content)
                .text_sm()
                .child(SharedString::from(label.to_string()))
        }))
        .into_any_element()
}

impl SettingsView {
    fn edit_onboarding_shortcut(
        &mut self,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.capture.is_listening() && self.capture_toggle != toggle {
            self.capture.cancel();
            self.shortcut_edit.take();
        }
        self.capture_toggle = toggle;
        self.toggle_hold_capture(window, cx);
    }
}

fn permissions_screen(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let setup = view.onboarding.as_ref().unwrap();
    let ready = view.can_finish();
    let microphone_label = if setup.microphone_checking {
        "Checking…"
    } else if setup.permissions.microphone == Permission::Denied {
        "Settings"
    } else {
        "Allow"
    };
    let microphone_action = if setup.permissions.microphone == Permission::Granted {
        permission_allowed(
            tokens,
            if setup.microphone_checking {
                "Checking…"
            } else {
                "Allowed"
            },
        )
    } else {
        button(
            tokens,
            "onboarding-microphone",
            microphone_label,
            !setup.microphone_checking && !view.is_busy() && !view.settings_preview,
            false,
            cx,
            |view, _, cx| {
                let setup = view.onboarding.as_mut().unwrap();
                if setup.permissions.microphone == Permission::NotRequested {
                    permissions::request_microphone();
                } else if setup.permissions.microphone == Permission::Denied {
                    cx.open_url(permissions::microphone_settings_url());
                }
                setup.session_error = None;
                setup.microphone_ready = false;
                cx.notify();
            },
        )
    };
    let mut body = div().flex().flex_col().gap(px(12.)).child(
        div()
            .px(px(20.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(5.))
            .rounded(px(10.))
            .border_1()
            .border_color(tokens.hairline)
            .bg(tokens.group)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Permissions"),
            )
            .child(permission_row(
                tokens,
                "Microphone",
                svg()
                    .path("fluent/mic.svg")
                    .size(px(20.))
                    .text_color(tokens.muted)
                    .into_any_element(),
                microphone_action,
            ))
            .when(cfg!(target_os = "macos"), |el| {
                el.child(div().h(px(1.)).flex_shrink_0().bg(tokens.hairline))
                    .child(permission_row(
                        tokens,
                        "Accessibility",
                        div()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .text_color(tokens.muted)
                            .child("⌘")
                            .into_any_element(),
                        if setup.permissions.accessibility == Permission::Granted {
                            permission_allowed(tokens, "Allowed")
                        } else {
                            button(
                                tokens,
                                "onboarding-accessibility",
                                "Settings",
                                setup.permissions.accessibility != Permission::Granted
                                    && !view.settings_preview,
                                false,
                                cx,
                                |_, _, cx| cx.open_url(permissions::accessibility_settings_url()),
                            )
                        },
                    ))
            }),
    );
    if let Some(error) = &setup.session_error {
        body = body
            .child(super::view::error_text(tokens, error.clone()))
            .when(cfg!(target_os = "macos"), |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child("Check Privacy & Security, then relaunch Plume if needed."),
                )
            })
            .child(button(
                tokens,
                "onboarding-service-retry",
                "Check again",
                !view.is_busy(),
                false,
                cx,
                |view, _, cx| {
                    let setup = view.onboarding.as_mut().unwrap();
                    setup.session_error = None;
                    setup.microphone_ready = false;
                    cx.notify();
                },
            ));
    }
    let status = match &setup.feedback {
        Feedback::Starting => "Opening microphone…".to_string(),
        Feedback::RecordingNotice { .. } => "Listening…".into(),
        Feedback::Transcribing => "Transcribing…".into(),
        Feedback::Cancelling => "Cancelling…".into(),
        Feedback::NoSpeech => "No speech detected. Try again.".into(),
        Feedback::Success => "Done".into(),
        Feedback::Error { title, advice } => format!("{title}: {advice}"),
        _ if setup.permissions.microphone != Permission::Granted
            && setup.permissions.accessibility != Permission::Granted =>
        {
            "Allow Microphone and Accessibility to continue".into()
        }
        _ if setup.permissions.microphone != Permission::Granted => {
            "Allow Microphone to continue".into()
        }
        _ if setup.permissions.accessibility != Permission::Granted => {
            "Enable Plume in Accessibility settings to continue".into()
        }
        _ if setup.session_error.is_some() => "Resolve the error above to continue".into(),
        _ if setup.microphone_checking => "Checking microphone…".into(),
        _ if !setup.model_ready => "Loading model…".into(),
        _ if setup.session_starting => "Starting shortcut service…".into(),
        _ if ready => "Hold to speak".into(),
        _ if view.save_error.is_some() => "Save settings to continue".into(),
        _ => "Preparing dictation…".into(),
    };
    let active = matches!(
        setup.feedback,
        Feedback::Starting
            | Feedback::RecordingNotice { .. }
            | Feedback::Transcribing
            | Feedback::Cancelling
    );
    let idle_ready = ready && matches!(setup.feedback, Feedback::Empty | Feedback::NoSpeech);
    body.child(
        div()
            .h(px(180.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .rounded(px(10.))
            .border_1()
            .border_color(if active {
                tokens.accent
            } else {
                tokens.hairline
            })
            .bg(tokens.group)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Try dictation"),
                    )
                    .child(div().text_xs().text_color(tokens.muted).child("Optional")),
            )
            .child(if setup.transcript.is_empty() {
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(if idle_ready {
                        shortcut_keys(view.phase.prefs().hold(), tokens)
                    } else {
                        svg()
                            .path("fluent/mic.svg")
                            .size(px(28.))
                            .text_color(if active { tokens.accent } else { tokens.muted })
                            .into_any_element()
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(tokens.muted)
                            .text_center()
                            .child(status),
                    )
                    .into_any_element()
            } else {
                div()
                    .id("onboarding-test-transcript")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_sm()
                    .child(setup.transcript.clone())
                    .into_any_element()
            }),
    )
    .into_any_element()
}

fn permission_row(
    tokens: &Tokens,
    title: &'static str,
    icon: AnyElement,
    action: AnyElement,
) -> AnyElement {
    div()
        .h(px(32.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(icon)
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(tokens.muted)
                .child(title),
        )
        .child(action)
        .into_any_element()
}

fn permission_allowed(tokens: &Tokens, label: &'static str) -> AnyElement {
    div()
        .h(px(32.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_xs()
        .text_color(tokens.muted)
        .child("✓")
        .child(label)
        .into_any_element()
}

fn pretty(shortcut: &str) -> String {
    shortcut.split('+').collect::<Vec<_>>().join(" + ")
}

fn button(
    tokens: &Tokens,
    id: &'static str,
    label: &'static str,
    enabled: bool,
    _primary: bool,
    cx: &mut Context<SettingsView>,
    handler: impl Fn(&mut SettingsView, &mut Window, &mut Context<SettingsView>) + 'static,
) -> AnyElement {
    let handler = std::rc::Rc::new(handler);
    let click = handler.clone();
    div()
        .id(id)
        .px(px(11.))
        .h(px(32.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.fill)
        .text_color(if enabled { tokens.text } else { tokens.muted })
        .when(!enabled, |el| el.opacity(0.65))
        .when(enabled, |el| {
            el.cursor_pointer()
                .tab_index(0)
                .hover(|style| style.bg(tokens.fill_hover))
                .active(|style| style.bg(tokens.group))
                .on_click(cx.listener(move |view, _, window, cx| click(view, window, cx)))
                .on_key_down(
                    cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                            && !event.is_held
                        {
                            cx.stop_propagation();
                            window.prevent_default();
                            handler(view, window, cx);
                        }
                    }),
                )
        })
        .child(label)
        .into_any_element()
}
