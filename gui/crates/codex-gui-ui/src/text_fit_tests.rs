//! Tests for the CJK-aware single-line truncation helpers.
#![allow(clippy::expect_used, clippy::panic)]

use super::display_width;
use super::ellipsize;
use pretty_assertions::assert_eq;

#[test]
fn fitting_text_is_returned_verbatim() {
    assert_eq!(ellipsize("hello", 10), "hello");
    assert_eq!(ellipsize("你好", 4), "你好");
    assert_eq!(ellipsize("", 4), "");
}

#[test]
fn ascii_overflow_cut_at_budget() {
    assert_eq!(ellipsize("abcdefghij", 6), "abcde…");
}

#[test]
fn cjk_counts_double_and_never_splits_a_char() {
    // 4 CJK chars = width 8; a budget of 6 fits 2 chars + …
    assert_eq!(ellipsize("你好世界", 6), "你好…");
    // Budget 5 still fits only 2 CJK chars (a 3rd would overflow to 6+1).
    assert_eq!(ellipsize("你好世界", 5), "你好…");
}

#[test]
fn display_width_counts_fullwidth_as_two() {
    assert_eq!(display_width("ab"), 2);
    assert_eq!(display_width("中文"), 4);
    assert_eq!(display_width("a中b文"), 6);
}
