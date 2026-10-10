use crate::settings::i18n::tr;
use gpui::{deferred, div, prelude::*, px, svg, AnyElement, Context, FontWeight, SharedString};
use plume_session::{Destination, InsertionMode};
use plume_ui::{Segment, Segmented, Tokens};

use crate::prefs::Prefs;

use super::view::{
    error_text, page, settings_divider, settings_group, settings_row, settings_section,
};
use super::SettingsView;

const HOLD_CAPTURE_ID: &str = "hold-capture";
const INSERTION_MODES: [(InsertionMode, &str); 3] = [
    (InsertionMode::Auto, "ui.automatic"),
    (InsertionMode::Clipboard, "ui.paste"),
    (InsertionMode::Typing, "ui.typing"),
];

pub(super) fn dictation_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let error = view
        .capture
        .reject()
        .map(str::to_string)
        .or_else(|| view.save_error.clone());
    page(
        tr("ui.dictation"),
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .when(
                view.session_control.is_none() && !view.settings_preview,
                |el| {
                    el.child(div().text_sm().child(tr("dictation.model_unavailable")))
                        .child(
                            div()
                                .id("dictation-open-models")
                                .text_sm()
                                .text_color(tokens.accent)
                                .cursor_pointer()
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.section = super::SettingsSection::Model;
                                    cx.notify();
                                }))
                                .child(tr("ui.open_models")),
                        )
                },
            )
            .child(settings_section(
                tokens,
                tr("ui.shortcuts"),
                settings_group(tokens)
                    .flex()
                    .flex_col()
                    .child(shortcut_row(view, tokens, prefs, false, cx))
                    .child(settings_divider(tokens))
                    .child(shortcut_row(view, tokens, prefs, true, cx)),
            ))
            .child(settings_section(
                tokens,
                tr("ui.text_output"),
                settings_group(tokens)
                    .flex()
                    .flex_col()
                    .child(destination_row(tokens, prefs.destination(), cx))
                    .child(settings_divider(tokens))
                    .child(insertion_mode_row(
                        tokens,
                        prefs.insertion_mode(),
                        view.insertion_open,
                        prefs.destination() == Destination::FocusedField,
                        cx,
                    ))
                    .child(settings_divider(tokens))
                    .child(copy_on_failure_row(
                        tokens,
                        prefs.copy_on_failure(),
                        prefs.destination() == Destination::FocusedField,
                        cx,
                    )),
            ))
            .children(error.map(|text| error_text(tokens, text))),
    )
}
pub(super) fn shortcut_row(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    toggle: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let listening = view.capture.is_listening() && view.capture_toggle == toggle;
    let shortcut = if toggle {
        prefs.toggle().unwrap_or(tr("ui.disabled"))
    } else {
        prefs.hold()
    };
    let label = if listening {
        view.capture
            .preview()
            .unwrap_or_else(|| tr("ui.press_keys_progress"))
            .to_string()
    } else {
        pretty_hold(shortcut)
    };
    settings_row()
        .id(if toggle {
            "toggle-capture"
        } else {
            HOLD_CAPTURE_ID
        })
        .when(listening, |el| {
            el.track_focus(&view.hold_focus)
                .on_key_down(cx.listener(SettingsView::on_hold_key))
                .on_key_up(cx.listener(SettingsView::on_hold_key_up))
                .on_modifiers_changed(cx.listener(SettingsView::on_hold_modifiers))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_sm().child(if toggle {
                    tr("ui.hands_free")
                } else {
                    tr("ui.push_to_talk")
                }))
                .when(listening, |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(tokens.muted)
                            .child(tr("ui.press_a_shortcut_then_release_to_apply")),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .gap(px(6.))
                .child(
                    div()
                        .id(if toggle {
                            "toggle-change"
                        } else {
                            "hold-change"
                        })
                        .h(px(32.))
                        .px(px(11.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .bg(tokens.fill)
                        .text_sm()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            if this.capture.is_listening() && this.capture_toggle != toggle {
                                this.capture.cancel();
                                this.shortcut_edit.take();
                            }
                            this.capture_toggle = toggle;
                            this.toggle_hold_capture(window, cx);
                        }))
                        .child(SharedString::from(label)),
                )
                .child(
                    div()
                        .id(if toggle { "toggle-reset" } else { "hold-reset" })
                        .h(px(32.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .text_xs()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.capture_toggle = toggle;
                            this.reset_hold(window, cx);
                        }))
                        .child(tr("ui.reset")),
                ),
        )
}

/// Insert into the focused field, or only copy to the clipboard.
fn destination_row(
    tokens: &Tokens,
    selected: Destination,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_row()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .child(tr("ui.text_destination")),
        )
        .child(Segmented::new(
            *tokens,
            selected,
            [
                Segment::new(
                    Destination::FocusedField,
                    "destination-insert",
                    tr("ui.insert"),
                )
                .on_click(cx, |this: &mut SettingsView, cx| {
                    this.commit_destination(Destination::FocusedField, cx);
                }),
                Segment::new(Destination::Clipboard, "destination-copy", tr("ui.copy")).on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        this.commit_destination(Destination::Clipboard, cx);
                    },
                ),
            ],
        ))
}

/// Insertion settings only apply when text goes into the focused field.
fn inserting(view: &SettingsView) -> bool {
    view.phase.prefs().destination() == Destination::FocusedField
}

fn insertion_mode_row(
    tokens: &Tokens,
    selected: InsertionMode,
    open: bool,
    enabled: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let open = open && enabled;
    settings_row()
        .relative()
        .when(!enabled, |row| row.opacity(0.45))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .child(tr("ui.text_insertion")),
        )
        .child(
            div()
                .id("insertion-select")
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
                .when(enabled, |select| {
                    select
                        .cursor_pointer()
                        .hover(|style| style.bg(tokens.fill_hover))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.insertion_open = !this.insertion_open;
                            cx.notify();
                        }))
                })
                .child(tr(insertion_mode_label(selected)))
                .child(
                    svg()
                        .path("fluent/chevron-down.svg")
                        .size(px(14.))
                        .text_color(tokens.muted),
                ),
        )
        .when(open, |row| {
            row.child(
                div()
                    .id("insertion-menu-boundary")
                    .absolute()
                    .bottom(px(10.))
                    .right(px(14.))
                    .w(px(156.))
                    .h(px(136.))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.insertion_open = false;
                        cx.notify();
                    })),
            )
            .child(
                deferred(
                    div()
                        .absolute()
                        // This group sits near the bottom of the settings window.
                        .bottom(px(46.))
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
                        .children(INSERTION_MODES.into_iter().map(|(mode, label)| {
                            let active = mode == selected;
                            div()
                                .id(SharedString::from(format!(
                                    "insertion-option-{}",
                                    mode.as_str()
                                )))
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_sm()
                                .when(active, |option| {
                                    option.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
                                })
                                .when(!active, |option| {
                                    option
                                        .cursor_pointer()
                                        .hover(|style| style.bg(tokens.fill_hover))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.commit_insertion_mode(mode, cx);
                                        }))
                                })
                                .child(tr(label))
                        })),
                )
                .with_priority(1),
            )
        })
}

fn insertion_mode_label(mode: InsertionMode) -> &'static str {
    INSERTION_MODES
        .into_iter()
        .find_map(|(candidate, label)| (candidate == mode).then_some(label))
        .expect("every insertion mode has a label")
}

fn copy_on_failure_row(
    tokens: &Tokens,
    enabled: bool,
    available: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_row()
        .when(!available, |row| row.opacity(0.45))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .child(tr("ui.copy_on_insertion_failure")),
        )
        .child(Segmented::new(
            *tokens,
            enabled,
            [
                Segment::new(true, "copy-failure-on", tr("ui.on")).on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        if inserting(this) && !this.phase.prefs().copy_on_failure() {
                            this.toggle_copy_on_failure(cx);
                        }
                    },
                ),
                Segment::new(false, "copy-failure-off", tr("ui.off")).on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        if inserting(this) && this.phase.prefs().copy_on_failure() {
                            this.toggle_copy_on_failure(cx);
                        }
                    },
                ),
            ],
        ))
}

fn pretty_hold(hold: &str) -> String {
    hold.split('+').collect::<Vec<_>>().join(" + ")
}

#[cfg(test)]
mod tests {
    use super::pretty_hold;

    #[test]
    fn pretty_hold_spaces_tokens_and_leaves_a_bare_trigger() {
        assert_eq!(pretty_hold("Ctrl+Space"), "Ctrl + Space");
        assert_eq!(pretty_hold("Alt+a"), "Alt + a");
        assert_eq!(pretty_hold("F9"), "F9");
    }
}
