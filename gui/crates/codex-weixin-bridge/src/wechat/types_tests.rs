//! Wire-shape tests for the iLink protocol types.

use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::*;

#[test]
fn uint64_message_ids_survive_a_full_round_trip() {
    // JS needs the quote-the-number trick; serde_json is exact up to u64::MAX.
    let raw = r#"{
        "ret": 0,
        "msgs": [{
            "message_id": 18446744073709551615,
            "from_user_id": "alice@im.wechat",
            "context_token": "ctx-1",
            "item_list": [{"type": 1, "msg_id": 9007199254740993, "text_item": {"text": "hi"}}]
        }],
        "get_updates_buf": "cursor-blob"
    }"#;
    let resp: GetUpdatesResp = serde_json::from_str(raw).expect("parse should succeed");
    let message = resp.msgs.as_ref().and_then(|m| m.first()).expect("one msg");
    assert_eq!(message.message_id, Some(u64::MAX));
    assert_eq!(
        message.item_list.as_ref().and_then(|i| i.first()),
        Some(&MessageItem {
            r#type: Some(item_type::TEXT),
            msg_id: Some(9_007_199_254_740_993),
            text_item: Some(TextItem {
                text: Some("hi".to_string()),
            }),
            ..MessageItem::default()
        })
    );
    assert_eq!(message.text(), "hi");
    assert_eq!(resp.get_updates_buf.as_deref(), Some("cursor-blob"));
}

#[test]
fn empty_response_carries_the_previous_cursor() {
    let resp = GetUpdatesResp::empty("cursor-blob");
    assert!(!resp.is_api_error());
    assert_eq!(resp.get_updates_buf.as_deref(), Some("cursor-blob"));
    assert_eq!(
        serde_json::to_value(&resp).expect("serialize"),
        json!({"ret": 0, "msgs": [], "get_updates_buf": "cursor-blob"})
    );

    let fresh = GetUpdatesResp::empty("");
    assert_eq!(fresh.get_updates_buf, None);
}

#[test]
fn api_error_detection_covers_ret_and_errcode() {
    let mut resp = GetUpdatesResp {
        ret: Some(-1),
        ..GetUpdatesResp::default()
    };
    assert!(resp.is_api_error());
    resp.ret = Some(0);
    resp.errcode = Some(-14);
    assert!(resp.is_api_error());
    resp.errcode = Some(0);
    assert!(!resp.is_api_error());
}

#[test]
fn text_reply_serializes_to_the_documented_wire_shape() {
    let message = WeixinMessage::text_reply(
        "alice@im.wechat",
        "任务完成",
        Some("ctx-9"),
        "codex-weixin-bridge-01",
    );
    let wire: Value = serde_json::to_value(&message).expect("serialize");
    assert_eq!(
        wire,
        json!({
            "from_user_id": "",
            "to_user_id": "alice@im.wechat",
            "client_id": "codex-weixin-bridge-01",
            "message_type": 2,
            "message_state": 2,
            "item_list": [{"type": 1, "text_item": {"text": "任务完成"}}],
            "context_token": "ctx-9"
        })
    );
}

#[test]
fn empty_text_reply_omits_the_item_list() {
    let message = WeixinMessage::text_reply("alice@im.wechat", "", None, "cid");
    let wire: Value = serde_json::to_value(&message).expect("serialize");
    assert!(wire.get("item_list").is_none());
    assert!(wire.get("context_token").is_none());
    assert_eq!(wire.get("message_type"), Some(&json!(2)));
}

#[test]
fn client_ids_are_unique_and_prefixed() {
    let first = generate_client_id();
    let second = generate_client_id();
    assert!(first.starts_with("codex-weixin-bridge-"));
    assert_ne!(first, second);
}

#[test]
fn qr_status_parses_every_documented_state() {
    for (raw, expected) in [
        (r#""wait""#, QrStatus::Waiting),
        (r#""scaned""#, QrStatus::Scaned),
        (r#""confirmed""#, QrStatus::Confirmed),
        (r#""expired""#, QrStatus::Expired),
        (r#""scaned_but_redirect""#, QrStatus::ScanedButRedirect),
        (r#""need_verifycode""#, QrStatus::NeedVerifycode),
        (r#""verify_code_blocked""#, QrStatus::VerifyCodeBlocked),
        (r#""binded_redirect""#, QrStatus::BindedRedirect),
    ] {
        let parsed: QrStatus = serde_json::from_str(raw).expect("known state");
        assert_eq!(parsed, expected, "state {raw}");
    }

    let confirmed: QrStatusResponse = serde_json::from_str(
        r#"{"status":"confirmed","bot_token":"tok","ilink_bot_id":"hex@im.bot",
            "baseurl":"https://other.example","ilink_user_id":"me@im.wechat"}"#,
    )
    .expect("parse confirmed payload");
    assert_eq!(confirmed.status, Some(QrStatus::Confirmed));
    assert_eq!(confirmed.ilink_bot_id.as_deref(), Some("hex@im.bot"));
}
