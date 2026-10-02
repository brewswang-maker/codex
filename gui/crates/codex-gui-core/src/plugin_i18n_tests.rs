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

#[test]
fn frontmatter_names_resolve_via_normalization() {
    // Composio skills whose frontmatter name carries spaces resolve to
    // the catalog's dashed wire name.
    let (name, description) = localize("Ahrefs Automation", None);
    assert_eq!(name, "Ahrefs 自动化");
    assert!(description.is_some_and(|text| text.contains("SEO")));
    // Exact wire names still match first.
    assert_eq!(
        localize("algolia-automation", None).0,
        "Algolia 自动化".to_string()
    );
    // Unknown names never invent a row.
    assert_eq!(
        localize("Totally Unknown Skill", Some("desc")),
        (
            "Totally Unknown Skill".to_string(),
            Some("desc".to_string())
        )
    );
}

#[test]
fn anthropic_agent_skills_resolve_by_wire_name() {
    // The anthropic-agent-skills marketplace bundles skills under a
    // `plugin:skill` wire name; both rows resolve from one catalog.
    let (name, description) = localize("document-skills:docx", None);
    assert_eq!(name, "Word 文档");
    assert!(description.is_some_and(|text| text.contains("Word")));

    let (name, _) = localize("example-skills:algorithmic-art", None);
    assert_eq!(name, "算法艺术");

    // Single-skill plugins repeat their own name after the prefix.
    assert_eq!(localize("claude-api:claude-api", None).0, "Claude API 参考");

    // The plugin rows themselves are also covered.
    let (name, description) = localize("document-skills", None);
    assert_eq!(name, "文档技能");
    assert!(description.is_some_and(|text| text.contains("PDF")));
}
