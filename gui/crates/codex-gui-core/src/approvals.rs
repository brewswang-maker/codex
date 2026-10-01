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
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// The smart approval policy applied before a dialog is ever shown.
///
/// Three tiers, checked in order: destructive commands are declined
/// without display, safe confirmations are accepted without display, and
/// everything else (permissions, anything the policy leaves un automated)
/// escalates to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// Auto-accept file-change (apply patch) approvals; patches are
    /// reviewable in the transcript and revertible via git.
    #[serde(default = "default_true")]
    pub auto_accept_file_changes: bool,
    /// Auto-accept plain command approvals whose command text carries no
    /// destructive pattern.
    #[serde(default = "default_true")]
    pub auto_accept_plain_commands: bool,
    /// Auto-decline commands matching a destructive `rm -rf` variant
    /// without ever showing a dialog.
    #[serde(default = "default_true")]
    pub deny_destructive_commands: bool,
}

fn default_true() -> bool {
    true
}

/// The user's home directory from the environment; POSIX sets `HOME`, and
/// `USERPROFILE` covers Windows.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            auto_accept_file_changes: true,
            auto_accept_plain_commands: true,
            deny_destructive_commands: true,
        }
    }
}

impl ApprovalPolicy {
    /// Loads `~/.codex/gui-policy.json`; any read or parse failure falls
    /// back to the built-in defaults so a corrupt file never blocks work.
    pub fn load_or_default() -> Self {
        let Some(home) = home_dir() else {
            return Self::default();
        };
        let path = home.join(".codex").join("gui-policy.json");
        Self::load_from(&path).unwrap_or_default()
    }

    /// Loads the policy from an explicit path (also the test seam).
    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Classifies a pending approval under this policy. `Some` means the
    /// request is resolved without showing a dialog; `None` escalates to
    /// the user (permissions always do: they widen the session's trust
    /// scope, which only the user may grant).
    pub fn classify(&self, kind: &ApprovalKind) -> Option<AutoDecision> {
        match kind {
            ApprovalKind::FileChange { .. } => self
                .auto_accept_file_changes
                .then_some(AutoDecision::Accept),
            ApprovalKind::CommandExecution { command, .. } => {
                let text = command.as_deref().unwrap_or("");
                if self.deny_destructive_commands && is_destructive_rm(text) {
                    return Some(AutoDecision::Decline);
                }
                self.auto_accept_plain_commands
                    .then_some(AutoDecision::Accept)
            }
            ApprovalKind::Permissions { .. } => None,
        }
    }
}

/// The decision the policy reached on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoDecision {
    /// Approve without showing a dialog.
    Accept,
    /// Decline without showing a dialog.
    Decline,
}

/// Detects a recursive-force `rm` anywhere in a command line, covering
/// `rm -rf /x`, `rm -fr /*`, `sudo rm -rf`, `/bin/rm -r -f`,
/// `sh -c "rm --recursive --force …"` and flag clusters like `-rnf`.
pub fn is_destructive_rm(command: &str) -> bool {
    // Drop quote characters so `sh -c "rm -rf /"` tokenizes the same as a
    // bare `rm -rf /`.
    let flattened: String = command
        .chars()
        .map(|c| {
            if matches!(c, '"' | '\'' | '`') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let tokens: Vec<&str> = flattened.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        let basename = token.rsplit('/').next().unwrap_or(token);
        if basename != "rm" {
            continue;
        }
        let mut recursive = false;
        let mut force = false;
        for next in tokens.iter().skip(index + 1) {
            if *next == "--recursive" {
                recursive = true;
                continue;
            }
            if *next == "--force" {
                force = true;
                continue;
            }
            if *next == "--" || !next.starts_with('-') {
                // End of the option block; later tokens are operands.
                break;
            }
            let flags = next.trim_start_matches('-');
            recursive = recursive || flags.contains('r');
            force = force || flags.contains('f');
        }
        if recursive && force {
            return true;
        }
    }
    false
}

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
