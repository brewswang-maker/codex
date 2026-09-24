//! Pending approval requests and their decisions.
//!
//! The app-server escalates risky actions through three server-initiated
//! request kinds (`item/commandExecution/requestApproval`,
//! `item/fileChange/requestApproval`, `item/permissions/requestApproval`).
//! Each must eventually be answered with a typed response payload; this
//! module tracks the outstanding ones and builds those payloads so the UI
//! layer never hand-rolls wire JSON.

use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::GrantedPermissionProfile;
use codex_app_server_protocol::PermissionGrantScope;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use serde_json::Value;

/// One outstanding server request awaiting a user decision.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingApproval {
    /// Wire id used both to answer the request and to match the
    /// `serverRequest/resolved` notification.
    pub request_id: RequestId,
    /// What is being approved, with the details the dialog displays.
    pub kind: ApprovalKind,
}

/// The three request kinds the GUI escalates to a dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalKind {
    /// A command execution (or stdin write) about to run.
    CommandExecution {
        command: Option<String>,
        reason: Option<String>,
    },
    /// A file change about to be applied.
    FileChange { reason: Option<String> },
    /// Additional permissions requested for the session/turn.
    Permissions {
        reason: Option<String>,
        /// The requested profile; granted verbatim on accept.
        requested: GrantedPermissionProfile,
    },
}

/// The decisions a dialog offers for command/file approvals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Approve once.
    Accept,
    /// Approve for the remainder of the session.
    AcceptForSession,
    /// Deny; the agent continues the turn.
    Decline,
    /// Deny and interrupt the turn.
    Cancel,
}

/// Queue of pending approvals, answered strictly in arrival order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Approvals {
    pending: Vec<PendingApproval>,
}

impl Approvals {
    /// Outstanding approvals, oldest first; the dialog shows the head.
    pub fn pending(&self) -> &[PendingApproval] {
        &self.pending
    }

    /// Normalizes a server request into the queue; returns `None` for
    /// request kinds the GUI does not escalate.
    pub fn offered(&mut self, request: &ServerRequest) -> Option<PendingApproval> {
        let pending = match request {
            ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                PendingApproval {
                    request_id: request_id.clone(),
                    kind: ApprovalKind::CommandExecution {
                        command: params.command.clone(),
                        reason: params.reason.clone(),
                    },
                }
            }
            ServerRequest::FileChangeRequestApproval { request_id, params } => PendingApproval {
                request_id: request_id.clone(),
                kind: ApprovalKind::FileChange {
                    reason: params.reason.clone(),
                },
            },
            ServerRequest::PermissionsRequestApproval { request_id, params } => PendingApproval {
                request_id: request_id.clone(),
                kind: ApprovalKind::Permissions {
                    reason: params.reason.clone(),
                    requested: GrantedPermissionProfile {
                        network: params.permissions.network.clone(),
                        file_system: params.permissions.file_system.clone(),
                    },
                },
            },
            _other => return None,
        };

        self.pending.push(pending.clone());
        Some(pending)
    }

    /// Drops the entry matched by a `serverRequest/resolved` notification;
    /// the server answered the request itself (e.g. turn was cancelled).
    pub fn resolved(&mut self, request_id: &RequestId) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|approval| approval.request_id != *request_id);
        self.pending.len() != before
    }

    /// Takes the entry out of the queue so a decision can be sent.
    pub fn take(&mut self, request_id: &str) -> Option<PendingApproval> {
        let index = self
            .pending
            .iter()
            .position(|approval| approval.request_id.to_string() == request_id)?;
        Some(self.pending.remove(index))
    }

    /// Builds the wire payload answering the given approval. The `to_value`
    /// hops are invariants of the protocol response types.
    #[allow(clippy::expect_used)]
    pub fn response_payload(approval: &PendingApproval, decision: Decision) -> Value {
        match &approval.kind {
            ApprovalKind::CommandExecution { .. } => {
                serde_json::to_value(CommandExecutionRequestApprovalResponse {
                    decision: match decision {
                        Decision::Accept => CommandExecutionApprovalDecision::Accept,
                        Decision::AcceptForSession => {
                            CommandExecutionApprovalDecision::AcceptForSession
                        }
                        Decision::Decline => CommandExecutionApprovalDecision::Decline,
                        Decision::Cancel => CommandExecutionApprovalDecision::Cancel,
                    },
                })
                .expect("command approval response serializes")
            }
            ApprovalKind::FileChange { .. } => {
                serde_json::to_value(FileChangeRequestApprovalResponse {
                    decision: match decision {
                        Decision::Accept => FileChangeApprovalDecision::Accept,
                        Decision::AcceptForSession => FileChangeApprovalDecision::AcceptForSession,
                        Decision::Decline => FileChangeApprovalDecision::Decline,
                        Decision::Cancel => FileChangeApprovalDecision::Cancel,
                    },
                })
                .expect("file change approval response serializes")
            }
            // Permissions requests are answered with the granted profile
            // itself; an empty profile is the "deny" path.
            ApprovalKind::Permissions { requested, .. } => {
                serde_json::to_value(PermissionsRequestApprovalResponse {
                    permissions: if decision.accepts() {
                        requested.clone()
                    } else {
                        GrantedPermissionProfile::default()
                    },
                    scope: PermissionGrantScope::Turn,
                    strict_auto_review: None,
                })
                .expect("permissions approval response serializes")
            }
        }
    }
}

impl Decision {
    /// Whether the decision grants what was asked.
    pub fn accepts(self) -> bool {
        matches!(self, Decision::Accept | Decision::AcceptForSession)
    }
}
