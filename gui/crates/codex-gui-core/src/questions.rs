//! Pending `item/tool/requestUserInput` escalations and their answers.
//!
//! The agent can pause a turn and ask the user a handful of multiple-choice
//! (or free-form) questions through one server-initiated request. The GUI
//! shows them as a modal form; this module tracks the outstanding requests
//! and projects the drafted answers into the wire response.

use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ToolRequestUserInputAnswer;
use codex_app_server_protocol::ToolRequestUserInputParams;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_app_server_protocol::ToolRequestUserInputResponse;
use serde_json::Value;
use std::collections::HashMap;

/// The label of the trailing free-choice row, shared with the reference
/// client so servers see the same answer vocabulary.
pub const OTHER_ROW_LABEL: &str = "None of the above";

/// One outstanding request awaiting answers.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingQuestion {
    /// Wire id used both to answer the request and to match the
    /// `serverRequest/resolved` notification.
    pub request_id: RequestId,
    /// The questions and their options.
    pub params: ToolRequestUserInputParams,
}

/// The GUI's draft for one question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestionDraft {
    /// Selected option index; the trailing free-choice row sits at
    /// `options.len()` when the question enables it.
    pub selected: Option<usize>,
    /// Free-text notes submitted alongside the selection.
    pub notes: String,
}

/// Queue of pending question requests, answered in arrival order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Questions {
    pending: Vec<PendingQuestion>,
}

impl Questions {
    /// Outstanding requests, oldest first; the dialog shows the head.
    pub fn pending(&self) -> &[PendingQuestion] {
        &self.pending
    }

    /// Normalizes a server request into the queue; returns `None` for
    /// request kinds that are not agent questions.
    pub fn offered(&mut self, request: &ServerRequest) -> Option<PendingQuestion> {
        let ServerRequest::ToolRequestUserInput { request_id, params } = request else {
            return None;
        };
        let pending = PendingQuestion {
            request_id: request_id.clone(),
            params: params.clone(),
        };
        self.pending.push(pending.clone());
        Some(pending)
    }

    /// Drops the entry matched by a `serverRequest/resolved` notification;
    /// the server answered the request itself (e.g. the turn was cancelled).
    pub fn resolved(&mut self, request_id: &RequestId) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|question| question.request_id != *request_id);
        self.pending.len() != before
    }

    /// Takes the entry out of the queue so its answer can be sent.
    pub fn take(&mut self, request_id: &str) -> Option<PendingQuestion> {
        let index = self
            .pending
            .iter()
            .position(|question| question.request_id.to_string() == request_id)?;
        Some(self.pending.remove(index))
    }
}

/// The option label at `index`, including the trailing free-choice row;
/// `None` for a free-form question or an out-of-range index.
pub fn option_label(question: &ToolRequestUserInputQuestion, index: usize) -> Option<String> {
    let options = question.options.as_ref()?;
    if let Some(option) = options.get(index) {
        return Some(option.label.clone());
    }
    if index == options.len() && other_row_enabled(question) {
        return Some(OTHER_ROW_LABEL.to_string());
    }
    None
}

/// How many selectable rows the question renders: its options plus, when
/// `is_other` is set and options exist, one free-choice row.
pub fn option_rows(question: &ToolRequestUserInputQuestion) -> usize {
    let options_len = question.options.as_ref().map_or(0, Vec::len);
    if other_row_enabled(question) {
        options_len + 1
    } else {
        options_len
    }
}

/// The free-choice row only appears alongside real options.
fn other_row_enabled(question: &ToolRequestUserInputQuestion) -> bool {
    question.is_other
        && question
            .options
            .as_ref()
            .is_some_and(|options| !options.is_empty())
}

/// Builds the wire response from per-question drafts. A question with no
/// selection and no notes answers with an empty list, mirroring the
/// reference client's auto-resolution.
#[allow(clippy::expect_used)]
pub fn response_payload(
    questions: &[ToolRequestUserInputQuestion],
    drafts: &HashMap<String, QuestionDraft>,
) -> Value {
    let empty = QuestionDraft::default();
    let mut answers = HashMap::new();
    for question in questions {
        let draft = drafts.get(&question.id).unwrap_or(&empty);
        let mut list = Vec::new();
        if let Some(label) = draft
            .selected
            .and_then(|index| option_label(question, index))
        {
            list.push(label);
        }
        let notes = draft.notes.trim();
        if !notes.is_empty() {
            // The `user_note: ` prefix is the shared encoding for the
            // free-text companion of a selection.
            list.push(format!("user_note: {notes}"));
        }
        answers.insert(
            question.id.clone(),
            ToolRequestUserInputAnswer { answers: list },
        );
    }
    serde_json::to_value(ToolRequestUserInputResponse { answers })
        .expect("question response serializes")
}
