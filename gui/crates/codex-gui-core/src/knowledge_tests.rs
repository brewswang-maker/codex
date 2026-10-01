//! Tests for the knowledge base.
#![allow(clippy::expect_used, clippy::panic)]

use super::KnowledgeBase;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gui-knowledge-test-{}-{tag}.json",
        std::process::id()
    ))
}

fn seeded() -> KnowledgeBase {
    let mut base = KnowledgeBase::default();
    base.add(
        "构建命令",
        "cargo build --release 在 gui 目录执行",
        &[String::from("构建")],
        "workspace",
        10,
    );
    base.add(
        "审批策略",
        "rm -rf 会自动拒绝",
        &[String::from("审批"), String::from("安全")],
        "workspace",
        20,
    );
    base.add(
        "发布流程",
        "先 fmt 再测试再提交",
        &[String::from("流程"), String::from("git")],
        "/repo",
        30,
    );
    base
}

#[test]
fn add_update_remove_round_trip() {
    let mut base = seeded();
    assert_eq!(base.entries.len(), 3);

    let id = base.entries[0].id.clone();
    assert!(base.update(&id, "构建命令 v2", "新正文", &[String::from("构建")], 99,));
    assert_eq!(base.entries[0].title, "构建命令 v2");
    assert_eq!(base.entries[0].updated_at, 99);
    assert_eq!(base.entries[0].created_at, 10);

    assert!(base.remove(&id));
    assert_eq!(base.entries.len(), 2);
    assert!(!base.remove(&id));
}

#[test]
fn search_scores_title_over_body_and_sorts() {
    let base = seeded();
    let hits = base.search("审批");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "审批策略");

    // "构建" hits the 构建命令 title (3) + tag (2) + body (1) = 6, and the
    // 发布流程 body not at all.
    let hits = base.search("构建");
    assert_eq!(
        hits.first().map(|entry| entry.title.as_str()),
        Some("构建命令")
    );
}

#[test]
fn search_is_case_insensitive_and_empty_queries_match_nothing() {
    let mut base = KnowledgeBase::default();
    base.add("Release build", "cargo build --release", &[], "s", 1);
    assert_eq!(base.search("release").len(), 1);
    assert_eq!(base.search("RELEASE").len(), 1);
    assert!(base.search("").is_empty());
    assert!(base.search("   ").is_empty());
}

#[test]
fn inject_fragment_respects_budget_and_orders_hits() {
    let base = seeded();
    let fragment = base
        .inject_fragment("审批 构建命令", 4_096)
        .expect("has hits");
    assert!(fragment.contains("## 审批策略"));
    assert!(fragment.contains("## 构建命令"));
    // Title+tag hits outrank body-only mentions, so 审批策略 comes first.
    assert!(
        fragment.find("审批策略") < fragment.find("构建命令"),
        "higher-scored entry first: {fragment}"
    );

    assert!(base.inject_fragment("审批", 8).is_none(), "tiny budget");
    assert!(base.inject_fragment("不存在的词", 4_096).is_none());
}

#[test]
fn registry_round_trips_through_disk() {
    let path = temp_path("roundtrip");
    seeded().save_to(&path);
    assert!(KnowledgeBase::load_from(&path).is_some());

    let loaded = KnowledgeBase::load_from(&path).expect("parses back");
    assert_eq!(loaded.entries.len(), 3);
    assert_eq!(loaded.entries[1].tags, vec!["审批", "安全"]);
    let _ = std::fs::remove_file(&path);
}
