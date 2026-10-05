use gpui::{
    deferred, div, prelude::*, px, svg, AnyElement, Context, ElementId, FontWeight, SharedString,
};
use stt_ui::{ListGroup, Tokens};

use super::view::{error_text, page, settings_group};
use super::{HistoryLimitMenu, SettingsView};
use crate::history_policy::HistoryPolicy;

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
        .child(history_limit_row(
            view,
            tokens,
            HistoryLimitMenu::Retention,
            cx,
        ))
        .child(history_limit_row(
            view,
            tokens,
            HistoryLimitMenu::Entries,
            cx,
        ))
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child("Reducing a limit immediately deletes older or excess transcriptions."),
        )
        .children(
            view.history_error
                .clone()
                .or_else(|| view.save_error.clone())
                .map(|error| error_text(tokens, error)),
        )
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

    page("History", body)
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

fn history_limit_row(
    view: &SettingsView,
    tokens: &Tokens,
    menu: HistoryLimitMenu,
    cx: &mut Context<SettingsView>,
) -> impl IntoElement {
    let policy = view.phase.prefs().history();
    let (id, title, selected, options) = match menu {
        HistoryLimitMenu::Retention => (
            "history-retention",
            "Keep transcriptions for",
            policy.retention_days().map(|days| days as usize),
            vec![Some(7), Some(30), Some(90), None],
        ),
        HistoryLimitMenu::Entries => (
            "history-entries",
            "Maximum transcriptions",
            policy.max_entries(),
            vec![Some(100), Some(500), Some(5_000), None],
        ),
    };
    let label = move |value: Option<usize>| match (menu, value) {
        (_, None) => "Unlimited".to_string(),
        (HistoryLimitMenu::Retention, Some(days)) => format!("{days} days"),
        (HistoryLimitMenu::Entries, Some(entries)) => format!("{entries} entries"),
    };
    let open = view.history_menu == Some(menu);
    let menu_height = 46. + 30. * options.len() as f32;
    settings_group(tokens)
        .relative()
        .min_h(px(58.))
        .px(px(14.))
        .py(px(11.))
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_sm().child(title))
        .child(
            div()
                .id(id)
                .w(px(156.))
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.fill)
                .text_sm()
                .cursor_pointer()
                .hover(|style| style.bg(tokens.fill_hover))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.history_menu = if this.history_menu == Some(menu) {
                        None
                    } else {
                        Some(menu)
                    };
                    cx.notify();
                }))
                .child(label(selected))
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
                    .id(SharedString::from(format!("{id}-boundary")))
                    .absolute()
                    .top(px(10.))
                    .right(px(14.))
                    .w(px(156.))
                    .h(px(menu_height))
                    .on_mouse_down_out(cx.listener(|this, _event, _window, cx| {
                        this.history_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                deferred(
                    div()
                        .id(SharedString::from(format!("{id}-menu")))
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
                        .children(options.into_iter().map(|value| {
                            div()
                                .id(SharedString::from(format!("{id}-{value:?}")))
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_sm()
                                .when(value == selected, |option| {
                                    option.bg(tokens.fill).font_weight(FontWeight::MEDIUM)
                                })
                                .cursor_pointer()
                                .hover(|style| style.bg(tokens.fill_hover))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.history_menu = None;
                                    let current = this.phase.prefs().history();
                                    let next = match menu {
                                        HistoryLimitMenu::Retention => HistoryPolicy::new(
                                            value.map(|days| days as u32),
                                            current.max_entries(),
                                        ),
                                        HistoryLimitMenu::Entries => {
                                            HistoryPolicy::new(current.retention_days(), value)
                                        }
                                    }
                                    .expect("validated history option");
                                    this.commit_history_policy(next, cx);
                                }))
                                .child(label(value))
                        })),
                )
                .with_priority(1),
            )
        })
}
