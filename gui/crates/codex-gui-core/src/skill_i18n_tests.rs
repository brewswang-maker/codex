//! Coverage for the skill picker localization table.
#![allow(clippy::expect_used)]

use crate::skill_i18n::localize;
use pretty_assertions::assert_eq;

#[test]
fn known_skills_use_the_chinese_one_liner() {
    assert_eq!(
        localize(
            "superpowers:systematic-debugging",
            "Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes"
        ),
        "先系统定位根因，再提出修复"
    );
    assert_eq!(
        localize("imagegen", "Generate or edit raster images"),
        "生成或编辑位图图像（照片、插画、纹理等）"
    );
}

#[test]
fn marketplace_skills_resolve_through_the_generated_catalog() {
    assert_eq!(
        localize(
            "coderabbit:code-review",
            "Reviews code changes using CodeRabbit AI. Use when user asks for code review"
        ),
        "用 CodeRabbit AI 评审代码变更与 PR"
    );
    // Unknown skills under a catalogued plugin reuse the parent's copy.
    assert_eq!(
        localize("figma:some-future-skill", "Brand new Figma skill"),
        "使用 Figma MCP server 获取设计上下文、截图、变量和资产，并将 Figma 节点转换为生产代码；当任务涉及 Figma URL、节点 ID 或 d 时触发。",
        "the parent fallback keeps picker rows Chinese"
    );
    // Standalone user/repo skills resolve by their bare wire name.
    assert_eq!(
        localize(
            "hypit-rs",
            "Drive the hypit-rs short-video engine (SVML script to rendered MP4) from an agent."
        ),
        "从智能体驱动 hypit-rs 短视频引擎：SVML 脚本到成片渲染"
    );
}

#[test]
fn unknown_skills_fall_back_to_the_condensed_wire_description() {
    assert_eq!(
        localize("my-skill", "Watch   the  build\nand report back"),
        "Watch the build and report back",
        "whitespace is squeezed"
    );
    assert_eq!(
        localize("my-skill", "第一句话结束。第二句话不应出现。"),
        "第一句话结束。",
        "a Chinese full stop ends the excerpt"
    );
}

#[test]
fn blank_descriptions_stay_blank() {
    assert_eq!(localize("my-skill", ""), "");
}

#[test]
fn overlong_fallbacks_are_cut_with_an_ellipsis() {
    let localized = localize("my-skill", &"a".repeat(200));

    assert_eq!(localized.chars().count(), 81);
    assert!(localized.ends_with('…'), "the cut is marked: {localized}");
}
