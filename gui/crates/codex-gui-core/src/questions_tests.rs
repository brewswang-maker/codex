//! Tests for the pending-questions queue and answer payload construction.
#![allow(clippy::expect_used, clippy::panic)]

use super::QuestionDraft;
use super::Questions;
use super::question_option_label;
use super::question_option_rows;
use super::question_response_payload;
use crate::questions::OTHER_ROW_LABEL;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;
use std::collections::HashMap;

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

/// A question request with one option-based and one free-form question.
fn question_request(id: &str) -> ServerRequest {
    server_request(json!({
        "method": "item/tool/requestUserInput",
        "id": id,
        "params": {
            "threadId": "t",
            "turnId": "turn-1",
            "itemId": "item-1",
            "questions": [
                {
                    "id": "q1",
                    "header": "Deploy",
                    "question": "Which region?",
                    "isOther": true,
                    "options": [
                        {"label": "us", "description": "US East"},
                        {"label": "eu", "description": "EU West"}
                    ]
                },
                {
                    "id": "q2",
                    "header": "Notes",
                    "question": "Anything else?"
                }
            ],
            "isBlocking": true
        }
    }))
}

#[test]
fn offered_queues_only_agent_questions() {
    let mut questions = Questions::default();

    let offered = questions
        .offered(&question_request("srv-1"))
        .expect("agent questions are escalated");

    assert_eq!(offered.request_id, RequestId::String("srv-1".to_string()));
    assert_eq!(offered.params.questions.len(), 2);
    assert_eq!(questions.pending().len(), 1);

    let unrelated = questions.offered(&server_request(json!({
        "method": "currentTime/read",
        "id": "srv-2",
        "params": {"threadId": "t"}
    })));
    assert!(unrelated.is_none());
    assert_eq!(questions.pending().len(), 1);
}

#[test]
fn resolved_removes_only_the_matching_entry() {
    let mut questions = Questions::default();
    let _ignored = questions.offered(&question_request("srv-1"));
    let _ignored = questions.offered(&question_request("srv-2"));

    assert!(questions.resolved(&RequestId::String("srv-1".to_string())));
    assert!(!questions.resolved(&RequestId::String("ghost".to_string())));

    assert_eq!(questions.pending().len(), 1);
    assert_eq!(
        questions.pending()[0].request_id,
        RequestId::String("srv-2".to_string())
    );
}

#[test]
fn take_removes_the_entry_and_returns_it() {
    let mut questions = Questions::default();
    let _ignored = questions.offered(&question_request("srv-1"));

    let taken = questions.take("srv-1").expect("entry is pending");

    assert_eq!(taken.request_id, RequestId::String("srv-1".to_string()));
    assert!(questions.pending().is_empty());
    assert!(questions.take("srv-1").is_none());
}

#[test]
fn free_choice_row_only_renders_alongside_real_options() {
    let with_options = server_request(json!({
        "method": "item/tool/requestUserInput",
        "id": "srv-1",
        "params": {
            "threadId": "t",
            "turnId": "turn-1",
            "itemId": "item-1",
            "questions": [{
                "id": "q1",
                "header": "Pick",
                "question": "Which one?",
                "isOther": true,
                "options": [{"label": "A", "description": "first"}]
            }],
            "isBlocking": true
        }
    }));
    let ServerRequest::ToolRequestUserInput { params, .. } = &with_options else {
        panic!("expected a user-input request");
    };
    let question = &params.questions[0];

    assert_eq!(question_option_rows(question), 2);
    assert_eq!(
        question_option_label(question, 1),
        Some(OTHER_ROW_LABEL.to_string())
    );
    assert_eq!(question_option_label(question, 2), None);

    // `isOther` without options has no free-choice row to fall back to.
    let ServerRequest::ToolRequestUserInput { params, .. } = &question_request("srv-2") else {
        panic!("expected a user-input request");
    };
    let free_form = &params.questions[1];
    assert_eq!(question_option_rows(free_form), 0);
    assert_eq!(question_option_label(free_form, 0), None);
}

#[test]
fn response_payload_encodes_selections_and_notes() {
    let ServerRequest::ToolRequestUserInput { params, .. } = question_request("srv-1") else {
        panic!("expected a user-input request");
    };

    let drafts = HashMap::from([
        (
            "q1".to_string(),
            QuestionDraft {
                // Index 2 is the free-choice row appended past the options.
                selected: Some(2),
                notes: "  pick this instead  ".to_string(),
            },
        ),
        ("q2".to_string(), QuestionDraft::default()),
    ]);

    let payload = question_response_payload(&params.questions, &drafts);
    let response: ToolRequestUserInputResponse =
        from_value(payload).expect("payload decodes as the wire response");

    assert_eq!(
        response,
        ToolRequestUserInputResponse {
            answers: HashMap::from([
                (
                    "q1".to_string(),
                    ToolRequestUserInputAnswer {
                        answers: vec![
                            OTHER_ROW_LABEL.to_string(),
                            "user_note: pick this instead".to_string(),
                        ],
                    },
                ),
                (
                    "q2".to_string(),
                    ToolRequestUserInputAnswer {
                        answers: Vec::new()
                    },
                ),
            ]),
        }
    );
}
