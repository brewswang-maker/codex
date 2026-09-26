//! Projects status-bearing notifications onto the [`StatusBoard`].
//!
//! Spec section 9: `account/updated`, `model/rerouted`, and
//! `model/verification` feed the status bar; `warning`, `guardianWarning`,
//! `configWarning`, `modelProvider/authRecovery*`, and `deprecationNotice`
//! surface as the dismissible banner; a `thread/settings/updated` for the
//! displayed thread re-syncs the model label; the experimental guardian
//! auto-review notifications are display-only.

use crate::state::State;
use codex_app_server_protocol::AuthMode;
use codex_app_server_protocol::ServerNotification;
use codex_gui_core::AccountBadge;

/// Applies the status-bearing slice of one notification; returns whether it
/// matched (transcript-driven notifications keep flowing to the transcript).
pub fn apply(state: &mut State, notification: &ServerNotification) -> bool {
    match notification {
        ServerNotification::AccountUpdated(updated) => {
            if let Some(badge) = project_account(updated) {
                state.status_board.set_account(badge);
            }
            true
        }
        ServerNotification::ModelRerouted(rerouted) => {
            state.status_board.apply_notice(format!(
                "model rerouted: {} -> {}",
                rerouted.from_model, rerouted.to_model
            ));
            true
        }
        ServerNotification::ModelVerification(verification) => {
            state.status_board.apply_notice(format!(
                "model verification: {} entries",
                verification.verifications.len()
            ));
            true
        }
        ServerNotification::Warning(warning) => {
            state.status_board.raise_banner(warning.message.clone());
            true
        }
        ServerNotification::GuardianWarning(warning) => {
            // The review pipeline also reports automatic approvals after the
            // fact; mirror the TUI and keep those out of the banner.
            if !warning
                .message
                .starts_with("Automatic approval review approved (")
            {
                state.status_board.raise_banner(warning.message.clone());
            }
            true
        }
        ServerNotification::ConfigWarning(warning) => {
            let message = match &warning.details {
                Some(details) => format!("{}: {details}", warning.summary),
                None => warning.summary.clone(),
            };
            state.status_board.raise_banner(message);
            true
        }
        ServerNotification::ThreadSettingsUpdated(updated) => {
            // Only the displayed thread's effective settings drive the
            // status bar; the notification also fires for background threads.
            if state.thread_id.as_deref() == Some(updated.thread_id.as_str()) {
                state
                    .status_board
                    .sync_model(&updated.thread_settings.model);
                // The server reports the thread's own pair, so the binding
                // tracks in-place model switches exactly.
                state.active_binding = Some((
                    updated.thread_settings.model_provider.clone(),
                    updated.thread_settings.model.clone(),
                ));
            }
            true
        }
        ServerNotification::AuthRecoveryStarted(recovery) => {
            state.status_board.raise_banner(format!(
                "authentication recovery started ({})",
                recovery.provider
            ));
            true
        }
        ServerNotification::AuthRecoveryCompleted(recovery) => {
            state.status_board.raise_banner(format!(
                "authentication recovery completed ({})",
                recovery.provider
            ));
            true
        }
        ServerNotification::DeprecationNotice(notice) => {
            state.status_board.raise_banner(notice.summary.clone());
            true
        }
        ServerNotification::ItemGuardianApprovalReviewStarted(_) => {
            state
                .status_board
                .apply_notice("guardian auto-review started".to_string());
            true
        }
        ServerNotification::ItemGuardianApprovalReviewCompleted(_) => {
            state
                .status_board
                .apply_notice("guardian auto-review completed".to_string());
            true
        }
        ServerNotification::StrictReviewRequired(_) => {
            state
                .status_board
                .apply_notice("guardian strict review required".to_string());
            true
        }
        _other => false,
    }
}

/// Projects an `account/updated` payload onto the login badge.
///
/// - No auth mode left means the account is signed out (mirrors the
///   `account/read` projection).
/// - Auth modes without a human-facing identity return `None` so a prior
///   `account/read` answer is not downgraded to a guess.
fn project_account(
    updated: &codex_app_server_protocol::AccountUpdatedNotification,
) -> Option<AccountBadge> {
    match updated.auth_mode {
        Some(AuthMode::ApiKey) => Some(AccountBadge::ApiKey),
        Some(AuthMode::Chatgpt) => Some(AccountBadge::Chatgpt {
            email: None,
            plan: updated
                .plan_type
                .map(|plan| format!("{plan:?}").to_lowercase())
                .unwrap_or_default(),
        }),
        None => Some(AccountBadge::SignedOut),
        Some(
            AuthMode::ChatgptAuthTokens
            | AuthMode::Headers
            | AuthMode::AgentIdentity
            | AuthMode::PersonalAccessToken
            | AuthMode::BedrockApiKey
            | AuthMode::BedrockAccessKeys,
        ) => None,
    }
}
