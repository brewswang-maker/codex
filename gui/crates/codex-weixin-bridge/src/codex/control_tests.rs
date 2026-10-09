//! Tests for command helpers.
#![allow(clippy::expect_used)]

use super::{
    find_section, model_list_text, parse_model_pick, parse_resume_pick, sanitize_contact,
    thread_list_text, thread_pairs,
};
use serde_json::json;

#[test]
fn model_picker_lists_ids() {
    let text = model_list_text(&["gpt-5.1".to_string(), "o4-mini".to_string()]);
    assert!(text.contains("1. gpt-5.1"));
    assert!(text.contains("2. o4-mini"));

    assert_eq!(model_list_text(&[]), "未获取到可用模型列表。");
}

#[test]
fn thread_picker_lists_titles() {
    let threads = vec![
        ("t1".to_string(), "修复登录".to_string()),
        ("t2".to_string(), "(无标题)".to_string()),
    ];
    let text = thread_list_text(&threads);
    assert!(text.contains("1. 修复登录"));
    assert!(text.contains("2. (无标题)"));

    assert_eq!(thread_list_text(&[]), "没有可恢复的线程。");
}

#[test]
fn thread_pairs_extracts_id_and_title() {
    let value = json!({
        "data": [
            {"id": "t1", "name": "修复登录"},
            {"id": "t2"},
        ],
    });
    let pairs = thread_pairs(&value);
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0], ("t1".to_string(), "修复登录".to_string()));
    assert_eq!(pairs[1], ("t2".to_string(), "(无标题)".to_string()));
}

#[test]
fn picks_are_one_based_indices() {
    assert_eq!(parse_model_pick("2"), Some(2));
    assert_eq!(parse_model_pick(" 3 "), Some(3));
    assert_eq!(parse_model_pick("0"), None);
    assert_eq!(parse_model_pick("abc"), None);
    assert_eq!(parse_resume_pick("1"), Some(1));
    assert_eq!(parse_resume_pick("-1"), None);
}

#[test]
fn contact_ids_are_trimmed_and_bounded() {
    assert_eq!(sanitize_contact(" alice ").expect("ok"), "alice");
    assert!(sanitize_contact("").is_err());
    assert!(sanitize_contact("   ").is_err());
    let long = "x".repeat(200);
    assert!(sanitize_contact(&long).is_err());
}

#[test]
fn section_lookup_matches_by_exact_name() {
    let listed = json!({
        "data": [
            {"id": "s1", "name": "工作"},
            {"id": "s2", "name": "微信任务"},
        ],
    });
    assert_eq!(find_section(&listed, "微信任务").as_deref(), Some("s2"));
    assert_eq!(find_section(&listed, "不存在"), None);
    assert_eq!(find_section(&json!({}), "微信任务"), None);
}
