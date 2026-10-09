//! Tests for the WeChat approval broker; server requests are built through
//! their wire JSON so the decode path is exercised end to end.
#![allow(clippy::expect_used, clippy::panic)]

use super::{APPROVAL_TIMEOUT, AnswerOutcome, ApprovalBroker};
use codex_app_server_protocol::ServerRequest;
use serde_json::json;
use std::time::Instant;

fn command_request() -> ServerRequest {
    serde_json::from_value(json!({
        "method": "item/commandExecution/requestApproval",
        "id": 7,
        "params": {
            "threadId": "t1", "turnId": "u1", "itemId": "i1",
            "startedAtMs": 0,
            "command": "cargo publish",
            "reason": "需要网络访问",
        }
    }))
    .expect("command approval request decodes")
}

fn file_change_request() -> ServerRequest {
    serde_json::from_value(json!({
        "method": "item/fileChange/requestApproval",
        "id": 8,
        "params": {
            "threadId": "t1", "turnId": "u1", "itemId": "i2",
            "startedAtMs": 0,
            "reason": "写入仓库外的目录",
        }
    }))
    .expect("file change approval request decodes")
}

#[test]
fn offered_prompts_with_command_and_reason() {
    let mut broker = ApprovalBroker::new();
    let prompt = broker
        .offered(&command_request(), "alice")
        .expect("command escalates");
    assert!(prompt.contains("cargo publish"));
    assert!(prompt.contains("需要网络访问"));
    assert!(prompt.contains("60 秒"));
    assert!(broker.has_pending("alice"));
}

#[test]
fn answer_accept_and_decline_produce_typed_payloads() {
    let mut broker = ApprovalBroker::new();
    broker
        .offered(&command_request(), "alice")
        .expect("offered");

    let outcome = broker.answer("alice", "y");
    let super::AnswerOutcome::Answered(answered) = outcome else {
        panic!("y should answer the pending approval");
    };
    assert_eq!(answered.payload["decision"], "accept");
    assert!(answered.reply.contains("同意"));

    broker
        .offered(&command_request(), "alice")
        .expect("offered");
    let outcome = broker.answer("alice", "n");
    let super::AnswerOutcome::Answered(answered) = outcome else {
        panic!("n should answer the pending approval");
    };
    assert_eq!(answered.payload["decision"], "decline");
}

#[test]
fn answer_with_feedback_carries_follow_up() {
    let mut broker = ApprovalBroker::new();
    broker
        .offered(&file_change_request(), "bob")
        .expect("offered");

    let outcome = broker.answer("bob", "y 但请保留测试文件");
    let super::AnswerOutcome::Answered(answered) = outcome else {
        panic!("y <反馈> should answer");
    };
    assert_eq!(answered.payload["decision"], "accept");
    assert_eq!(answered.follow_up.as_deref(), Some("但请保留测试文件"));
}

#[test]
fn plain_task_text_is_not_an_answer() {
    let mut broker = ApprovalBroker::new();
    broker
        .offered(&command_request(), "alice")
        .expect("offered");
    assert!(matches!(
        broker.answer("alice", "请帮我修复编译错误"),
        AnswerOutcome::NoPending
    ));
}

#[test]
fn answer_without_pending_is_a_no_op() {
    let mut broker = ApprovalBroker::new();
    assert!(matches!(
        broker.answer("carol", "y"),
        AnswerOutcome::NoPending
    ));
}

#[test]
fn expiry_auto_declines_once() {
    let mut broker = ApprovalBroker::new();
    broker
        .offered(&command_request(), "alice")
        .expect("offered");

    let before = Instant::now() + APPROVAL_TIMEOUT - std::time::Duration::from_secs(1);
    assert!(broker.expired_before(before).is_empty());

    let after = Instant::now() + APPROVAL_TIMEOUT + std::time::Duration::from_secs(1);
    let expired = broker.expired_before(after);
    assert_eq!(expired.len(), 1);
    let (request_id, payload, contact, notice) = &expired[0];
    assert_eq!(request_id.to_string(), "7");
    assert_eq!(payload["decision"], "decline");
    assert_eq!(contact, "alice");
    assert!(notice.contains("超时"));
    assert!(!broker.has_pending("alice"));
}

#[test]
fn resolved_notification_drops_the_entry() {
    let mut broker = ApprovalBroker::new();
    broker
        .offered(&command_request(), "alice")
        .expect("offered");
    let request_id = codex_app_server_protocol::RequestId::Integer(7);
    broker.resolved(&request_id);
    assert!(!broker.has_pending("alice"));
    assert!(matches!(
        broker.answer("alice", "y"),
        AnswerOutcome::NoPending
    ));
}
