//! Tests for notification-to-text shaping; protocol items are built through
//! their serde wire shape so the assertions also pin the expected payloads.
#![allow(clippy::expect_used)]

use super::{
    clean_for_wechat, describe_duration, final_reply, split_weixin_actions, strip_inline_markup,
    summarize_item,
};
use codex_app_server_protocol::{ThreadItem, Turn};
use serde_json::json;

fn parse_item(value: serde_json::Value) -> ThreadItem {
    serde_json::from_value(value).expect("item decodes")
}

fn parse_turn(value: serde_json::Value) -> Turn {
    serde_json::from_value(value).expect("turn decodes")
}

#[test]
fn command_summary_marks_exit_code() {
    let ok = parse_item(json!({
        "type": "commandExecution", "id": "i1", "command": "cargo test",
        "cwd": "/repo", "status": "completed", "commandActions": [], "exitCode": 0,
    }));
    assert_eq!(summarize_item(&ok).as_deref(), Some("✓ cargo test"));

    let failed = parse_item(json!({
        "type": "commandExecution", "id": "i2", "command": "cargo test",
        "cwd": "/repo", "status": "failed", "commandActions": [], "exitCode": 101,
    }));
    assert_eq!(
        summarize_item(&failed).as_deref(),
        Some("✗ cargo test（退出码 101）")
    );
}

#[test]
fn file_change_summary_lists_paths() {
    let item = parse_item(json!({
        "type": "fileChange", "id": "i3",
        "changes": [
            {"path": "src/a.rs", "kind": {"type": "add"}, "diff": ""},
            {"path": "src/b.rs", "kind": {"type": "update"}, "diff": ""},
        ],
        "status": "completed",
    }));
    assert_eq!(
        summarize_item(&item).as_deref(),
        Some("✎ src/a.rs, src/b.rs")
    );
}

#[test]
fn agent_and_reasoning_items_are_not_progress() {
    let message = parse_item(json!({"type": "agentMessage", "id": "i4", "text": "hi"}));
    assert_eq!(summarize_item(&message), None);
    let reasoning = parse_item(json!({"type": "reasoning", "id": "i5"}));
    assert_eq!(summarize_item(&reasoning), None);
}

#[test]
fn final_reply_prefers_turn_error() {
    let turn = parse_turn(json!({
        "id": "t1", "items": [], "status": "failed",
        "error": {"message": "模型超时"},
        "startedAt": null, "completedAt": null, "durationMs": null,
    }));
    assert_eq!(final_reply(&turn).as_deref(), Some("任务出错: 模型超时"));
}

#[test]
fn final_reply_takes_last_agent_message() {
    let turn = parse_turn(json!({
        "id": "t2",
        "items": [
            {"type": "agentMessage", "id": "a1", "text": "第一段回复"},
            {"type": "commandExecution", "id": "c1", "command": "ls",
             "cwd": ".", "status": "completed", "commandActions": [], "exitCode": 0},
            {"type": "agentMessage", "id": "a2", "text": "最终结论"},
        ],
        "status": "completed",
        "error": null,
        "startedAt": null, "completedAt": null, "durationMs": 1500,
    }));
    assert_eq!(final_reply(&turn).as_deref(), Some("最终结论"));
}

#[test]
fn final_reply_skips_blank_messages() {
    let turn = parse_turn(json!({
        "id": "t3",
        "items": [{"type": "agentMessage", "id": "a1", "text": "  "}],
        "status": "completed",
        "error": null,
        "startedAt": null, "completedAt": null, "durationMs": null,
    }));
    assert_eq!(final_reply(&turn), None);
}

#[test]
fn duration_description() {
    assert_eq!(describe_duration(Some(400)), "用时 400ms");
    assert_eq!(describe_duration(Some(1_500)), "用时 1.5s");
    assert_eq!(describe_duration(Some(63_000)), "用时 1m03s");
    assert_eq!(describe_duration(None), "");
}

#[test]
fn cleanup_drops_markdown_noise() {
    let cleaned = clean_for_wechat(
        "# 标题\n\n**重点**：见 [文档](https://example.com)。\n\n```rust\nfn main() {}\n```\n\n普通行。",
    );
    assert!(cleaned.contains("重点：见 文档。"));
    assert!(!cleaned.contains('#'));
    assert!(!cleaned.contains("**"));
    assert!(!cleaned.contains("https://example.com"));
    // Code fences are dropped but fenced content stays readable.
    assert!(cleaned.contains("fn main() {}"));
}

#[test]
fn cleanup_clamps_length() {
    let long = "x".repeat(5_000);
    let cleaned = clean_for_wechat(&long);
    assert!(cleaned.chars().count() < 3_000);
    assert!(cleaned.ends_with("（内容过长已截断）"));
}

#[test]
fn inline_markup_strip_keeps_plain_brackets() {
    assert_eq!(strip_inline_markup("[text](https://x.y)"), "text");
    assert_eq!(
        strip_inline_markup("plain [not a link] here"),
        "plain [not a link] here"
    );
    assert_eq!(strip_inline_markup("**bold** and `code`"), "bold and code");
}

#[test]
fn actions_split_extracts_send_lines_and_keeps_body() {
    let reply = "已生成两张图：\n```codex-weixin-actions\nsend /tmp/out/a.png\n\"/tmp/out/b.pdf\"\n```\n完成。";
    let (body, actions) = split_weixin_actions(reply);
    assert_eq!(body, "已生成两张图：\n完成。");
    assert_eq!(actions, vec!["/tmp/out/a.png", "/tmp/out/b.pdf"]);
}

#[test]
fn actions_split_bare_paths_and_multiple_blocks() {
    let reply = "```codex-weixin-actions\n/tmp/a.png\n```\n中间文本\n```codex-weixin-actions\nsend /tmp/b.mp4\n```";
    let (body, actions) = split_weixin_actions(reply);
    assert_eq!(body, "中间文本");
    assert_eq!(actions, vec!["/tmp/a.png", "/tmp/b.mp4"]);
}

#[test]
fn actions_split_noop_without_fence() {
    let (body, actions) = split_weixin_actions("普通回复\n```rust\nfn x() {}\n```");
    assert_eq!(actions, Vec::<String>::new());
    assert!(body.contains("普通回复"));
    // Rust fences survive (they are not actions blocks).
    assert!(body.contains("fn x() {}"));
}

#[test]
fn actions_split_ignores_blank_lines_inside_block() {
    let (body, actions) = split_weixin_actions("```codex-weixin-actions\n\nsend /a.png\n\n```");
    assert_eq!(body, "");
    assert_eq!(actions, vec!["/a.png"]);
}
