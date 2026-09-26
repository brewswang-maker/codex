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
