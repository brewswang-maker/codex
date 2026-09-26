//! Tests for the elicitation queue, schema projection, and form answers.
#![allow(clippy::expect_used, clippy::panic)]

use super::ElicitationDraft;
use super::Elicitations;
use super::FieldKind;
use super::FormField;
use super::auto_decline_elicitation;
use super::elicitation_default_drafts;
use super::elicitation_form_content;
use super::elicitation_form_fields;
use super::elicitation_response_payload;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::from_value;
use serde_json::json;
use std::collections::HashMap;

fn server_request(payload: Value) -> ServerRequest {
    let message: codex_app_server_protocol::JSONRPCMessage =
        from_value(payload).expect("request frame decodes");
    match message {
        codex_app_server_protocol::JSONRPCMessage::Request(request) => {
            ServerRequest::try_from(request).expect("server request decodes")
        }
        other => panic!("expected a request frame, got {other:?}"),
    }
}

/// A typed form covering every renderable field kind.
fn form_request(id: &str) -> ServerRequest {
    server_request(json!({
        "method": "mcpServer/elicitation/request",
        "id": id,
        "params": {
            "threadId": "t",
            "turnId": "turn-1",
            "serverName": "deployments",
            "mode": "form",
            "message": "Deploy config",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "title": "Name", "description": "Target name", "default": "prod"},
                    "count": {"type": "integer", "title": "Count", "default": 3},
                    "ratio": {"type": "number", "title": "Ratio"},
                    "dry": {"type": "boolean", "title": "Dry run", "default": true},
                    "region": {"type": "string", "title": "Region", "enum": ["us", "eu"], "default": "eu"},
                    "tier": {"type": "string", "title": "Tier", "oneOf": [
                        {"const": "std", "title": "Standard"},
                        {"const": "pro", "title": "Pro"}
                    ]},
                    "flags": {"type": "array", "title": "Flags", "items": {"type": "string", "enum": ["a", "b"]}, "default": ["b"]},
                    "extras": {"type": "array", "title": "Extras", "items": {"anyOf": [{"const": "x", "title": "X"}]}},
                    "legacy": {"type": "string", "title": "Legacy", "enum": ["l1"], "enumNames": ["Legacy One"]}
                },
                "required": ["name", "count"]
            }
        }
    }))
}

/// A URL-mode elicitation.
fn url_request(id: &str) -> ServerRequest {
    server_request(json!({
        "method": "mcpServer/elicitation/request",
        "id": id,
        "params": {
            "threadId": "t",
            "serverName": "deployments",
            "mode": "url",
            "message": "Sign in to continue",
            "url": "https://example.com/auth",
            "elicitationId": "auth-1"
        }
    }))
}

/// The raw OpenAI form mode the GUI declines automatically.
fn openai_form_request(id: &str) -> ServerRequest {
    server_request(json!({
        "method": "mcpServer/elicitation/request",
        "id": id,
        "params": {
            "threadId": "t",
            "serverName": "codex_apps",
            "mode": "openai/form",
            "message": "Allow this request?",
            "requestedSchema": {"type": "object"}
        }
    }))
}

/// The schema of a parsed form request.
fn schema_of(request: &ServerRequest) -> &codex_app_server_protocol::McpElicitationSchema {
    let ServerRequest::McpServerElicitationRequest { params, .. } = request else {
        panic!("expected an elicitation request");
    };
    let McpServerElicitationRequest::Form {
        requested_schema, ..
    } = &params.request
    else {
        panic!("expected a form elicitation");
    };
    requested_schema
}

fn field<'a>(fields: &'a [FormField], key: &str) -> &'a FormField {
    fields
        .iter()
        .find(|field| field.key == key)
        .unwrap_or_else(|| panic!("missing field {key}"))
}

#[test]
fn offered_queues_dialog_modes_and_skips_raw_openai_forms() {
    let mut elicitations = Elicitations::default();

    assert!(elicitations.offered(&form_request("srv-1")).is_some());
    assert!(elicitations.offered(&url_request("srv-2")).is_some());
    assert!(
        elicitations
            .offered(&openai_form_request("srv-3"))
            .is_none()
    );
    assert_eq!(elicitations.pending().len(), 2);
    assert_eq!(elicitations.pending()[0].server_name(), "deployments");
}

#[test]
fn resolved_and_take_follow_queue_semantics() {
    let mut elicitations = Elicitations::default();
    let _ignored = elicitations.offered(&form_request("srv-1"));
    let _ignored = elicitations.offered(&url_request("srv-2"));

    assert!(elicitations.resolved(&RequestId::String("srv-1".to_string())));
    assert!(!elicitations.resolved(&RequestId::String("ghost".to_string())));

    let taken = elicitations.take("srv-2").expect("entry is pending");
    assert_eq!(taken.request_id, RequestId::String("srv-2".to_string()));
    assert!(elicitations.pending().is_empty());
}

#[test]
fn auto_decline_covers_only_the_raw_openai_forms() {
    let declined = auto_decline_elicitation(&openai_form_request("srv-1"))
        .expect("raw OpenAI forms are declined");
    assert_eq!(declined.0, RequestId::String("srv-1".to_string()));
    assert_eq!(
        declined.1,
        json!({"action": "decline", "content": null, "_meta": null})
    );

    assert!(auto_decline_elicitation(&form_request("srv-2")).is_none());
    assert!(auto_decline_elicitation(&url_request("srv-3")).is_none());
}

#[test]
fn form_fields_project_every_schema_kind() {
    let request = form_request("srv-1");
    let fields = elicitation_form_fields(schema_of(&request));

    assert_eq!(
        *field(&fields, "name"),
        FormField {
            key: "name".to_string(),
            label: "Name".to_string(),
            description: Some("Target name".to_string()),
            required: true,
            kind: FieldKind::Text,
        }
    );
    assert_eq!(
        *field(&fields, "count"),
        FormField {
            key: "count".to_string(),
            label: "Count".to_string(),
            description: None,
            required: true,
            kind: FieldKind::Number { integer: true },
        }
    );
    assert_eq!(
        field(&fields, "ratio").kind,
        FieldKind::Number { integer: false }
    );
    assert_eq!(field(&fields, "dry").kind, FieldKind::Boolean);
    assert_eq!(
        field(&fields, "region").kind,
        FieldKind::SingleSelect {
            options: vec![
                ("us".to_string(), "us".to_string()),
                ("eu".to_string(), "eu".to_string()),
            ],
        }
    );
    assert_eq!(
        field(&fields, "tier").kind,
        FieldKind::SingleSelect {
            options: vec![
                ("std".to_string(), "Standard".to_string()),
                ("pro".to_string(), "Pro".to_string()),
            ],
        }
    );
    assert_eq!(
        field(&fields, "flags").kind,
        FieldKind::MultiSelect {
            options: vec![
                ("a".to_string(), "a".to_string()),
                ("b".to_string(), "b".to_string()),
            ],
        }
    );
    assert_eq!(
        field(&fields, "extras").kind,
        FieldKind::MultiSelect {
            options: vec![("x".to_string(), "X".to_string())],
        }
    );
    assert_eq!(
        field(&fields, "legacy").kind,
        FieldKind::SingleSelect {
            options: vec![("l1".to_string(), "Legacy One".to_string())],
        }
    );
}

#[test]
fn default_drafts_seed_from_the_schema() {
    let mut elicitations = Elicitations::default();
    let pending = elicitations
        .offered(&form_request("srv-1"))
        .expect("form is queued");

    let drafts = elicitation_default_drafts(&pending);

    assert_eq!(
        drafts["name"],
        ElicitationDraft {
            text: "prod".to_string(),
            ..ElicitationDraft::default()
        }
    );
    assert_eq!(
        drafts["count"],
        ElicitationDraft {
            text: "3".to_string(),
            ..ElicitationDraft::default()
        }
    );
    assert_eq!(
        drafts["dry"],
        ElicitationDraft {
            boolean: true,
            ..ElicitationDraft::default()
        }
    );
    assert_eq!(
        drafts["region"],
        ElicitationDraft {
            single: Some("eu".to_string()),
            ..ElicitationDraft::default()
        }
    );
    assert_eq!(
        drafts["flags"],
        ElicitationDraft {
            multi: vec!["b".to_string()],
            ..ElicitationDraft::default()
        }
    );
    assert_eq!(drafts["ratio"], ElicitationDraft::default());
}

#[test]
fn form_content_validates_and_serializes_the_answers() {
    let request = form_request("srv-1");
    let schema = schema_of(&request);

    let drafts = HashMap::from([
        (
            "name".to_string(),
            ElicitationDraft {
                text: " staging ".to_string(),
                ..ElicitationDraft::default()
            },
        ),
        (
            "count".to_string(),
            ElicitationDraft {
                text: "5".to_string(),
                ..ElicitationDraft::default()
            },
        ),
        (
            "dry".to_string(),
            ElicitationDraft {
                boolean: false,
                ..ElicitationDraft::default()
            },
        ),
        (
            "region".to_string(),
            ElicitationDraft {
                single: Some("us".to_string()),
                ..ElicitationDraft::default()
            },
        ),
        (
            "flags".to_string(),
            ElicitationDraft {
                // Clicked out of schema order; the payload stays ordered.
                multi: vec!["b".to_string(), "a".to_string()],
                ..ElicitationDraft::default()
            },
        ),
    ]);

    let content = elicitation_form_content(schema, &drafts).expect("drafts satisfy the schema");
    assert_eq!(
        content,
        json!({
            "name": "staging",
            "count": 5,
            "dry": false,
            "region": "us",
            "flags": ["a", "b"]
        })
    );

    // A required field left empty blocks submission.
    let mut missing = drafts.clone();
    missing.remove("count");
    assert_eq!(
        elicitation_form_content(schema, &missing),
        Err("\"Count\" is required".to_string())
    );

    // A non-numeric value fails with the field label.
    let mut invalid = drafts.clone();
    invalid.insert(
        "count".to_string(),
        ElicitationDraft {
            text: "many".to_string(),
            ..ElicitationDraft::default()
        },
    );
    assert_eq!(
        elicitation_form_content(schema, &invalid),
        Err("\"Count\" must be a whole number".to_string())
    );
}

#[test]
fn response_payload_uses_wire_spelling() {
    assert_eq!(
        elicitation_response_payload(
            McpServerElicitationAction::Accept,
            Some(json!({"name": "prod"}))
        ),
        json!({"action": "accept", "content": {"name": "prod"}, "_meta": null})
    );
    assert_eq!(
        elicitation_response_payload(McpServerElicitationAction::Decline, /*content*/ None),
        json!({"action": "decline", "content": null, "_meta": null})
    );
    assert_eq!(
        elicitation_response_payload(McpServerElicitationAction::Cancel, /*content*/ None),
        json!({"action": "cancel", "content": null, "_meta": null})
    );
}
