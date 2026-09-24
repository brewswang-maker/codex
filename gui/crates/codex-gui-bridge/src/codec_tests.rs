//! Codec round-trips and wire-format invariants.
#![allow(clippy::expect_used, clippy::panic)]

use super::codec::{decode_line, encode_line};
use codex_app_server_protocol::{JSONRPCMessage, JSONRPCNotification, JSONRPCRequest, RequestId};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn requests_have_no_jsonrpc_field() {
    let message = JSONRPCMessage::Request(JSONRPCRequest {
        id: RequestId::Integer(7),
        method: "initialize".to_string(),
        params: Some(json!({})),
        trace: None,
    });

    let line = encode_line(&message).expect("encode request");
    let value: serde_json::Value = serde_json::from_str(&line).expect("reparse");

    assert!(value.get("jsonrpc").is_none());
    assert_eq!(value.get("method"), Some(&json!("initialize")));
    assert_eq!(value.get("id"), Some(&json!(7)));
}

#[test]
fn request_round_trip() {
    let message = JSONRPCMessage::Request(JSONRPCRequest {
        id: RequestId::String("abc".to_string()),
        method: "turn/start".to_string(),
        params: Some(json!({"threadId": "t", "input": []})),
        trace: None,
    });

    let line = encode_line(&message).expect("encode");
    assert_eq!(decode_line(&line).expect("decode"), message);
}

#[test]
fn notification_round_trip() {
    let message = JSONRPCMessage::Notification(JSONRPCNotification {
        method: "thread/started".to_string(),
        params: Some(json!({})),
    });

    let line = encode_line(&message).expect("encode");
    assert_eq!(decode_line(&line).expect("decode"), message);
}

#[test]
fn server_request_decodes_through_jsonrpc_message() {
    // A server-initiated approval request arrives as a plain request frame and
    // must be recoverable as a `ServerRequest` via the tagged-enum round trip.
    let raw = json!({
        "method": "item/commandExecution/requestApproval",
        "id": "srv-1",
        "params": {
            "threadId": "t",
            "turnId": "turn-1",
            "itemId": "item-1",
            "startedAtMs": 1_700_000_000_000i64,
            "command": "ls -la"
        }
    });

    let JSONRPCMessage::Request(request) = decode_line(&raw.to_string()).expect("decode") else {
        panic!("expected a request frame");
    };

    let server_request: codex_app_server_protocol::ServerRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("to value"))
            .expect("server request");

    assert_eq!(server_request.id(), &RequestId::String("srv-1".to_string()));
}
