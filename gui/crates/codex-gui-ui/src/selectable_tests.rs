//! Unit coverage of the selectable-text helpers: line splitting across
//! styled spans, selection baking into span highlights, selection
//! boundary clamps, and row hit testing.

#![allow(clippy::expect_used)]

use super::bake_selection;
use super::clamp_selection;
use super::row_position;
use super::split_lines;
use iced::Background;
use iced::Border;
use iced::Color;
use iced::Font;
use iced::Point;
use iced::Rectangle;
use iced::Size;
use iced::advanced::text::Highlight;
use iced::advanced::text::Span;
use iced::color;
use pretty_assertions::assert_eq;

const BLUE: Color = color!(0x3366FF);
const RED: Color = color!(0xFF0000);

/// A plain span over `text`, the shape the markdown cache hands in.
fn span(text: &str) -> Span<'static, String, Font> {
    Span::new(String::from(text))
}

/// The concatenated text of one split line.
fn line_text(line: &[Span<'static, String, Font>]) -> String {
    line.iter().map(|span| span.text.as_ref()).collect()
}

#[test]
fn split_lines_returns_none_without_newlines() {
    let spans = vec![span("hello "), span("world")];

    assert_eq!(split_lines(&spans), None);
}

#[test]
fn split_lines_splits_across_span_boundaries() {
    // The newlines straddle span boundaries: the first span ends with
    // one, the second starts and ends with ones, so the trailing empty
    // line must survive as an empty group.
    let spans = vec![span("alpha\nbeta"), span("\ngamma\n")];

    let lines = split_lines(&spans).expect("newlines force a split");

    assert_eq!(lines.len(), 4);
    assert_eq!(line_text(&lines[0]), "alpha");
    assert_eq!(line_text(&lines[1]), "beta");
    assert_eq!(line_text(&lines[2]), "gamma");
    assert_eq!(line_text(&lines[3]), "");
}

#[test]
fn split_lines_keeps_each_fragment_style() {
    let mut bold = span("alpha\nbeta");
    bold.font = Some(Font {
        weight: iced::font::Weight::Bold,
        ..Font::default()
    });
    let plain = span("gamma");
    let spans = vec![bold, plain];

    let lines = split_lines(&spans).expect("newlines force a split");

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].len(), 1);
    // The bold span splits into both lines; the plain one lands on the
    // second line next to the bold tail.
    assert_eq!(lines[0][0].font, lines[1][0].font);
    assert!(lines[0][0].font.is_some());
    assert_eq!(lines[1].len(), 2);
    assert_eq!(lines[1][1].font, None);
    assert_eq!(line_text(&lines[1]), "betagamma");
}

#[test]
fn bake_selection_splits_and_highlights() {
    let spans = vec![span("hello world")];

    let baked = bake_selection(&spans, &(6..11), BLUE);

    assert_eq!(baked.len(), 2);
    assert_eq!(baked[0].text.as_ref(), "hello ");
    assert_eq!(baked[0].highlight, None);
    assert_eq!(baked[1].text.as_ref(), "world");
    assert_eq!(
        baked[1].highlight.map(|highlight| highlight.background),
        Some(Background::Color(BLUE))
    );
}

#[test]
fn bake_selection_spans_multiple_spans() {
    let spans = vec![span("abc"), span("defgh")];

    // Bytes 2..6 select "c" (the tail of the first span) and "def".
    let baked = bake_selection(&spans, &(2..6), BLUE);

    assert_eq!(baked.len(), 4);
    assert_eq!(baked[0].text.as_ref(), "ab");
    assert_eq!(baked[0].highlight, None);
    assert_eq!(baked[1].text.as_ref(), "c");
    assert!(baked[1].highlight.is_some());
    assert_eq!(baked[2].text.as_ref(), "def");
    assert!(baked[2].highlight.is_some());
    assert_eq!(baked[3].text.as_ref(), "gh");
    assert_eq!(baked[3].highlight, None);
}

#[test]
fn bake_selection_keeps_the_existing_highlight_border() {
    let mut styled = span("abcdef");
    styled.highlight = Some(Highlight {
        background: Background::Color(RED),
        border: Border {
            color: RED,
            width: 1.0,
            radius: 2.0.into(),
        },
    });

    let baked = bake_selection(&[styled], &(2..4), BLUE);

    assert_eq!(baked.len(), 3);
    let selected = &baked[1];
    assert_eq!(
        selected.highlight.map(|highlight| highlight.background),
        Some(Background::Color(BLUE))
    );
    assert_eq!(
        selected.highlight.map(|highlight| highlight.border.width),
        Some(1.0)
    );
}

#[test]
fn clamp_selection_snaps_and_rejects() {
    // 'é' occupies bytes 0..2, so byte 1 is not a char boundary.
    let content = "éa";

    assert_eq!(clamp_selection(None, content), None);
    assert_eq!(clamp_selection(Some(&(1..3)), content), Some(0..3));
    assert_eq!(clamp_selection(Some(&(0..99)), content), Some(0..3));
    // Built from variables so the empty reversed range is not a literal
    // (clippy rejects those outright).
    let (start, end) = (3, 1);
    assert_eq!(clamp_selection(Some(&(start..end)), content), None);
    assert_eq!(clamp_selection(Some(&(3..3)), content), None);
}

#[test]
fn row_position_accepts_the_leading_marker_band() {
    let bounds = Rectangle::new(Point::new(100.0, 50.0), Size::new(200.0, 20.0));

    // On the text: the position passes through unchanged.
    assert_eq!(
        row_position(bounds, Point::new(140.0, 60.0)),
        Some(Point::new(40.0, 10.0))
    );
    // In the leading marker band: accepted, collapsed to the line start,
    // so a drag that begins on a bullet or checkbox selects from char
    // zero.
    assert_eq!(
        row_position(bounds, Point::new(70.0, 60.0)),
        Some(Point::new(0.0, 10.0))
    );
    // Further left misses the line entirely.
    assert_eq!(row_position(bounds, Point::new(67.0, 60.0)), None);
    // And the y band stays strict, so a row never claims its neighbors'.
    assert_eq!(row_position(bounds, Point::new(140.0, 45.0)), None);
}
