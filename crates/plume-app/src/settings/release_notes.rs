use std::ops::Range;

use gpui::{
    div, prelude::*, px, FontStyle, FontWeight, HighlightStyle, InteractiveText, SharedString,
    StyledText,
};
use plume_ui::Tokens;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

const INSTALLED_NOTES: &str = include_str!(concat!(
    "../../../../releases/v",
    env!("CARGO_PKG_VERSION"),
    ".md"
));

#[derive(Clone)]
pub(super) struct NotesContent {
    version: String,
    markdown: String,
    update: bool,
}

impl NotesContent {
    pub(super) fn installed() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").into(),
            markdown: INSTALLED_NOTES.into(),
            update: false,
        }
    }

    pub(super) fn update(version: &str, markdown: &str) -> Self {
        Self {
            version: version.into(),
            markdown: markdown.into(),
            update: true,
        }
    }

    #[cfg(test)]
    pub(super) fn link_label(version: &str, update: bool) -> String {
        if update {
            format!("What's new in version {version}")
        } else {
            format!("Installed release notes — {version}")
        }
    }

    fn title(&self) -> String {
        if self.update {
            format!("Available update — {}", self.version)
        } else {
            format!("Installed version — {}", self.version)
        }
    }
}

pub(super) fn inline_notes(content: &NotesContent, tokens: &Tokens) -> gpui::AnyElement {
    let blocks = parse(&content.markdown);
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .mt(px(4.))
        .pt(px(20.))
        .border_t_1()
        .border_color(tokens.hairline)
        .flex()
        .flex_col()
        .gap(px(12.))
        .text_color(tokens.text)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(content.title()),
        )
        .when(blocks.is_empty(), |body| {
            body.child(
                div()
                    .text_xs()
                    .text_color(tokens.muted)
                    .child("No release notes were provided for this version."),
            )
        })
        .children(
            blocks
                .iter()
                .enumerate()
                .map(|(index, block)| block.render(index, tokens)),
        )
        .into_any_element()
}

#[derive(Default, Clone, Copy)]
struct InlineStyle {
    strong: bool,
    emphasis: bool,
    code: bool,
}

#[derive(Default)]
enum Kind {
    #[default]
    Paragraph,
    Heading(u8),
    Code,
    Rule,
}

#[derive(Default)]
struct Block {
    kind: Kind,
    text: String,
    spans: Vec<(Range<usize>, InlineStyle)>,
    links: Vec<(Range<usize>, String)>,
    prefix: Option<String>,
    depth: usize,
    quote: bool,
}

impl Block {
    fn append(&mut self, text: &str, style: InlineStyle, link: Option<&str>) {
        let start = self.text.len();
        self.text.push_str(text);
        let range = start..self.text.len();
        self.spans.push((range.clone(), style));
        if let Some(url) =
            link.filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        {
            self.links.push((range, url.into()));
        }
    }

    fn render(&self, index: usize, tokens: &Tokens) -> gpui::AnyElement {
        if matches!(self.kind, Kind::Rule) {
            return div().h(px(1.)).bg(tokens.hairline).into_any_element();
        }
        let highlights = self.spans.iter().map(|(range, style)| {
            (
                range.clone(),
                HighlightStyle {
                    color: self
                        .links
                        .iter()
                        .any(|(link, _)| link == range)
                        .then_some(tokens.accent.into()),
                    font_weight: style.strong.then_some(FontWeight::SEMIBOLD),
                    font_style: style.emphasis.then_some(FontStyle::Italic),
                    background_color: style.code.then_some(tokens.fill.into()),
                    ..Default::default()
                },
            )
        });
        let links: Vec<_> = self.links.iter().map(|(_, url)| url.clone()).collect();
        let text = InteractiveText::new(
            SharedString::from(format!("release-note-{index}")),
            StyledText::new(self.text.clone()).with_highlights(highlights),
        )
        .on_click(
            self.links.iter().map(|(range, _)| range.clone()).collect(),
            move |index, _, cx| {
                cx.open_url(&links[index]);
            },
        );
        let content = div()
            .w(px(0.))
            .flex_grow()
            .min_w_0()
            .text_sm()
            .when(matches!(self.kind, Kind::Heading(_)), |el| {
                el.font_weight(FontWeight::SEMIBOLD).pt(px(6.))
            })
            .when(matches!(self.kind, Kind::Heading(1 | 2)), |el| {
                el.text_base()
            })
            .when(matches!(self.kind, Kind::Code), |el| {
                el.font_family("monospace")
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(tokens.fill)
            })
            .child(text);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .gap(px(8.))
            .pl(px(self.depth as f32 * 14.))
            .when(self.quote, |el| {
                el.border_l_2().border_color(tokens.hairline).pl(px(12.))
            })
            .children(self.prefix.as_ref().map(|prefix| {
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(tokens.muted)
                    .child(prefix.clone())
            }))
            .child(content)
            .into_any_element()
    }
}

fn flush(block: &mut Block, blocks: &mut Vec<Block>) {
    if !block.text.is_empty() || matches!(block.kind, Kind::Rule) {
        blocks.push(std::mem::take(block));
    }
}

fn parse(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut strong = false;
    let mut emphasis = false;
    let mut quote_depth = 0usize;
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut items: Vec<Option<String>> = Vec::new();
    let mut link: Option<String> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Paragraph) => {
                flush(&mut block, &mut blocks);
                block.kind = Kind::Paragraph;
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut block, &mut blocks);
                block.kind = Kind::Heading(level as u8);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut block, &mut blocks);
                block.kind = Kind::Code;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut block, &mut blocks);
                quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut block, &mut blocks);
                quote_depth = quote_depth.saturating_sub(1);
            }
            Event::Start(Tag::List(start)) => {
                flush(&mut block, &mut blocks);
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                flush(&mut block, &mut blocks);
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut block, &mut blocks);
                let prefix = match lists.last_mut() {
                    Some(Some(number)) => {
                        let prefix = format!("{number}.");
                        *number += 1;
                        prefix
                    }
                    _ => "•".into(),
                };
                items.push(Some(prefix));
            }
            Event::End(TagEnd::Item) => {
                flush(&mut block, &mut blocks);
                items.pop();
            }
            Event::Start(Tag::Strong) => strong = true,
            Event::End(TagEnd::Strong) => strong = false,
            Event::Start(Tag::Emphasis) => emphasis = true,
            Event::End(TagEnd::Emphasis) => emphasis = false,
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.into_string()),
            Event::End(TagEnd::Link) => link = None,
            Event::Text(ref text) | Event::Code(ref text) => {
                if block.text.is_empty() {
                    block.prefix = items.last_mut().and_then(Option::take);
                    block.depth = lists.len().saturating_sub(1);
                    block.quote = quote_depth > 0;
                }
                block.append(
                    text,
                    InlineStyle {
                        strong,
                        emphasis,
                        code: matches!(event, Event::Code(_)),
                    },
                    link.as_deref(),
                );
            }
            Event::SoftBreak | Event::HardBreak => block.append("\n", InlineStyle::default(), None),
            Event::Rule => {
                flush(&mut block, &mut blocks);
                block.kind = Kind::Rule;
                flush(&mut block, &mut blocks);
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock) => {
                flush(&mut block, &mut blocks)
            }
            _ => {}
        }
    }
    flush(&mut block, &mut blocks);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_and_available_notes_have_unambiguous_labels() {
        let installed = NotesContent::installed();
        let update = NotesContent::update("0.2.0", "# Changes");
        assert!(
            NotesContent::link_label(&installed.version, installed.update)
                .contains("Installed release")
        );
        assert_eq!(
            NotesContent::link_label(&update.version, update.update),
            "What's new in version 0.2.0"
        );
        assert_eq!(update.title(), "Available update — 0.2.0");
        assert!(!installed.markdown.is_empty());
    }

    #[test]
    fn markdown_preserves_lists_formatting_unicode_and_safe_links() {
        let blocks = parse("## Changes\n\n- **Été** and `code`\n- [Details](https://example.com)\n- [Unsafe](javascript:alert(1))\n");
        assert!(matches!(blocks[0].kind, Kind::Heading(2)));
        assert_eq!(blocks[1].text, "Été and code");
        assert_eq!(blocks[1].prefix.as_deref(), Some("•"));
        assert!(blocks[1].spans.iter().any(|(_, style)| style.strong));
        assert!(blocks[1].spans.iter().any(|(_, style)| style.code));
        assert_eq!(blocks[2].links.len(), 1);
        assert!(blocks[3].links.is_empty());
        for block in &blocks {
            for (range, _) in &block.spans {
                assert!(
                    block.text.is_char_boundary(range.start)
                        && block.text.is_char_boundary(range.end)
                );
            }
        }
    }

    #[test]
    fn empty_notes_and_nested_ordered_lists_are_supported() {
        assert!(parse("\n\n").is_empty());
        let blocks = parse("1. First\n   - Nested\n2. Second\n");
        assert_eq!(
            blocks
                .iter()
                .map(|block| block.prefix.as_deref())
                .collect::<Vec<_>>(),
            [Some("1."), Some("•"), Some("2.")]
        );
        assert_eq!(blocks[1].depth, 1);
    }
}
