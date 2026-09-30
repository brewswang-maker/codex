//! Renders parsed markdown content, with syntax-highlighted code blocks
//! (the `highlighter` feature drives them inside `markdown::Content`).
//!
//! A custom [`markdown::Viewer`] overlays a copy button on every code
//! block; the button shares the transcript-wide copy feedback keys, so it
//! flips to a check for the same window as the message actions. Body text
//! renders through [`SelectableText`], so the pointer can drag a partial
//! selection across the transcript.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::selectable::SelectableText;
use crate::selectable::split_lines;
use crate::state::TextSelection;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::Font;
use iced::Pixels;
use iced::Theme;
use iced::advanced::text::Span;
use iced::alignment;
use iced::padding;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::markdown;
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

/// Turns parsed markdown content into a rendered element.
///
/// `message_id` namespaces the code-block copy keys and the selection
/// keys of every text line; `copied_key` is the feedback key currently
/// showing its check mark; `selection` is the transcript's active drag
/// selection, whose anchor/focus points each line resolves into its own
/// highlight range.
pub fn render<'a>(
    content: &'a markdown::Content,
    message_id: &'a str,
    copied_key: Option<&'a str>,
    selection: Option<&'a TextSelection>,
) -> Element<'a, Message> {
    let viewer = CopyableViewer {
        message_id,
        copied_key,
        selection,
        namespace: format!("md:{message_id}"),
        block: Cell::new(0),
    };

    markdown::view_with(content.items(), &Theme::Light, &viewer)
}

/// The [`markdown::Viewer`] behind [`render`]: the default look, except
/// each code block gains a copy button in its top-right corner and body
/// text becomes drag-selectable.
struct CopyableViewer<'a> {
    message_id: &'a str,
    copied_key: Option<&'a str>,
    /// The transcript's active drag selection; each line of this message
    /// resolves its highlight range from it.
    selection: Option<&'a TextSelection>,
    /// The selection namespace of this message (`md:{message_id}`),
    /// shared by the selection keys the lines publish.
    namespace: String,
    /// The index of the block (paragraph or heading) being rendered;
    /// blocks advance in render order so their keys stay stable across
    /// frames.
    block: Cell<usize>,
}

impl<'a> CopyableViewer<'a> {
    /// The key of one selectable line: stable per message, block, line.
    fn line_key(&self, block: usize, line: usize) -> String {
        format!("md:{}:{block}:{line}", self.message_id)
    }

    /// Starts a new block and returns its index.
    fn next_block(&self) -> usize {
        let block = self.block.get();
        self.block.set(block + 1);
        block
    }

    /// Whether the live selection is owned by this message; its lines
    /// then report the focus while the pointer sweeps over them.
    fn selecting(&self) -> bool {
        self.selection.and_then(TextSelection::message) == Some(self.namespace.as_str())
    }

    /// The epoch covered lines report their plain text under.
    fn epoch(&self) -> u64 {
        self.selection.map_or(0, |selection| selection.epoch)
    }

    /// One selectable line wired for the cross-line selection protocol:
    /// it paints the range the active selection assigns to it, reports
    /// the focus while the pointer is over it, and reports its plain
    /// text once per selection epoch.
    fn line_widget(
        &self,
        key: String,
        spans: impl Into<Arc<[Span<'static, String, Font>]>>,
        size: Pixels,
        font: Option<Font>,
    ) -> SelectableText {
        let spans: Arc<[Span<'static, String, Font>]> = spans.into();
        let content_len = spans.iter().map(|span| span.text.len()).sum();
        let selection = self
            .selection
            .and_then(|selection| selection.range_for(&key, content_len));
        // A fn pointer keeps the callback free of the view's lifetime.
        let on_link_click: fn(String) -> Message =
            <Self as markdown::Viewer<'a, Message>>::on_link_click;

        let widget = SelectableText::new(key, spans)
            .size(size)
            .selection(selection)
            .epoch(self.epoch())
            .selection_active(self.selecting())
            .on_link_click(on_link_click)
            .on_select(Message::from);

        match font {
            Some(font) => widget.font(font),
            None => widget,
        }
    }

    /// One body block (a paragraph or a heading) as selectable text: a
    /// single block for single-line content, otherwise a zero-spaced
    /// column of per-line blocks — hit-test offsets are line-relative,
    /// so per-line blocks keep them content-relative.
    fn selectable_lines(
        &self,
        settings: markdown::Settings,
        body_text: &markdown::Text,
        block: usize,
        size: Pixels,
    ) -> Element<'a, Message> {
        let spans = body_text.spans(settings.style);

        let Some(lines) = split_lines(&spans) else {
            return self
                .line_widget(self.line_key(block, 0), spans, size, None)
                .into();
        };

        let mut body = column![].spacing(0);
        for (line, line_spans) in lines.into_iter().enumerate() {
            if line_spans.is_empty() {
                // An empty line still holds its height; a space keeps the
                // column's texture without adding anything to select.
                body = body.push(text(" ").size(size));
                continue;
            }

            body = body.push(self.line_widget(self.line_key(block, line), line_spans, size, None));
        }

        body.into()
    }
}

impl<'a> markdown::Viewer<'a, Message> for CopyableViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn heading(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        let block = self.next_block();
        let size = match level {
            markdown::HeadingLevel::H1 => settings.h1_size,
            markdown::HeadingLevel::H2 => settings.h2_size,
            markdown::HeadingLevel::H3 => settings.h3_size,
            markdown::HeadingLevel::H4 => settings.h4_size,
            markdown::HeadingLevel::H5 => settings.h5_size,
            markdown::HeadingLevel::H6 => settings.h6_size,
        };

        // Mirrors the stock heading look: the text, plus top padding
        // after the opening block.
        container(self.selectable_lines(settings, text, block, size))
            .padding(padding::top(if index > 0 {
                settings.text_size / 2.0
            } else {
                Pixels::ZERO
            }))
            .into()
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        let block = self.next_block();

        self.selectable_lines(settings, text, block, settings.text_size)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let block = self.next_block();

        // Mirrors the stock code-block look (themed background, padding,
        // horizontal scrolling, code font), except every line renders as
        // a selectable block so the pointer can drag a partial selection
        // across code as well.
        let mut body = column![].spacing(0);
        for (line, line_text) in lines.iter().enumerate() {
            let spans = line_text.spans(settings.style);
            if spans.is_empty() {
                body = body.push(text(" ").size(settings.code_size));
                continue;
            }

            body = body.push(self.line_widget(
                self.line_key(block, line),
                spans,
                settings.code_size,
                Some(settings.style.code_block_font),
            ));
        }
        let body = container(
            scrollable(container(body).padding(settings.code_size)).direction(
                scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default()
                        .width(settings.code_size / 2)
                        .scroller_width(settings.code_size / 2),
                ),
            ),
        )
        .width(Fill)
        .padding(settings.code_size / 4)
        .class(<Theme as markdown::Catalog>::code_block());

        let key = code_copy_key(self.message_id, code);
        let copied = self.copied_key == Some(key.as_str());
        let icon = if copied {
            IconKind::Check
        } else {
            IconKind::Copy
        };
        let color = if copied { theme::GLOW } else { theme::FAINT };
        let copy = button(Icon::new(icon, color, 14.0).widget())
            .padding(4)
            .style(theme::code_copy_button)
            .on_press(Message::CopyMessage {
                key,
                text: code.trim_end_matches('\n').to_string(),
            });

        stack![
            body,
            container(copy)
                .width(Fill)
                .height(Fill)
                .align_x(alignment::Horizontal::Right)
                .align_y(alignment::Vertical::Top)
                .padding(iced::Padding::default().top(6).right(10)),
        ]
        .into()
    }
}

/// The copy feedback key of one code block: message id plus a content
/// hash, so re-renders of the same block keep a stable key.
fn code_copy_key(message_id: &str, code: &str) -> String {
    let mut hasher = DefaultHasher::new();
    message_id.hash(&mut hasher);
    code.hash(&mut hasher);
    format!("code:{message_id}:{:x}", hasher.finish())
}

#[cfg(test)]
#[path = "markdown_view_tests.rs"]
mod tests;
