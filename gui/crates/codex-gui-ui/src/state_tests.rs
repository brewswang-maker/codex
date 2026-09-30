//! Unit coverage of the transcript selection model: line-key parsing,
//! per-line highlight ranges, and clipboard assembly.

use super::SelectionPoint;
use super::TextSelection;
use super::parse_line_key;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

/// A selection over `anchor..focus` with no reported line texts yet.
fn selection(anchor: (&str, usize), focus: (&str, usize)) -> TextSelection {
    TextSelection {
        anchor: SelectionPoint {
            key: String::from(anchor.0),
            offset: anchor.1,
        },
        focus: SelectionPoint {
            key: String::from(focus.0),
            offset: focus.1,
        },
        epoch: 1,
        lines: BTreeMap::new(),
    }
}

#[test]
fn parse_line_key_separates_the_families() {
    assert_eq!(parse_line_key("md:msg-1:2:3"), Some(("md:msg-1", 2, 3)));
    assert_eq!(parse_line_key("user:msg-1:5"), Some(("user:msg-1", 0, 5)));
    assert_eq!(parse_line_key("code:msg-1:abc"), None);
    assert_eq!(parse_line_key("md:msg-1:x:3"), None);
    assert_eq!(parse_line_key("md:msg-1:3"), None);
}

#[test]
fn accepts_keeps_the_focus_inside_the_anchor_message() {
    let selection = selection(("md:m:0:0", 0), ("md:m:0:1", 2));

    assert!(selection.accepts(&SelectionPoint {
        key: String::from("md:m:3:0"),
        offset: 1,
    }));
    assert!(!selection.accepts(&SelectionPoint {
        key: String::from("md:n:0:0"),
        offset: 1,
    }));
    // User bubbles are their own namespace even for the same id.
    assert!(!selection.accepts(&SelectionPoint {
        key: String::from("user:m:0"),
        offset: 1,
    }));
}

#[test]
fn range_for_single_line_is_the_offset_pair() {
    let selection = selection(("md:m:0:0", 2), ("md:m:0:0", 8));

    assert_eq!(selection.range_for("md:m:0:0", 10), Some(2..8));
    assert_eq!(selection.range_for("md:m:0:1", 10), None);
}

#[test]
fn range_for_spans_lines_and_blocks() {
    // Anchor at offset 4 of block 1 line 0; focus at 6 of block 2 line 1.
    let selection = selection(("md:m:1:0", 4), ("md:m:2:1", 6));

    assert_eq!(selection.range_for("md:m:1:0", 10), Some(4..10)); // anchor line tail
    assert_eq!(selection.range_for("md:m:1:1", 10), Some(0..10)); // middle line
    assert_eq!(selection.range_for("md:m:2:0", 10), Some(0..10)); // next block
    assert_eq!(selection.range_for("md:m:2:1", 10), Some(0..6)); // focus line head
    assert_eq!(selection.range_for("md:m:2:2", 10), None); // past the focus
    assert_eq!(selection.range_for("md:m:0:9", 10), None); // before the anchor
}

#[test]
fn range_for_normalizes_backwards_drags() {
    let selection = selection(("md:m:2:1", 6), ("md:m:1:0", 4));

    assert_eq!(selection.range_for("md:m:1:0", 10), Some(4..10));
    assert_eq!(selection.range_for("md:m:2:1", 10), Some(0..6));
}

#[test]
fn range_for_ignores_other_messages() {
    // A focus dragged into another message is dropped by the app, and a
    // selection whose points disagree paints nothing anywhere.
    let selection = selection(("md:m:0:0", 0), ("md:n:0:0", 3));

    assert_eq!(selection.range_for("md:m:0:0", 10), None);
    assert_eq!(selection.range_for("md:n:0:0", 10), None);
}

#[test]
fn text_joins_reported_lines_in_document_order() {
    let mut selection = selection(("md:m:0:0", 0), ("md:m:1:1", 6));
    selection
        .lines
        .insert(String::from("md:m:1:1"), String::from("second line"));
    selection
        .lines
        .insert(String::from("md:m:0:0"), String::from("first line"));

    assert_eq!(selection.text(), Some(String::from("first line\nsecond")));
}

#[test]
fn text_keeps_skipped_blank_lines() {
    let mut selection = selection(("md:m:0:0", 0), ("md:m:0:2", 1));
    selection
        .lines
        .insert(String::from("md:m:0:0"), String::from("top"));
    selection
        .lines
        .insert(String::from("md:m:0:2"), String::from("bottom"));

    // Line 1 was covered but reported nothing (an empty line rides as a
    // gap), so its newline must survive the join.
    assert_eq!(selection.text(), Some(String::from("top\n\nb")));
}

#[test]
fn text_is_none_without_reported_lines() {
    let selection = selection(("md:m:0:0", 0), ("md:m:0:1", 4));

    assert_eq!(selection.text(), None);
}
