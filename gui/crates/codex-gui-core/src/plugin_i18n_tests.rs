//! Coverage for the curated marketplace localization table.
#![allow(clippy::expect_used)]

use crate::plugin_i18n::localize;
use pretty_assertions::assert_eq;

#[test]
fn curated_plugins_use_the_chinese_copy() {
    assert_eq!(
        localize(
            "game-studio",
            Some("Design, prototype, and ship browser games")
        ),
        (
            "游戏工作室".to_string(),
            Some("设计、原型化并发布浏览器游戏".to_string())
        )
    );
}

#[test]
fn uncurated_plugins_keep_the_wire_metadata() {
    assert_eq!(
        localize("pdf-tools", Some("Split PDF files")),
        ("pdf-tools".to_string(), Some("Split PDF files".to_string()))
    );
    assert_eq!(
        localize("pdf-tools", None),
        ("pdf-tools".to_string(), None),
        "a missing description stays missing"
    );
}
