use std::rc::Rc;

use gpui::{
    div, prelude::*, px, svg, AnyElement, App, ClickEvent, Context, ElementId, FontWeight,
    IntoElement, KeyDownEvent, RenderOnce, SharedString, Window,
};

use crate::tokens::{Tokens, FONT_MONO};

type Handler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

fn bind<V: 'static>(
    cx: &mut Context<V>,
    handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Handler {
    let listener = cx.listener(move |view, _: &(), window, cx| handler(view, window, cx));
    Rc::new(move |window, cx| listener(&(), window, cx))
}

/// Run `handler` on click, and on Enter or Space once the element has keyboard focus.
fn activate<E: InteractiveElement + StatefulInteractiveElement>(el: E, handler: Handler) -> E {
    let click = handler.clone();
    el.tab_index(0)
        .on_click(move |_: &ClickEvent, window, cx| click(window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                cx.stop_propagation();
                window.prevent_default();
                handler(window, cx);
            }
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonVariant {
    /// Ink pill: the one action that moves the user forward.
    Primary,
    /// Outlined pill on a surface.
    Secondary,
    /// Text only until hovered.
    Ghost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonSize {
    Large,
    Medium,
    Small,
}

/// The website's pill button.
#[derive(IntoElement)]
pub struct Button {
    tokens: Tokens,
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    enabled: bool,
    icon: Option<&'static str>,
    trailing_icon: Option<&'static str>,
    on_click: Option<Handler>,
}

impl Button {
    pub fn new(tokens: Tokens, id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Button {
            tokens,
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Medium,
            enabled: true,
            icon: None,
            trailing_icon: None,
            on_click: None,
        }
    }

    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }

    pub fn ghost(mut self) -> Self {
        self.variant = ButtonVariant::Ghost;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    pub fn trailing_icon(mut self, path: &'static str) -> Self {
        self.trailing_icon = Some(path);
        self
    }

    pub fn on_click<V: 'static>(
        mut self,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_click = Some(bind(cx, handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let tokens = self.tokens;
        let (height, pad, text, icon) = match self.size {
            ButtonSize::Large => (40., 18., 14., 15.),
            ButtonSize::Medium => (34., 14., 13., 14.),
            ButtonSize::Small => (28., 11., 12., 13.),
        };
        let fg = match self.variant {
            ButtonVariant::Primary => tokens.btn_fg,
            ButtonVariant::Secondary => tokens.text,
            ButtonVariant::Ghost => tokens.muted,
        };
        let icon_el = |path: &'static str| svg().path(path).size(px(icon)).text_color(fg);
        let el = div()
            .id(self.id)
            .flex_shrink_0()
            .h(px(height))
            .pl(px(if self.icon.is_some() { pad - 3. } else { pad }))
            .pr(px(if self.trailing_icon.is_some() {
                pad - 3.
            } else {
                pad
            }))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(7.))
            .rounded_full()
            .whitespace_nowrap()
            .text_size(px(text))
            .text_color(fg)
            .map(|el| match self.variant {
                ButtonVariant::Primary => el
                    .bg(tokens.btn_bg)
                    .font_weight(FontWeight::MEDIUM)
                    .hover(|style| style.opacity(0.86)),
                ButtonVariant::Secondary => el
                    .border_1()
                    .border_color(tokens.border_strong)
                    .bg(tokens.surface)
                    .hover(move |style| style.border_color(tokens.faint)),
                ButtonVariant::Ghost => el.hover(move |style| style.bg(tokens.hover)),
            })
            .children(self.icon.map(icon_el))
            .child(self.label)
            .children(self.trailing_icon.map(icon_el));
        match self.on_click.filter(|_| self.enabled) {
            Some(handler) => activate(el.cursor_pointer(), handler),
            None => el.when(!self.enabled, |el| el.opacity(0.45)),
        }
    }
}

/// A round icon-only button.
#[derive(IntoElement)]
pub struct IconButton {
    tokens: Tokens,
    id: ElementId,
    icon: &'static str,
    size: f32,
    on_click: Option<Handler>,
}

impl IconButton {
    pub fn new(tokens: Tokens, id: impl Into<ElementId>, icon: &'static str) -> Self {
        IconButton {
            tokens,
            id: id.into(),
            icon,
            size: 30.,
            on_click: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn on_click<V: 'static>(
        mut self,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_click = Some(bind(cx, handler));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let tokens = self.tokens;
        let el = div()
            .id(self.id)
            .flex_shrink_0()
            .size(px(self.size))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .hover(move |style| style.bg(tokens.hover))
            .child(
                svg()
                    .path(self.icon)
                    .size(px((self.size / 2.).round()))
                    .text_color(tokens.subtle),
            );
        match self.on_click {
            Some(handler) => activate(el.cursor_pointer(), handler),
            None => el,
        }
    }
}

pub struct Segment<T> {
    value: T,
    id: ElementId,
    label: SharedString,
    on_click: Option<Handler>,
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
        self.on_click = Some(bind(cx, move |view, _, cx| handler(view, cx)));
        self
    }
}

/// Pill track holding a raised chip for the selected value.
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
        div()
            .flex_shrink_0()
            .flex()
            .p(px(3.))
            .rounded_full()
            .border_1()
            .border_color(tokens.border)
            .bg(tokens.surface_2)
            .children(self.segments.into_iter().map(move |segment| {
                let active = segment.value == selected;
                let el = div()
                    .id(segment.id)
                    .h(px(28.))
                    .px(px(13.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .when_else(
                        active,
                        |el| {
                            el.bg(tokens.surface)
                                .shadow(tokens.raise())
                                .text_color(tokens.text)
                                .font_weight(FontWeight::MEDIUM)
                        },
                        |el| {
                            el.text_color(tokens.muted)
                                .hover(move |style| style.text_color(tokens.text))
                        },
                    )
                    .child(segment.label);
                match segment.on_click.filter(|_| !active) {
                    Some(handler) => activate(el.cursor_pointer(), handler).into_any_element(),
                    None => el.into_any_element(),
                }
            }))
    }
}

/// On/off switch.
#[derive(IntoElement)]
pub struct Switch {
    tokens: Tokens,
    id: ElementId,
    on: bool,
    on_click: Option<Handler>,
}

impl Switch {
    pub fn new(tokens: Tokens, id: impl Into<ElementId>, on: bool) -> Self {
        Switch {
            tokens,
            id: id.into(),
            on,
            on_click: None,
        }
    }

    pub fn on_click<V: 'static>(
        mut self,
        cx: &mut Context<V>,
        handler: impl Fn(&mut V, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_click = Some(bind(cx, move |view, _, cx| handler(view, cx)));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let tokens = self.tokens;
        let el = div()
            .id(self.id)
            .flex_shrink_0()
            .w(px(38.))
            .h(px(22.))
            .p(px(3.))
            .flex()
            .when(self.on, |el| el.justify_end())
            .rounded_full()
            .bg(if self.on {
                tokens.accent
            } else {
                tokens.switch_off
            })
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .bg(gpui::white())
                    .shadow_sm(),
            );
        match self.on_click {
            Some(handler) => activate(el.cursor_pointer(), handler),
            None => el,
        }
    }
}

/// How large a keycap is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeySize {
    /// Inline in a settings row.
    Small,
    /// The onboarding's shortcut cards.
    Large,
}

/// A shortcut such as "Ctrl+Space" drawn as physical keys.
pub fn keycaps(tokens: &Tokens, shortcut: &str, size: KeySize) -> AnyElement {
    let (text, height, pad, wide_pad, radius) = match size {
        KeySize::Small => (12., 26., 8., 18., 7.),
        KeySize::Large => (14., 40., 13., 30., 10.),
    };
    let tokens = *tokens;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(if size == KeySize::Small { 5. } else { 6. }))
        .children(shortcut.split('+').map(|key| {
            let key = key.trim();
            let label = key_label(key);
            let wide = matches!(key, "Space" | "Espace");
            div()
                .h(px(height))
                .px(px(if wide { wide_pad } else { pad }))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius))
                .border_1()
                .border_b_2()
                .border_color(tokens.border_strong)
                .bg(tokens.surface_2)
                .text_size(px(text))
                .text_color(tokens.text)
                .whitespace_nowrap()
                .child(SharedString::from(label))
        }))
        .into_any_element()
}

/// Platform glyphs on macOS, names elsewhere.
pub fn key_label(key: &str) -> String {
    match key {
        "Super" if cfg!(target_os = "macos") => "⌘",
        "Ctrl" if cfg!(target_os = "macos") => "⌃",
        "Alt" if cfg!(target_os = "macos") => "⌥",
        "Shift" if cfg!(target_os = "macos") => "⇧",
        key => key,
    }
    .to_string()
}

/// Mono capitals: the website's eyebrow and section labels.
pub fn eyebrow(text: impl Into<SharedString>, color: gpui::Rgba) -> gpui::Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(11.))
        .text_color(color)
        .child(text.into().to_uppercase())
}
