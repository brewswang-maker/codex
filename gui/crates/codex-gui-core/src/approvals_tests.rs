//! Tests for the approval queue and wire payload construction.
#![allow(clippy::expect_used, clippy::panic)]

use super::ApprovalKind;
use super::Approvals;
use super::Decision;
use super::PendingApproval;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;

fn server_request(payload: serde_json::Value) -> ServerRequest {
    let message: codex_app_server_protocol::JSONRPCMessage =
        from_value(payload).expect("request frame decodes");
    match message {
        codex_app_server_protocol::JSONRPCMessage::Request(request) => {
            ServerRequest::try_from(request).expect("server request decodes")
        }
        other => panic!("expected a request frame, got {other:?}"),
    }
}

fn command_approval(id: &str) -> ServerRequest {
    server_request(json!({
        "method": "item/commandExecution/requestApproval",
        "id": id,
        "params": {
            "threadId": "t",
            "turnId": "turn-1",
            "itemId": "item-1",
            "startedAtMs": 1,
            "command": "rm -rf /",
            "reason": "needs network"
        }
    }))
}

#[test]
fn offered_tracks_the_three_dialog_kinds() {
    let mut approvals = Approvals::default();

    let command = approvals
        .offered(&command_approval("srv-1"))
        .expect("command approval is escalated");
    assert_eq!(
        command.kind,
        ApprovalKind::CommandExecution {
            command: Some("rm -rf /".to_string()),
            reason: Some("needs network".to_string()),
        }
    );

    let file_change = approvals
        .offered(&server_request(json!({
            "method": "item/fileChange/requestApproval",
            "id": "srv-2",
            "params": {
                "threadId": "t",
                "turnId": "turn-1",
                "itemId": "item-2",
                "startedAtMs": 1
            }
        })))
        .expect("file change approval is escalated");
    assert_eq!(file_change.kind, ApprovalKind::FileChange { reason: None });

    let permissions = approvals
        .offered(&server_request(json!({
            "method": "item/permissions/requestApproval",
            "id": "srv-3",
            "params": {
                "threadId": "t",
                "turnId": "turn-1",
                "itemId": "item-3",
                "startedAtMs": 1,
                "cwd": "/tmp",
                "permissions": {"network": {"enabled": true}}
            }
        })))
        .expect("permissions approval is escalated");
    assert!(matches!(permissions.kind, ApprovalKind::Permissions { .. }));

    assert_eq!(approvals.pending().len(), 3);
}

#[test]
fn unrelated_server_requests_are_not_escalated() {
    let mut approvals = Approvals::default();

    let offered = approvals.offered(&server_request(json!({
        "method": "currentTime/read",
        "id": "srv-1",
        "params": {"threadId": "t"}
    })));

    assert!(offered.is_none());
    assert!(approvals.pending().is_empty());
}

#[test]
fn resolved_removes_only_the_matching_entry() {
    let mut approvals = Approvals::default();
    let _ignored = approvals.offered(&command_approval("srv-1"));
    let _ignored = approvals.offered(&command_approval("srv-2"));

    assert!(approvals.resolved(&RequestId::String("srv-1".to_string())));

    assert_eq!(approvals.pending().len(), 1);
    assert_eq!(
        approvals.pending()[0].request_id,
        RequestId::String("srv-2".to_string())
    );

    // Resolving an unknown id reports the miss.
    assert!(!approvals.resolved(&RequestId::String("ghost".to_string())));
}

#[test]
fn take_removes_the_entry_and_returns_it() {
    let mut approvals = Approvals::default();
    let _ignored = approvals.offered(&command_approval("srv-1"));

    let taken = approvals.take("srv-1").expect("entry is pending");

    assert_eq!(taken.request_id, RequestId::String("srv-1".to_string()));
    assert!(approvals.pending().is_empty());
    assert!(approvals.take("srv-1").is_none());
}

#[test]
fn command_decision_payload_uses_wire_spelling() {
    let approval = PendingApproval {
        request_id: RequestId::String("srv-1".to_string()),
        kind: ApprovalKind::CommandExecution {
            command: None,
            reason: None,
        },
    };

    assert_eq!(
        Approvals::response_payload(&approval, Decision::Accept),
        json!({"decision": "accept"})
    );
    assert_eq!(
        Approvals::response_payload(&approval, Decision::AcceptForSession),
        json!({"decision": "acceptForSession"})
    );
    assert_eq!(
        Approvals::response_payload(&approval, Decision::Decline),
        json!({"decision": "decline"})
    );
    assert_eq!(
        Approvals::response_payload(&approval, Decision::Cancel),
        json!({"decision": "cancel"})
    );
}

#[test]
fn file_change_decision_payload_uses_wire_spelling() {
    let approval = PendingApproval {
        request_id: RequestId::String("srv-1".to_string()),
        kind: ApprovalKind::FileChange { reason: None },
    };

    assert_eq!(
        Approvals::response_payload(&approval, Decision::Decline),
        json!({"decision": "decline"})
    );
}

#[test]
fn permissions_payload_grants_verbatim_or_denies_empty() {
    let approval = PendingApproval {
        request_id: RequestId::String("srv-1".to_string()),
        kind: ApprovalKind::Permissions {
            reason: None,
            requested: from_value(json!({"network": {"enabled": true}})).expect("profile decodes"),
        },
    };

    let granted = Approvals::response_payload(&approval, Decision::Accept);
    assert_eq!(
        granted,
        json!({
            "permissions": {"network": {"enabled": true}},
            "scope": "turn"
        })
    );

    let denied = Approvals::response_payload(&approval, Decision::Decline);
    assert_eq!(
        denied,
        json!({
            "permissions": {},
            "scope": "turn"
        })
    );
}
