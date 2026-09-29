use gpui::{
    div, prelude::*, px, rgb, AnyElement, App, ClickEvent, Context, Div, ElementId, FontWeight,
    IntoElement, RenderOnce, Rgba, SharedString, Window,
};

use crate::tokens::Tokens;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

fn bind_click<V: 'static>(
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &mut Context<V>) + 'static,
) -> ClickHandler {
    Box::new(cx.listener(move |view, _event: &ClickEvent, _window, cx| {
        handler(view, cx);
    }))
}

pub struct Segment<T> {
    value: T,
    id: ElementId,
    label: SharedString,
    on_click: Option<ClickHandler>,
}

impl<T> Segment<T> {
    pub fn new(value: T, id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Segment {
            value,
            id: id.into(),
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click<V: 'static>(
        mut self,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_click = Some(bind_click(cx, handler));
        self
    }
}

#[derive(IntoElement)]
pub struct Segmented<T: PartialEq + 'static> {
    tokens: Tokens,
    selected: T,
    segments: Vec<Segment<T>>,
}

impl<T: PartialEq + 'static> Segmented<T> {
    pub fn new(
        tokens: Tokens,
        selected: T,
        segments: impl IntoIterator<Item = Segment<T>>,
    ) -> Self {
        Segmented {
            tokens,
            selected,
            segments: segments.into_iter().collect(),
        }
    }
}

impl<T: PartialEq + 'static> RenderOnce for Segmented<T> {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let tokens = self.tokens;
        let selected = self.selected;
        div().flex().flex_row().child(
            div()
                .flex()
                .flex_row()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(tokens.hairline)
                .bg(tokens.fill)
                .children(self.segments.into_iter().map(move |segment| {
                    let is_selected = segment.value == selected;
                    div()
                        .id(segment.id)
                        .px_2()
                        .py_1()
                        .text_sm()
                        .when_else(
                            is_selected,
                            |el| {
                                el.bg(tokens.elevated)
                                    .text_color(tokens.text)
                                    .font_weight(FontWeight::MEDIUM)
                            },
                            |el| {
                                el.text_color(tokens.text)
                                    .hover(|style| style.bg(tokens.fill_hover))
                                    .active(|style| style.bg(tokens.fill))
                            },
                        )
                        .when_some(segment.on_click, |el, on_click| {
                            el.cursor_pointer().on_click(on_click)
                        })
                        .child(segment.label)
                })),
        )
    }
}

#[derive(IntoElement)]
pub struct ListGroup {
    tokens: Tokens,
    children: Vec<AnyElement>,
}

impl ListGroup {
    pub fn new(tokens: Tokens) -> Self {
        ListGroup {
            tokens,
            children: Vec::new(),
        }
    }
}

impl ParentElement for ListGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ListGroup {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let tokens = self.tokens;
        let mut items = Vec::new();
        for (i, child) in self.children.into_iter().enumerate() {
            if i > 0 {
                items.push(
                    div()
                        .w_full()
                        .h(px(1.))
                        .bg(tokens.hairline)
                        .into_any_element(),
                );
            }
            items.push(child);
        }
        div()
            .flex()
            .flex_col()
            .w_full()
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(tokens.hairline)
            .bg(tokens.group)
            .children(items)
    }
}

#[derive(IntoElement)]
pub struct InsetRow {
    tokens: Tokens,
    id: ElementId,
    title: SharedString,
    meta: SharedString,
    detail: SharedString,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl InsetRow {
    pub fn new(tokens: Tokens, id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        InsetRow {
            tokens,
            id: id.into(),
            title: title.into(),
            meta: SharedString::from(""),
            detail: SharedString::from(""),
            selected: false,
            on_click: None,
        }
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = meta.into();
        self
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click<V: 'static>(
        mut self,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_click = Some(bind_click(cx, handler));
        self
    }
}

impl RenderOnce for InsetRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .w_full()
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .when(self.selected, |el| el.bg(self.tokens.fill))
            .when_some(self.on_click, |el, on_click| {
                el.cursor_pointer()
                    .when(!self.selected, |el| {
                        el.hover(|style| style.bg(self.tokens.fill_hover))
                    })
                    .on_click(on_click)
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .text_color(self.tokens.text)
                            .child(self.title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(self.tokens.muted)
                            .px_2()
                            .py_1()
                            .rounded_full()
                            .border_1()
                            .border_color(self.tokens.hairline)
                            .bg(self.tokens.canvas)
                            .child(self.meta),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(self.tokens.muted)
                    .child(self.detail),
            )
    }
}

#[derive(IntoElement)]
pub struct AccentButton {
    tokens: Tokens,
    id: ElementId,
    label: SharedString,
    on_click: ClickHandler,
}

impl AccentButton {
    pub fn new<V: 'static>(
        tokens: Tokens,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Context<V>) + 'static,
    ) -> Self {
        AccentButton {
            tokens,
            id: id.into(),
            label: label.into(),
            on_click: bind_click(cx, handler),
        }
    }
}

impl RenderOnce for AccentButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .px_3()
            .py_2()
            .rounded_md()
            .bg(self.tokens.accent)
            .text_color(self.tokens.on_accent)
            .cursor_pointer()
            .hover(|style| style.opacity(0.92))
            .on_click(self.on_click)
            .child(self.label)
    }
}

const BAR_SHAPE: [f32; 8] = [0.32, 0.68, 0.46, 1.0, 0.82, 0.42, 0.72, 0.28];

struct BubbleChrome {
    fill: Rgba,
    mark: Rgba,
}

fn bubble_chrome() -> BubbleChrome {
    BubbleChrome {
        fill: rgb(0x000000),
        mark: rgb(0xffffff),
    }
}

#[derive(IntoElement)]
pub struct BubbleFrame {
    chrome: BubbleChrome,
    level: f32,
}

impl BubbleFrame {
    pub fn new(level: f32) -> Self {
        BubbleFrame {
            chrome: bubble_chrome(),
            level,
        }
    }
}

impl RenderOnce for BubbleFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mark = self.chrome.mark;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .px(px(14.))
            .size_full()
            .rounded_full()
            .bg(self.chrome.fill)
            .child(voice_dots(self.level, mark))
    }
}

fn voice_dots(level: f32, color: Rgba) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .h(px(20.))
        .children((0..BAR_SHAPE.len()).map(|index| voice_bar(index, level, color)))
}

fn voice_bar(index: usize, level: f32, color: Rgba) -> Div {
    div()
        .w(px(4.))
        .h(px(bar_height(index, level)))
        .rounded(px(2.))
        .bg(color)
}

fn visual_level(level: f32) -> f32 {
    ((level.clamp(0.0, 1.0) - 0.02) / 0.48)
        .clamp(0.0, 1.0)
        .sqrt()
}

fn bar_height(index: usize, level: f32) -> f32 {
    3.0 + BAR_SHAPE[index] * visual_level(level) * 17.0
}

#[cfg(test)]
mod voice_tests {
    use gpui::rgb;

    use super::*;

    #[test]
    fn dark_bubble_is_black_with_white_marks() {
        let chrome = bubble_chrome();
        assert_eq!(chrome.fill, rgb(0x000000));
        assert_eq!(chrome.mark, rgb(0xffffff));
    }

    #[test]
    fn voice_bars_have_a_wider_visible_range() {
        assert_eq!(BAR_SHAPE.len(), 8);
        assert_eq!(bar_height(0, 0.0), 3.0);
        assert_eq!(bar_height(3, 0.0), 3.0);
        assert_eq!(bar_height(3, 0.5), 20.0);
        assert!(bar_height(3, 0.5) - bar_height(7, 0.5) > 12.0);
        assert_eq!(bar_height(3, 1.0), 20.0);
    }

    #[test]
    fn visual_level_amplifies_normal_speech_without_animating_noise() {
        assert_eq!(visual_level(0.0), 0.0);
        assert_eq!(visual_level(0.02), 0.0);
        assert!(visual_level(0.1) > 0.4);
        assert_eq!(visual_level(0.5), 1.0);
        assert_eq!(visual_level(1.0), 1.0);
    }
}
