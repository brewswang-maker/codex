//! Snapshot coverage of the streaming markdown segmentation (spec section
//! 10, snapshot row): the parsed item structure is the render structure, so
//! snapshotting `Content::items()` pins the streamed output shape without
//! involving pixels.

use super::render;
use iced::widget::markdown;

/// Streams the same document in realistic delta-sized chunks.
fn streamed_content(chunks: &[&str]) -> markdown::Content {
    let mut content = markdown::Content::new();
    for chunk in chunks {
        content.push_str(chunk);
    }
    content
}

#[test]
fn streamed_segmentation_is_snapshot_stable() {
    let content = streamed_content(&[
        "# Title\n\nSome intro text with ",
        "**bold** and `code`.\n\n",
        "- first bullet\n- second bullet\n\n",
        "```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n",
        "A [link](https://example.com) at the end.",
    ]);

    // The parsed items are exactly what `markdown_view::render` draws.
    insta::assert_debug_snapshot!(content.items());
}

#[test]
fn chunk_boundaries_do_not_change_the_final_items() {
    let fine = streamed_content(&["# Ti", "tle\n\nSom", "e intro **bo", "ld** text", ".\n"]);
    let coarse = streamed_content(&["# Title\n\nSome intro **bold** text.\n"]);

    assert_eq!(
        format!("{:?}", fine.items()),
        format!("{:?}", coarse.items()),
        "delta boundaries must not leak into the parsed structure"
    );
}

#[test]
fn render_returns_an_element_for_parsed_content() {
    let content = streamed_content(&[
        "# Title\n\nbody\n\n",
        "```rust\nfn main() {}\n```\n\n",
        "- bullet\n",
    ]);

    // A smoke check that the view layer accepts the streamed content,
    // including the selectable code-block override.
    let _element = render(&content, "msg-1", None, None);
}

#[test]
fn code_copy_keys_are_stable_and_content_scoped() {
    use super::code_copy_key;

    // Keys must reproduce across re-renders so the feedback check can
    // match, and stay distinct per message and per block content.
    let key = code_copy_key("m1", "let x = 1;");
    assert_eq!(key, code_copy_key("m1", "let x = 1;"));
    assert_ne!(key, code_copy_key("m1", "let x = 2;"));
    assert_ne!(key, code_copy_key("m2", "let x = 1;"));
}
