//! A selectable flavor of the built-in rich-text widget.
//!
//! Mirrors `iced::widget::rich_text`'s layout and draw paths — span
//! highlights, underlines, strikethroughs, and link hit-testing — and adds
//! pointer-drag selection over one line of text.
//!
//! The widget shape is one logical line per block on purpose:
//! `Paragraph::hit_test` reports byte offsets relative to the logical line
//! the point falls on, so a single-line block makes each reported offset a
//! direct byte index into the block's plain text. [`split_lines`] prepares
//! multi-line content for that shape. The selected slice is baked into the
//! spans as a background highlight, so the shared span-drawing path paints
//! it with no custom geometry.
//!
//! Selections that span lines coordinate through [`SelectionEvent`]s: the
//! dragged line publishes the anchor and every line under the pointer
//! publishes the focus; the app stores the two points, hands each line its
//! highlight range back, and joins the line texts that covered blocks
//! report once per epoch into the clipboard payload — see
//! [`TextSelection`](crate::state::TextSelection).

use crate::message::Message;
use crate::state::SelectionPoint;
use crate::theme;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::Renderer as _;
use iced::advanced::text::{self, Paragraph, Span, Text};
use iced::advanced::widget::text as widget_text;
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size,
    Vector, alignment, mouse,
};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

/// The GUI's concrete renderer.
type Renderer = iced::Renderer;

/// A shaped paragraph of [`Renderer`].
type Shaped = <Renderer as text::Renderer>::Paragraph;

/// The pointer travel, in logical pixels, that turns a press into a drag.
const DRAG_THRESHOLD: f32 = 3.0;

/// How far left of a line's text a press still lands on the line: the
/// band covers the leading markers markdown indents its blocks behind
/// (list bullets, task checkboxes, ordered numbers, quote bars, code
/// padding), so a press that starts on a marker begins the selection at
/// the line's first character.
const PRESS_LEAD: f32 = 32.0;

/// A rich-text block the pointer can drag a selection across.
///
/// Build one per logical line (see [`split_lines`]); the block identity
/// (`key`) travels with every selection update, and the app stores the
/// anchor/focus points across lines, computes each line's highlight, and
/// joins the reported line texts into the clipboard payload.
pub struct SelectableText {
    /// Stable identity carried into every published selection.
    key: String,
    /// The styled spans, already `'static` (the markdown cache shares
    /// them across frames).
    spans: Arc<[Span<'static, String, Font>]>,
    /// The block's plain text, joined from `spans`; selection offsets and
    /// the clipboard payload index into this.
    content: String,
    /// The default text size, for spans without their own.
    size: Option<Pixels>,
    /// The default text color, for spans without their own.
    color: Option<Color>,
    /// The default font, for spans without their own.
    font: Option<Font>,
    /// The default line height.
    line_height: text::LineHeight,
    /// The width contracted by the block (shrink by default).
    width: Length,
    /// The height contracted by the block (shrink by default).
    height: Length,
    /// The wrapping strategy of the block.
    wrapping: text::Wrapping,
    /// The selection this block should display, as a byte range.
    selection: Option<Range<usize>>,
    /// The epoch of the live selection this block belongs to; a covered
    /// block reports its plain text once per epoch.
    epoch: u64,
    /// Whether the live selection owns this block's message; the block
    /// then reports the focus while the pointer sweeps over it.
    selection_active: bool,
    /// The span index under the pointer, when the pointer hovers a link.
    hovered_link: Option<usize>,
    /// Produces the message for a link activation.
    on_link_click: Option<Box<dyn Fn(String) -> Message>>,
    /// Produces the selection protocol updates while the pointer drags.
    on_select: Option<Box<dyn Fn(SelectionEvent) -> Message>>,
}

impl SelectableText {
    /// Creates a block from already-styled spans.
    pub fn new(
        key: impl Into<String>,
        spans: impl Into<Arc<[Span<'static, String, Font>]>>,
    ) -> Self {
        let spans = spans.into();
        let content = spans.iter().map(|span| span.text.as_ref()).collect();

        Self {
            key: key.into(),
            spans,
            content,
            size: None,
            color: None,
            font: None,
            line_height: text::LineHeight::default(),
            width: Length::Shrink,
            height: Length::Shrink,
            wrapping: text::Wrapping::default(),
            selection: None,
            epoch: 0,
            selection_active: false,
            hovered_link: None,
            on_link_click: None,
            on_select: None,
        }
    }

    /// Creates a single-span block from plain text.
    pub fn plain(key: impl Into<String>, text: impl Into<String>) -> Self {
        let spans: Arc<[Span<'static, String, Font>]> = Arc::from(vec![Span::new(text.into())]);

        Self::new(key, spans)
    }

    /// Sets the default [`Pixels`] size of the block.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the default [`Color`] of the block.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Sets the default [`Font`] of the block.
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the selection the block should display, as a byte range.
    pub fn selection(mut self, selection: Option<Range<usize>>) -> Self {
        self.selection = selection;
        self
    }

    /// Sets the epoch of the live selection this block belongs to.
    pub fn epoch(mut self, epoch: u64) -> Self {
        self.epoch = epoch;
        self
    }

    /// Marks the block as part of the message that owns the live
    /// selection, so it reports the focus while the pointer is over it.
    pub fn selection_active(mut self, selection_active: bool) -> Self {
        self.selection_active = selection_active;
        self
    }

    /// Sets the message produced when one of the block's links is
    /// activated.
    pub fn on_link_click(mut self, on_link_click: impl Fn(String) -> Message + 'static) -> Self {
        self.on_link_click = Some(Box::new(on_link_click));
        self
    }

    /// Sets the message produced while the pointer drags a selection.
    pub fn on_select(mut self, on_select: impl Fn(SelectionEvent) -> Message + 'static) -> Self {
        self.on_select = Some(Box::new(on_select));
        self
    }

    /// The [`Text`] definition of one shaped pass over `spans`.
    fn text_with<'a>(
        &self,
        spans: &'a [Span<'static, String, Font>],
        bounds: Size,
        size: Pixels,
        font: Font,
    ) -> Text<&'a [Span<'static, String, Font>], Font> {
        Text {
            content: spans,
            bounds,
            size,
            line_height: self.line_height,
            font,
            align_x: text::Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: self.wrapping,
        }
    }
}

/// The block's persistent state: the spans last shaped, the selection
/// baked into them, and the in-flight drag.
#[derive(Default)]
struct State {
    /// The widget's input spans as last seen, kept static so later frames
    /// compare against them without reshapes.
    base: Vec<Span<'static, String, Font>>,
    /// The spans actually shaped into `paragraph` (the base with the baked
    /// selection applied), kept for draw-time span attributes.
    spans: Vec<Span<'static, String, Font>>,
    /// The selection baked into `spans`, if any.
    baked: Option<Range<usize>>,
    /// The shaped paragraph.
    paragraph: Shaped,
    /// The drag currently owning the block, if the press started here.
    drag: Option<Drag>,
    /// The selection epoch whose line text this block already reported.
    reported_epoch: Option<u64>,
    /// The span index the press landed on, so a press that never turns
    /// into a drag can still activate a link.
    span_pressed: Option<usize>,
}

/// One pointer drag inside a block, in content byte offsets.
struct Drag {
    /// The offset where the press landed.
    anchor: usize,
    /// The offset under the pointer right now.
    focus: usize,
    /// The press position in block-local coordinates, for the
    /// click-versus-drag threshold.
    origin: Point,
    /// Whether the pointer has moved past [`DRAG_THRESHOLD`].
    moved: bool,
    /// Whether the drag already published its anchor as a fresh selection.
    started: bool,
}

impl Widget<Message, iced::Theme, Renderer> for SelectableText {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();

        layout::sized(limits, self.width, self.height, |limits| {
            let bounds = limits.max();
            let size = self.size.unwrap_or_else(|| renderer.default_size());
            let font = self.font.unwrap_or_else(|| renderer.default_font());
            let wanted = clamp_selection(self.selection.as_ref(), &self.content);
            let content_changed = state.base != self.spans.as_ref();

            if content_changed || state.baked != wanted {
                if content_changed {
                    state.base = self.spans.iter().cloned().map(Span::to_static).collect();
                }
                state.spans = match &wanted {
                    Some(range) => bake_selection(&state.base, range, theme::SELECTION),
                    None => state.base.clone(),
                };
                state.baked = wanted;

                let text = self.text_with(&state.spans, bounds, size, font);
                state.paragraph = Shaped::with_spans(text);
            } else {
                let difference = state.paragraph.compare(Text {
                    content: (),
                    bounds,
                    size,
                    line_height: self.line_height,
                    font,
                    align_x: text::Alignment::Default,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: self.wrapping,
                });

                match difference {
                    text::Difference::None => {}
                    text::Difference::Bounds => state.paragraph.resize(bounds),
                    text::Difference::Shape => {
                        let text = self.text_with(&state.spans, bounds, size, font);
                        state.paragraph = Shaped::with_spans(text);
                    }
                }
            }

            state.paragraph.min_bounds()
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &iced::Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let translation = layout.position() - Point::ORIGIN;

        for (index, span) in state.spans.iter().enumerate() {
            let is_hovered_link = self.on_link_click.is_some() && Some(index) == self.hovered_link;

            if span.highlight.is_none()
                && !span.underline
                && !span.strikethrough
                && !is_hovered_link
            {
                continue;
            }

            let regions = state.paragraph.span_bounds(index);

            if let Some(highlight) = span.highlight {
                for bounds in &regions {
                    let bounds = Rectangle::new(
                        bounds.position() - Vector::new(span.padding.left, span.padding.top),
                        bounds.size() + Size::new(span.padding.x(), span.padding.y()),
                    );

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: bounds + translation,
                            border: highlight.border,
                            ..Default::default()
                        },
                        highlight.background,
                    );
                }
            }

            if span.underline || span.strikethrough || is_hovered_link {
                let size = span
                    .size
                    .or(self.size)
                    .unwrap_or_else(|| renderer.default_size());

                let line_height = span
                    .line_height
                    .unwrap_or(self.line_height)
                    .to_absolute(size);

                let color = span.color.or(self.color).unwrap_or(defaults.text_color);

                let baseline =
                    translation + Vector::new(0.0, size.0 + (line_height.0 - size.0) / 2.0);

                if span.underline || is_hovered_link {
                    for bounds in &regions {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    bounds.position() + baseline - Vector::new(0.0, size.0 * 0.08),
                                    Size::new(bounds.width, 1.0),
                                ),
                                ..Default::default()
                            },
                            color,
                        );
                    }
                }

                if span.strikethrough {
                    for bounds in &regions {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    bounds.position() + baseline - Vector::new(0.0, size.0 / 2.0),
                                    Size::new(bounds.width, 1.0),
                                ),
                                ..Default::default()
                            },
                            color,
                        );
                    }
                }
            }
        }

        widget_text::draw(
            renderer,
            defaults,
            layout.bounds(),
            &state.paragraph,
            widget_text::Style { color: self.color },
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        // Lines covered by the live selection report their plain text once
        // per selection epoch, so Ctrl+C can join the pieces without
        // re-parsing the markdown source.
        if self.selection.is_some() {
            if state.reported_epoch != Some(self.epoch)
                && let Some(on_select) = &self.on_select
            {
                state.reported_epoch = Some(self.epoch);
                shell.publish(on_select(SelectionEvent::Line {
                    key: self.key.clone(),
                    text: self.content.clone(),
                }));
            }
        } else {
            state.reported_epoch = None;
        }

        // Link hover bookkeeping, suppressed while a drag runs so the
        // underline does not flicker across a selection sweep.
        if self.on_link_click.is_some() {
            let was_hovered = self.hovered_link.is_some();

            self.hovered_link = if state.drag.is_some() {
                None
            } else {
                cursor.position_in(bounds).and_then(|position| {
                    state
                        .paragraph
                        .hit_span(position)
                        .and_then(|span| state.spans.get(span)?.link.as_ref().map(|_| span))
                })
            };

            if was_hovered != self.hovered_link.is_some() {
                shell.request_redraw();
            }
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(on_select) = &self.on_select
                    && let Some(position) = cursor.position()
                    && let Some(local) = row_position(bounds, position)
                {
                    let offset = hit_offset(&state.paragraph, local, self.content.len());
                    // The raw press position (not the clamp to the text)
                    // drives the drag threshold, so a press on a leading
                    // marker starts the selection as soon as the pointer
                    // actually travels.
                    let origin = Point::new(position.x - bounds.x, position.y - bounds.y);
                    tracing::debug!(key = %self.key, offset, pos = ?origin, "select: press");

                    state.drag = Some(Drag {
                        anchor: offset,
                        focus: offset,
                        origin,
                        moved: false,
                        started: false,
                    });
                    // Only a press on the text itself can arm a link; a
                    // press on a leading marker must not fire one.
                    state.span_pressed = if origin.x >= 0.0 {
                        state.paragraph.hit_span(local)
                    } else {
                        None
                    };

                    // The press takes the selection away from any other
                    // line right away, so an old highlight drops on this
                    // click.
                    shell.publish(on_select(SelectionEvent::Cleared));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(on_select) = &self.on_select else {
                    return;
                };
                // The threshold check must survive a pointer that already
                // left this line; the focus itself follows the pointer
                // anywhere on the line's row, including the blank space
                // right of a short line's end.
                //
                // Track the corrected cursor, not the raw event position:
                // since iced 0.14 a scrollable applies its translation at
                // draw time, so `bounds` is in content coordinates while
                // the raw event position stays in window coordinates — the
                // two only agree while the transcript sits unscrolled at
                // the very top, which is why drags died mid-selection in
                // any real conversation.
                let Some(position) = cursor.position() else {
                    return;
                };
                let local = row_position(bounds, position);
                let anywhere = Point::new(position.x - bounds.x, position.y - bounds.y);

                if let Some(drag) = state.drag.as_mut() {
                    tracing::debug!(
                        key = %self.key,
                        pos_x = anywhere.x,
                        pos_y = anywhere.y,
                        bounds_y = bounds.y,
                        bounds_h = bounds.height,
                        hit = local.is_some(),
                        "select: drag move"
                    );
                    if !drag.moved {
                        let dx = anywhere.x - drag.origin.x;
                        let dy = anywhere.y - drag.origin.y;
                        drag.moved = dx * dx + dy * dy > DRAG_THRESHOLD * DRAG_THRESHOLD;
                    }
                    if let Some(local) = local {
                        drag.focus = hit_offset(&state.paragraph, local, self.content.len());
                    }

                    if drag.moved {
                        if !drag.started {
                            drag.started = true;
                            tracing::debug!(key = %self.key, anchor = drag.anchor, "select: started");
                            shell.publish(on_select(SelectionEvent::Started {
                                key: self.key.clone(),
                                offset: drag.anchor,
                            }));
                        }
                        if local.is_some() {
                            tracing::debug!(key = %self.key, focus = drag.focus, "select: focus");
                            shell.publish(on_select(SelectionEvent::Focused {
                                key: self.key.clone(),
                                offset: drag.focus,
                            }));
                        }
                        // Deliberately not captured: a focus sweep must
                        // keep reaching sibling lines, and a stacked block
                        // (code + copy button) stops forwarding once a
                        // capture is seen — capturing here would freeze
                        // every code line outside the anchor's stack.
                    }
                } else if self.selection_active {
                    tracing::debug!(
                        key = %self.key,
                        pos_x = anywhere.x,
                        pos_y = anywhere.y,
                        hit = local.is_some(),
                        "select: sweep"
                    );
                    if let Some(local) = local {
                        // The drag belongs to another line of this message;
                        // the pointer is over this one, so this line becomes
                        // the focus instead.
                        let offset = hit_offset(&state.paragraph, local, self.content.len());
                        tracing::debug!(key = %self.key, offset, "select: focus");
                        shell.publish(on_select(SelectionEvent::Focused {
                            key: self.key.clone(),
                            offset,
                        }));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(drag) = state.drag.take() {
                    tracing::debug!(key = %self.key, anchor = drag.anchor, focus = drag.focus, moved = drag.moved, "select: release");
                    if !drag.moved
                        && let Some(link) = state
                            .span_pressed
                            .and_then(|span| state.spans.get(span)?.link.clone())
                        && let Some(on_link_click) = &self.on_link_click
                    {
                        shell.publish(on_link_click(link));
                    }

                    state.span_pressed = None;
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.hovered_link.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a> From<SelectableText> for Element<'a, Message> {
    fn from(selectable: SelectableText) -> Self {
        Element::new(selectable)
    }
}

/// One step of the pointer-drag selection protocol between a line block
/// and the app that stitches the per-line pieces into one selection.
pub enum SelectionEvent {
    /// The drag passed the click threshold; a fresh selection anchors at
    /// `offset` of `key`.
    Started {
        /// The anchor line.
        key: String,
        /// The anchor byte offset inside that line.
        offset: usize,
    },
    /// The pointer is over `key` at `offset`; moves (or extends) the
    /// selection's focus.
    Focused {
        /// The line under the pointer.
        key: String,
        /// The byte offset inside that line.
        offset: usize,
    },
    /// A press landed, so any previous selection should drop.
    Cleared,
    /// The block is covered by the live selection; carries its full
    /// plain text for the clipboard payload.
    Line {
        /// The line-block key the text belongs to.
        key: String,
        /// The line's full plain text.
        text: String,
    },
}

impl From<SelectionEvent> for Message {
    fn from(event: SelectionEvent) -> Self {
        match event {
            SelectionEvent::Started { key, offset } => {
                Message::TextSelectionStarted(SelectionPoint { key, offset })
            }
            SelectionEvent::Focused { key, offset } => {
                Message::TextSelectionFocused(SelectionPoint { key, offset })
            }
            SelectionEvent::Cleared => Message::TextSelectionCleared,
            SelectionEvent::Line { key, text } => Message::TextSelectionLine { key, text },
        }
    }
}

/// Row-local pointer position that still lands on the row while the
/// pointer sits outside the line's text horizontally. The y band stays
/// strict so a row never claims its neighbors'. x has no right limit: a
/// drag that sweeps past the end of a short line (a code line, a
/// paragraph's last row) keeps focusing that line instead of falling into
/// a dead gutter. To the left it reaches [`PRESS_LEAD`] into the leading
/// marker band, and positions left of the text collapse to the line start
/// so the selection begins at the first character there.
fn row_position(bounds: Rectangle, position: Point) -> Option<Point> {
    let local = Point::new(position.x - bounds.x, position.y - bounds.y);

    (local.y >= 0.0 && local.y < bounds.height && local.x >= -PRESS_LEAD)
        .then_some(Point::new(local.x.max(0.0), local.y))
}

/// The content offset under `point` (block-local coordinates), clamped to
/// the content length; points beyond the shaped text fall back to the
/// nearest edge by direction.
fn hit_offset(paragraph: &Shaped, point: Point, content_len: usize) -> usize {
    match paragraph.hit_test(point).map(text::Hit::cursor) {
        Some(offset) => offset.min(content_len),
        None => {
            if point.y < 0.0 {
                0
            } else {
                content_len
            }
        }
    }
}

/// Snaps an incoming selection to the current content: both bounds are
/// clamped to the content length and to char boundaries. Empty or
/// out-of-range selections collapse to `None`.
fn clamp_selection(selection: Option<&Range<usize>>, content: &str) -> Option<Range<usize>> {
    let selection = selection?;
    let start = floor_char_boundary(content, selection.start);
    let end = ceil_char_boundary(content, selection.end);

    (start < end).then_some(start..end)
}

/// Splits styled spans into one span group per logical line.
///
/// Returns `None` when no span contains a newline, so callers can hand the
/// original (already shared) span list straight to a single block. Empty
/// lines yield empty groups.
pub fn split_lines(
    spans: &[Span<'static, String, Font>],
) -> Option<Vec<Vec<Span<'static, String, Font>>>> {
    if !spans.iter().any(|span| span.text.contains('\n')) {
        return None;
    }

    let mut lines = vec![Vec::new()];

    for span in spans {
        for (index, piece) in span.text.split('\n').enumerate() {
            if index > 0 {
                lines.push(Vec::new());
            }
            if !piece.is_empty() {
                let mut part = span.clone();
                part.text = Cow::Owned(piece.to_string());
                if let Some(line) = lines.last_mut() {
                    line.push(part);
                }
            }
        }
    }

    Some(lines)
}

/// Returns `spans` split at the selection bounds, with the enclosed
/// fragments carrying `background` as their highlight, so the draw path
/// paints the selection behind exactly those glyphs. A range that falls
/// outside the text (or is empty) leaves the spans unchanged. The range
/// must already be clamped to the content.
fn bake_selection(
    spans: &[Span<'static, String, Font>],
    range: &Range<usize>,
    background: Color,
) -> Vec<Span<'static, String, Font>> {
    let mut baked = Vec::with_capacity(spans.len() + 2);
    let mut offset = 0;

    for span in spans {
        let span_len = span.text.len();
        let span_start = offset;
        offset += span_len;

        let lo = range.start.max(span_start);
        let hi = range.end.min(span_start + span_len);
        if lo >= hi {
            baked.push(span.clone());
            continue;
        }

        let lo = floor_char_boundary(&span.text, lo - span_start);
        let hi = ceil_char_boundary(&span.text, hi - span_start);

        if lo > 0 {
            baked.push(with_text(span, &span.text[..lo]));
        }

        let mut selected = with_text(span, &span.text[lo..hi]);
        selected.highlight = Some(text::Highlight {
            background: Background::Color(background),
            border: selected
                .highlight
                .map_or_else(Border::default, |highlight| highlight.border),
        });
        baked.push(selected);

        if hi < span_len {
            baked.push(with_text(span, &span.text[hi..]));
        }
    }

    baked
}

/// Clones `span` with new fragment text, keeping its style.
fn with_text(span: &Span<'static, String, Font>, text: &str) -> Span<'static, String, Font> {
    let mut part = span.clone();
    part.text = Cow::Owned(text.to_string());
    part
}

/// The largest char boundary not past `index`.
fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// The smallest char boundary not before `index`.
fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
#[path = "selectable_tests.rs"]
mod tests;
