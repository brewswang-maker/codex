//! Tests for the streaming delta buffer.

use super::MarkdownStream;
use pretty_assertions::assert_eq;

#[test]
fn push_then_drain_returns_buffered_text() {
    let mut stream = MarkdownStream::default();

    stream.push("item-1", "Hello");
    stream.push("item-1", " world");
    stream.push("item-2", "$ ls");

    let mut drained = stream.drain();
    drained.sort();

    assert_eq!(
        drained,
        vec![
            ("item-1".to_string(), "Hello world".to_string()),
            ("item-2".to_string(), "$ ls".to_string()),
        ]
    );
}

#[test]
fn drain_clears_the_buffer() {
    let mut stream = MarkdownStream::default();
    stream.push("item-1", "text");

    let _ignored = stream.drain();

    assert!(!stream.is_active());
    assert!(stream.drain().is_empty());
}

#[test]
fn cancel_drops_only_the_named_item() {
    let mut stream = MarkdownStream::default();
    stream.push("item-1", "obsolete");
    stream.push("item-2", "keep");

    stream.cancel("item-1");

    assert_eq!(
        stream.drain(),
        vec![("item-2".to_string(), "keep".to_string())]
    );
}

#[test]
fn is_active_reflects_the_buffer() {
    let mut stream = MarkdownStream::default();

    assert!(!stream.is_active());

    stream.push("item-1", "text");

    assert!(stream.is_active());
}
