use gpui::{
    deferred, div, prelude::*, px, relative, svg, AnyElement, Context, Div, FontWeight,
    SharedString,
};
use plume_ui::Tokens;

use crate::catalog::{self, ModelEntry, ModelId};
use crate::phase::{AppPhase, OnboardStatus, Progress};
use crate::prefs::{LanguagePref, Prefs};

use super::view::{error_text, page, settings_divider, settings_group, settings_section};
use super::SettingsView;

const LANGUAGES: [LanguagePref; 3] = [
    LanguagePref::Auto,
    LanguagePref::French,
    LanguagePref::English,
];

pub(super) fn model_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    page(
        "Models",
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(settings_section(
                tokens,
                "Recognition",
                settings_group(tokens).child(language_row(
                    tokens,
                    prefs.language(),
                    view.language_open,
                    cx,
                )),
            ))
            .child(settings_section(
                tokens,
                "Available models",
                model_catalog(view, tokens, prefs, cx),
            ))
            .children(model_error(&view.phase).map(|text| error_text(tokens, text)))
            .children(view.save_error.clone().map(|text| error_text(tokens, text))),
    )
}

fn model_catalog(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let count = catalog::entries().len();
    settings_group(tokens).flex().flex_col().children(
        catalog::entries()
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                let mut rows = vec![catalog_model_row(
                    entry,
                    view,
                    index == 0,
                    index + 1 == count,
                    tokens,
                    prefs,
                    cx,
                )
                .into_any_element()];
                if index + 1 < catalog::entries().len() {
                    rows.push(settings_divider(tokens).into_any_element());
                }
                rows
            }),
    )
}

fn catalog_model_row(
    entry: &ModelEntry,
    view: &SettingsView,
    is_first: bool,
    is_last: bool,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let phase = &view.phase;
    let progress = match phase {
        AppPhase::Onboarding {
            prefs,
            status: OnboardStatus::Fetching { last },
            ..
        } if prefs.model == entry.id => Some(last),
        _ => None,
    };
    let downloading: Option<SharedString> =
        progress.and_then(download_status).map(SharedString::from);
    let supported = entry.id.supports(prefs.language());
    let installed = catalog::is_complete(entry.id, &entry.id.data_dir());
    let (label, enabled) = catalog_model_action(phase, view.active_model, entry.id, installed);
    // A pack that does not cover the pinned language cannot be selected.
    let (label, enabled) = if supported {
        (label, enabled && !view.settings_preview)
    } else {
        ("Unsupported".into(), false)
    };
    let can_delete = installed && !view.downloads.is_active_for(entry.id) && !view.settings_preview;
    let is_current = view.active_model == Some(entry.id);
    let detail = entry.size;
    div()
        .id(SharedString::from(format!("model-{}", entry.id.as_str())))
        .w_full()
        .relative()
        .min_h(px(88.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(16.))
        // Highlight only the model connected to the running session. A saved
        // preference or pending download does not mean a model is in use.
        .when(is_current && supported, |el| {
            let el = el.bg(tokens.accent_soft);
            if is_first && is_last {
                el.rounded(px(10.))
            } else if is_first {
                el.rounded_t(px(10.))
            } else if is_last {
                el.rounded_b(px(10.))
            } else {
                el
            }
        })
        // Pack hors langue épinglée : grisé, non cliquable.
        .when(!supported, |el| el.opacity(0.45))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(entry.name),
                )
                .child(match downloading {
                    Some(text) => div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child(text)
                        .into_any_element(),
                    None => div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(div().text_xs().text_color(tokens.muted).child(detail))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .gap(px(14.))
                                .child(spec_meter("Speed", entry.speed, tokens))
                                .child(spec_meter("Accuracy", entry.accuracy, tokens)),
                        )
                        .into_any_element(),
                }),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .child(div().w(px(32.)).flex_shrink_0().children(
                    can_delete.then(|| catalog_model_delete_button(tokens, entry.id, cx)),
                ))
                .child(catalog_model_button(tokens, entry.id, label, enabled, cx)),
        )
        .children(progress.and_then(|progress| catalog_model_progress(progress, tokens)))
}

/// 5 uniform rounded bars, neutral theme colors (no accent blue).
/// `value` is 1..=5, more filled bars = faster / more accurate.
fn spec_meter(label: &'static str, value: u8, tokens: &Tokens) -> impl IntoElement {
    let value = value.clamp(1, 5);
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .min_w(px(56.))
                .flex_shrink_0()
                .text_xs()
                .whitespace_nowrap()
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(3.))
                .children((0..5).map(move |i| {
                    div().w(px(3.)).h(px(7.)).rounded(px(1.)).bg(if i < value {
                        tokens.text
                    } else {
                        tokens.hairline
                    })
                })),
        )
}

fn catalog_model_action(
    phase: &AppPhase,
    active_model: Option<ModelId>,
    id: ModelId,
    installed: bool,
) -> (SharedString, bool) {
    if active_model == Some(id) {
        return (if installed { "In use" } else { "In memory" }.into(), false);
    }
    if phase.prefs().model == id {
        return match phase {
            AppPhase::Live { .. } => (if installed { "Use" } else { "Download" }.into(), true),
            AppPhase::Onboarding { status, .. } => match status {
                OnboardStatus::Idle => (if installed { "Use" } else { "Download" }.into(), true),
                OnboardStatus::Activating => ("Loading…".into(), false),
                OnboardStatus::Fetching { .. } => ("Downloading".into(), false),
                OnboardStatus::Failed { .. } => ("Retry".into(), true),
            },
            AppPhase::Refused { .. } => ("Retry".into(), true),
        };
    }
    let busy = matches!(
        phase,
        AppPhase::Onboarding {
            status: OnboardStatus::Activating | OnboardStatus::Fetching { .. },
            ..
        }
    );
    (if installed { "Use" } else { "Download" }.into(), !busy)
}

fn catalog_model_button(
    tokens: &Tokens,
    id: ModelId,
    label: SharedString,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let status = matches!(label.as_ref(), "In use" | "In memory");
    div()
        .id(SharedString::from(format!("model-{}-action", id.as_str())))
        .h(px(32.))
        .w(px(108.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .when(!status, |el| {
            el.border_1().border_color(tokens.hairline).bg(tokens.fill)
        })
        .text_sm()
        .text_color(if enabled || status {
            tokens.text
        } else {
            tokens.muted
        })
        .when(status, |el| el.font_weight(FontWeight::MEDIUM))
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .active(|style| style.bg(tokens.group))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.capture.cancel();
                    let mut prefs = this.phase.prefs().clone();
                    prefs.model = id;
                    let request = this.begin_download(prefs.clone(), cx);
                    crate::begin_pack(cx, prefs, request);
                }))
        })
        .child(label)
}

fn catalog_model_delete_button(
    tokens: &Tokens,
    id: ModelId,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("model-{}-delete", id.as_str())))
        .h(px(32.))
        .w(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_color(tokens.muted)
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover).text_color(tokens.text))
        .active(|style| style.bg(tokens.fill))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.capture.cancel();
            this.delete_model(id, cx);
        }))
        .child(
            svg()
                .path("fluent/delete.svg")
                .size(px(16.))
                .text_color(tokens.muted),
        )
}

fn catalog_model_progress(progress: &Progress, tokens: &Tokens) -> Option<Div> {
    let total = progress.total.filter(|total| *total > 0)?;
    let fraction = (progress.bytes as f64 / total as f64).clamp(0.0, 1.0) as f32;
    Some(
        div()
            .absolute()
            .left(px(14.))
            .right(px(14.))
            .bottom(px(3.))
            .h(px(3.))
            .rounded(px(2.))
            .bg(tokens.fill)
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded(px(2.))
                    .bg(tokens.accent),
            ),
    )
}

fn model_error(phase: &AppPhase) -> Option<String> {
    match phase {
        AppPhase::Live { .. } => None,
        AppPhase::Onboarding {
            status, warning, ..
        } => match status {
            OnboardStatus::Idle | OnboardStatus::Activating | OnboardStatus::Fetching { .. } => {
                warning.clone()
            }
            OnboardStatus::Failed { reason } => Some(reason.clone()),
        },
        AppPhase::Refused { reason, .. } => Some(reason.clone()),
    }
}

fn download_status(progress: &Progress) -> Option<String> {
    let mut parts = Vec::with_capacity(2);
    if let Some(total) = progress.total.filter(|total| *total > 0) {
        let percent = (progress.bytes.saturating_mul(100) / total).min(100);
        parts.push(format!("{percent}%"));
    }
    if let Some(speed) = progress_speed(progress.bytes_per_second) {
        parts.push(speed);
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn progress_speed(bytes_per_second: Option<u64>) -> Option<String> {
    bytes_per_second.map(|bytes| format!("{}/s", format_bytes(bytes)))
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

fn language_row(
    tokens: &Tokens,
    selected: LanguagePref,
    open: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    div()
        .w_full()
        .relative()
        .min_h(px(56.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(16.))
        .child(div().flex_1().min_w_0().text_sm().child("Language"))
        .child(
            div()
                .id("language-select")
                .w(px(156.))
                .flex_shrink_0()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.fill)
                .text_sm()
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .when(open, |el| el.bg(tokens.fill_hover))
                .active(|style| style.bg(tokens.group))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.language_open = !this.language_open;
                    cx.notify();
                }))
                .child(selected.label())
                .child(
                    svg()
                        .path("fluent/chevron-down.svg")
                        .size(px(14.))
                        .text_color(tokens.muted),
                ),
        )
        .when(open, |el| {
            el.child(
                // This hitbox spans both the trigger and the popup. It observes clicks
                // elsewhere in the window without intercepting language option clicks.
                div()
                    .id("language-menu-boundary")
                    .absolute()
                    .top(px(10.))
                    .right(px(14.))
                    .w(px(156.))
                    .h(px(136.))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.language_open = false;
                        cx.notify();
                    })),
            )
            .child(
                // The model catalog is a later sibling, so paint the menu after the page content.
                deferred(
                    div()
                        .absolute()
                        // Open into the page instead of the native title bar. The menu is
                        // deferred below, so it still paints over the model catalog.
                        .top(px(46.))
                        .right(px(14.))
                        .w(px(156.))
                        .p(px(4.))
                        .flex()
                        .flex_col()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(tokens.hairline)
                        .bg(tokens.elevated)
                        .shadow_md()
                        .children(LANGUAGES.into_iter().map(|language| {
                            let active = language == selected;
                            div()
                                .id(SharedString::from(format!(
                                    "language-{}",
                                    language.as_str()
                                )))
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_sm()
                                .when(active, |row| {
                                    row.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
                                })
                                .when(!active, |row| {
                                    row.cursor_pointer()
                                        .hover(|style| style.bg(tokens.fill_hover))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.commit_language(language, cx);
                                        }))
                                })
                                .child(language.label())
                        })),
                )
                .with_priority(1),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{catalog_model_action, download_status, format_bytes};
    use crate::catalog::ModelId;
    use crate::phase::{AppPhase, OnboardStatus, Progress};
    use crate::prefs::Prefs;

    fn onboarding(model: ModelId, status: OnboardStatus) -> AppPhase {
        let mut prefs = Prefs::default_fresh();
        prefs.model = model;
        AppPhase::Onboarding {
            prefs,
            status,
            warning: None,
        }
    }

    #[test]
    fn default_preference_is_not_an_active_model() {
        let phase = onboarding(ModelId::Nemotron35Compact, OnboardStatus::Idle);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "Download");
        assert!(enabled);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "Use");
        assert!(enabled);
    }

    #[test]
    fn installed_model_is_loading_until_the_session_is_connected() {
        let phase = onboarding(ModelId::WhisperLargeV3Turbo, OnboardStatus::Activating);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "Loading…");
        assert!(!enabled);
        let (label, enabled) =
            catalog_model_action(&phase, None, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "Download");
        assert!(!enabled);
    }

    #[test]
    fn switching_models_keeps_the_running_model_in_use() {
        let phase = onboarding(
            ModelId::Nemotron35Compact,
            OnboardStatus::Fetching {
                last: Progress {
                    file: "encoder.onnx".into(),
                    bytes: 0,
                    total: None,
                    bytes_per_second: None,
                },
            },
        );
        let active = Some(ModelId::WhisperLargeV3Turbo);
        let (label, enabled) =
            catalog_model_action(&phase, active, ModelId::WhisperLargeV3Turbo, true);
        assert_eq!(label.as_ref(), "In use");
        assert!(!enabled);
        let (label, enabled) =
            catalog_model_action(&phase, active, ModelId::Nemotron35Compact, false);
        assert_eq!(label.as_ref(), "Downloading");
        assert!(!enabled);
    }

    #[test]
    fn deleting_a_loaded_pack_does_not_unload_the_running_engine() {
        let phase = onboarding(ModelId::WhisperLargeV3Turbo, OnboardStatus::Idle);
        let (label, enabled) = catalog_model_action(
            &phase,
            Some(ModelId::WhisperLargeV3Turbo),
            ModelId::WhisperLargeV3Turbo,
            false,
        );
        assert_eq!(label.as_ref(), "In memory");
        assert!(!enabled);
    }

    #[test]
    fn progress_is_compact_and_bounded() {
        assert_eq!(
            download_status(&Progress {
                file: "starting".into(),
                bytes: 0,
                total: None,
                bytes_per_second: None,
            }),
            None
        );
        assert_eq!(
            download_status(&Progress {
                file: "encoder.onnx".into(),
                bytes: 25,
                total: Some(100),
                bytes_per_second: None,
            }),
            Some("25%".to_string())
        );
        assert_eq!(
            download_status(&Progress {
                file: "encoder.onnx".into(),
                bytes: 150,
                total: Some(100),
                bytes_per_second: Some(10 * 1024 * 1024),
            }),
            Some("100% · 10.0 MB/s".to_string())
        );
        assert_eq!(format_bytes(1536), "1.5 KB");
    }
}
