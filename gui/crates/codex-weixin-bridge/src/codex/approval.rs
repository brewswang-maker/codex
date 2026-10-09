//! WeChat-side approval forwarding.
//!
//! Server approval requests are normalized into short WeChat prompts; the
//! contact answers with `y` / `n` / `y <反馈>` within 60 seconds, and the
//! answer is converted into the typed response payload the app-server
//! expects (never hand-rolled wire JSON). Unanswered requests auto-decline
//! so a turn can never hang on a silent chat.

use std::time::Duration;
use std::time::Instant;

use codex_app_server_protocol::{
    CommandExecutionApprovalDecision, CommandExecutionRequestApprovalResponse,
    FileChangeApprovalDecision, FileChangeRequestApprovalResponse, GrantedPermissionProfile,
    PermissionGrantScope, PermissionsRequestApprovalResponse, RequestId, ServerRequest,
};
use codex_gui_bridge::Client;
use serde_json::Value;

/// How long a WeChat contact has to answer before the auto-decline.
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// What the bridge tells the contact once the approval prompt is sent.
pub struct Answered {
    pub request_id: RequestId,
    pub payload: Value,
    pub reply: String,
    /// Text after `y` in `y <反馈>`; queued as the contact's next task.
    pub follow_up: Option<String>,
}

pub enum AnswerOutcome {
    /// The contact has no outstanding approval; the text is a normal task.
    NoPending,
    Answered(Answered),
}

/// One approval awaiting a WeChat decision.
struct Pending {
    request_id: RequestId,
    kind: Kind,
    contact: String,
    expires_at: Instant,
}

/// The request kinds the bridge forwards; protocol responses are typed, so
/// only the display-relevant details are kept here.
#[derive(Clone)]
enum Kind {
    Command {
        command: Option<String>,
        reason: Option<String>,
    },
    FileChange {
        reason: Option<String>,
    },
    Permissions {
        reason: Option<String>,
        requested: GrantedPermissionProfile,
    },
}

impl Kind {
    fn describe(&self) -> String {
        match self {
            Kind::Command { command, reason } => {
                let mut lines = vec![format!(
                    "命令: {}",
                    command.clone().unwrap_or_else(|| "(未知命令)".to_string())
                )];
                if let Some(reason) = reason {
                    lines.push(format!("原因: {reason}"));
                }
                lines.join("\n")
            }
            Kind::FileChange { reason } => match reason {
                Some(reason) => format!("文件修改\n原因: {reason}"),
                None => "文件修改".to_string(),
            },
            Kind::Permissions { reason, .. } => {
                let mut lines = vec!["权限提升请求".to_string()];
                if let Some(reason) = reason {
                    lines.push(format!("原因: {reason}"));
                }
                lines.join("\n")
            }
        }
    }
}

pub struct ApprovalBroker {
    pending: Vec<Pending>,
}

impl Default for ApprovalBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalBroker {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Normalizes a server request into a WeChat prompt. `None` for request
    /// kinds the bridge does not escalate (they are answered upstream).
    pub fn offered(&mut self, request: &ServerRequest, contact: &str) -> Option<String> {
        let (request_id, kind) = match request {
            ServerRequest::CommandExecutionRequestApproval { request_id, params } => (
                request_id.clone(),
                Kind::Command {
                    command: params.command.clone(),
                    reason: params.reason.clone(),
                },
            ),
            ServerRequest::FileChangeRequestApproval { request_id, params } => (
                request_id.clone(),
                Kind::FileChange {
                    reason: params.reason.clone(),
                },
            ),
            ServerRequest::PermissionsRequestApproval { request_id, params } => (
                request_id.clone(),
                Kind::Permissions {
                    reason: params.reason.clone(),
                    requested: GrantedPermissionProfile {
                        network: params.permissions.network.clone(),
                        file_system: params.permissions.file_system.clone(),
                    },
                },
            ),
            _other => return None,
        };
        let prompt = format!(
            "⚠️ codex 请求审批（{seconds} 秒内回复）\n{}\n\n回复「y」同意，「n」拒绝；「y 你的反馈」同意并附反馈。",
            kind.describe(),
            seconds = APPROVAL_TIMEOUT.as_secs()
        );
        self.pending.push(Pending {
            request_id,
            kind,
            contact: contact.to_string(),
            expires_at: Instant::now() + APPROVAL_TIMEOUT,
        });
        Some(prompt)
    }

    /// Drops entries the server resolved on its own (e.g. turn cancelled).
    pub fn resolved(&mut self, request_id: &RequestId) {
        self.pending
            .retain(|pending| pending.request_id != *request_id);
    }

    /// Interprets a contact's text as an approval answer for their oldest
    /// pending request.
    pub fn answer(&mut self, contact: &str, text: &str) -> AnswerOutcome {
        let trimmed = text.trim();
        let (decision, follow_up) = match trimmed.split_once([' ', '\n']) {
            Some(("y", feedback)) if !feedback.trim().is_empty() => {
                (Some(true), Some(feedback.trim().to_string()))
            }
            _ => match trimmed {
                "y" | "Y" | "yes" => (Some(true), None),
                "n" | "N" | "no" => (Some(false), None),
                _ => (None, None),
            },
        };
        let Some(accept) = decision else {
            return AnswerOutcome::NoPending;
        };
        let index = self
            .pending
            .iter()
            .position(|pending| pending.contact == contact);
        let Some(index) = index else {
            return AnswerOutcome::NoPending;
        };
        let pending = self.pending.remove(index);
        let payload = payload_for(&pending.kind, accept);
        AnswerOutcome::Answered(Answered {
            request_id: pending.request_id,
            payload,
            reply: if accept {
                "已同意该审批。".to_string()
            } else {
                "已拒绝该审批，任务将继续并知道该操作被拒绝。".to_string()
            },
            follow_up,
        })
    }

    /// Auto-declines every request expired as of `now`; returns the ids plus
    /// the WeChat notice per affected entry.
    pub fn expired_before(&mut self, now: Instant) -> Vec<(RequestId, Value, String, String)> {
        let (mut expired, keep): (Vec<Pending>, Vec<Pending>) = self
            .pending
            .drain(..)
            .partition(|pending| pending.expires_at <= now);
        self.pending = keep;
        expired
            .drain(..)
            .map(|pending| {
                (
                    pending.request_id,
                    payload_for(&pending.kind, false),
                    pending.contact,
                    "审批已超时，已自动拒绝。".to_string(),
                )
            })
            .collect()
    }

    /// Auto-declines every expired request (wall-clock variant).
    pub fn expired(&mut self) -> Vec<(RequestId, Value, String, String)> {
        self.expired_before(Instant::now())
    }

    pub fn has_pending(&self, contact: &str) -> bool {
        self.pending.iter().any(|p| p.contact == contact)
    }
}

/// Typed response payload for one decision; mirrors the GUI core's
/// `response_payload` without pulling in its heavier dependencies.
fn payload_for(kind: &Kind, accept: bool) -> Value {
    match kind {
        Kind::Command { .. } => serde_json::to_value(CommandExecutionRequestApprovalResponse {
            decision: if accept {
                CommandExecutionApprovalDecision::Accept
            } else {
                CommandExecutionApprovalDecision::Decline
            },
        })
        .unwrap_or(Value::Null),
        Kind::FileChange { .. } => serde_json::to_value(FileChangeRequestApprovalResponse {
            decision: if accept {
                FileChangeApprovalDecision::Accept
            } else {
                FileChangeApprovalDecision::Decline
            },
        })
        .unwrap_or(Value::Null),
        // Permissions are answered with the granted profile itself; the
        // default (empty) profile is the deny path.
        Kind::Permissions { requested, .. } => {
            serde_json::to_value(PermissionsRequestApprovalResponse {
                permissions: if accept {
                    requested.clone()
                } else {
                    GrantedPermissionProfile::default()
                },
                scope: PermissionGrantScope::Turn,
                strict_auto_review: None,
            })
            .unwrap_or(Value::Null)
        }
    }
}

/// Convenience for answering through the raw client handle.
pub async fn respond(client: &Client, answered: &Answered) {
    let _ = client
        .respond_for_id(answered.request_id.clone(), answered.payload.clone())
        .await;
}

#[cfg(test)]
#[path = "approval_tests.rs"]
mod tests;
