use gpui::{div, prelude::*, px, svg, AnyElement, Context, ElementId, FontWeight};
use stt_ui::{ListGroup, Tokens};

use super::view::{error_text, page, settings_group};
use super::SettingsView;

pub(super) fn history_page(
    view: &SettingsView,
    tokens: &Tokens,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let entries = view.history.entries().to_vec();
    let body = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .when(!entries.is_empty(), |list| {
            list.child(div().w_full().flex().justify_end().child(action_button(
                tokens,
                "history-clear",
                "Clear all",
                cx,
                |this, cx| {
                    this.clear_history(cx);
                },
            )))
        })
        .when(entries.is_empty(), |list| {
            list.child(
                settings_group(tokens)
                    .px(px(14.))
                    .py(px(18.))
                    .text_sm()
                    .text_color(tokens.muted)
                    .child("No transcriptions yet."),
            )
        })
        .when(!entries.is_empty(), |list| {
            list.child(
                ListGroup::new(*tokens).children(entries.into_iter().map(move |entry| {
                    let copy_text = entry.text.clone();
                    let id = entry.id;
                    let copied = view.copied_history_id == Some(id);
                    div()
                        .w_full()
                        .px(px(14.))
                        .py(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(7.))
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(entry.status),
                                        )
                                        .child(
                                            div().text_xs().text_color(tokens.muted).child(
                                                entry
                                                    .application
                                                    .unwrap_or_else(|| "Unknown app".into()),
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(if copied {
                                                    tokens.accent
                                                } else {
                                                    tokens.muted
                                                })
                                                .child(if copied {
                                                    "Copied".to_string()
                                                } else {
                                                    crate::history::age_label(entry.created_at)
                                                }),
                                        )
                                        .child(icon_button(
                                            tokens,
                                            ("history-copy", id),
                                            "fluent/copy.svg",
                                            cx,
                                            move |this, cx| {
                                                this.copy_history(id, copy_text.clone(), cx);
                                            },
                                        ))
                                        .child(icon_button(
                                            tokens,
                                            ("history-delete", id),
                                            "fluent/delete.svg",
                                            cx,
                                            move |this, cx| {
                                                this.delete_history(id, cx);
                                            },
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .max_h(px(64.))
                                .overflow_hidden()
                                .child(entry.text),
                        )
                })),
            )
        });

    page(
        "History",
        body.children(
            view.history_error
                .clone()
                .map(|error| error_text(tokens, error)),
        ),
    )
}

fn action_button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    label: &'static str,
    cx: &mut Context<SettingsView>,
    on_click: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(28.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(tokens.hairline)
        .bg(tokens.fill)
        .text_xs()
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(label)
}

fn icon_button(
    tokens: &Tokens,
    id: impl Into<ElementId>,
    icon: &'static str,
    cx: &mut Context<SettingsView>,
    on_click: impl Fn(&mut SettingsView, &mut Context<SettingsView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .text_color(tokens.muted)
        .cursor_pointer()
        .hover(|style| style.bg(tokens.fill_hover).text_color(tokens.text))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(svg().path(icon).size(px(15.)).text_color(tokens.muted))
}
