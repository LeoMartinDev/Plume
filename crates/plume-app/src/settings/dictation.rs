use gpui::{deferred, div, prelude::*, px, svg, AnyElement, Context, FontWeight, SharedString};
use plume_session::InsertionMode;
use plume_ui::{Segment, Segmented, Tokens};

use crate::prefs::Prefs;
use crate::shortcut_capture::pill_label;

use super::view::{error_text, page, settings_group};
use super::SettingsView;

const HOLD_CAPTURE_ID: &str = "hold-capture";
const INSERTION_MODES: [(InsertionMode, &str); 3] = [
    (InsertionMode::Auto, "Automatic"),
    (InsertionMode::Clipboard, "Paste"),
    (InsertionMode::Typing, "Typing"),
];

pub(super) fn dictation_page(
    view: &SettingsView,
    tokens: &Tokens,
    prefs: &Prefs,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let listening = view.capture.is_listening();
    let label = if listening {
        SharedString::from(
            view.capture
                .preview()
                .unwrap_or_else(|| pill_label(view.capture.phase(), prefs.hold()))
                .to_string(),
        )
    } else {
        SharedString::from(pretty_hold(prefs.hold()))
    };
    let error = view
        .capture
        .reject()
        .map(str::to_string)
        .or_else(|| view.save_error.clone());

    page(
        "Dictation",
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                settings_group(tokens)
                    .id(HOLD_CAPTURE_ID)
                    .track_focus(&view.hold_focus)
                    .min_h(px(58.))
                    .px(px(14.))
                    .py(px(11.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .when(listening, |el| el.bg(tokens.fill))
                    .when(listening, |el| {
                        el.on_key_down(cx.listener(SettingsView::on_hold_key))
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
                            .child(div().text_sm().child("Push to talk"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(tokens.muted)
                                    .child(if listening {
                                        if view.capture.preview().is_some() {
                                            "Release the keys to apply"
                                        } else {
                                            "Press the new shortcut"
                                        }
                                    } else {
                                        "Hold while speaking"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("hold-change")
                                    .h(px(32.))
                                    .px(px(11.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .bg(tokens.fill)
                                    .border_1()
                                    .border_color(tokens.hairline)
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(tokens.fill_hover))
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.toggle_hold_capture(window, cx);
                                    }))
                                    .child(label),
                            )
                            .child(
                                div()
                                    .id("hold-reset")
                                    .h(px(32.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .text_xs()
                                    .text_color(tokens.muted)
                                    .cursor_pointer()
                                    .hover(|style| {
                                        style.bg(tokens.fill_hover).text_color(tokens.text)
                                    })
                                    .on_click(cx.listener(|this, _event, window, cx| {
                                        this.reset_hold(window, cx);
                                    }))
                                    .child("Reset"),
                            ),
                    ),
            )
            .child(insertion_mode_row(
                tokens,
                prefs.insertion_mode(),
                view.insertion_open,
                cx,
            ))
            .child(copy_on_failure_row(tokens, prefs.copy_on_failure(), cx))
            .children(error.map(|text| error_text(tokens, text))),
    )
}

fn insertion_mode_row(
    tokens: &Tokens,
    selected: InsertionMode,
    open: bool,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_group(tokens)
        .relative()
        .min_h(px(64.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_sm().child("Text insertion"))
                .child(
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child("Paste mode replaces the clipboard contents"),
                ),
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
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.insertion_open = !this.insertion_open;
                    cx.notify();
                }))
                .child(insertion_mode_label(selected))
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
                    .top(px(10.))
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
                                .child(label)
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
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    settings_group(tokens)
        .min_h(px(58.))
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_sm().child("Copy after an insertion failure"))
                .child(
                    div()
                        .text_xs()
                        .text_color(tokens.muted)
                        .child("Keeps the final transcript available for manual paste"),
                ),
        )
        .child(Segmented::new(
            *tokens,
            enabled,
            [
                Segment::new(true, "copy-failure-on", "On").on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        if !this.phase.prefs().copy_on_failure() {
                            this.toggle_copy_on_failure(cx);
                        }
                    },
                ),
                Segment::new(false, "copy-failure-off", "Off").on_click(
                    cx,
                    |this: &mut SettingsView, cx| {
                        if this.phase.prefs().copy_on_failure() {
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
