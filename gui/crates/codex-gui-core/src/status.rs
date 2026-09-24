//! Status bar domain: the model catalog, the login badge, and the error
//! banner.
//!
//! Each piece is a projection of one app-server response or notification;
//! the status bar renders whatever this board holds.

use codex_app_server_protocol::Account;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::ModelListResponse;

/// One model-picker row, projected from the catalog entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
}

/// Login state as the status bar shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AccountBadge {
    /// `account/read` has not answered yet.
    #[default]
    Unknown,
    /// No account is signed in.
    SignedOut,
    /// An API-key account; no user-facing identity.
    ApiKey,
    /// A ChatGPT account with its email and plan.
    Chatgpt { email: Option<String>, plan: String },
}

/// The current error banner; replaced by the next error or dismissed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorBanner {
    pub message: String,
    pub will_retry: bool,
}

/// Everything the status bar renders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusBoard {
    models: Vec<ModelEntry>,
    current_model: Option<String>,
    account: AccountBadge,
    error: Option<ErrorBanner>,
    /// Transient one-line notice (model reroutes, guardian reviews, ...).
    notice: Option<String>,
    /// Working directory of the active thread, as the server reported it.
    cwd: Option<String>,
}

impl StatusBoard {
    /// Replaces the model catalog; the first visible entry seeds the
    /// displayed model until the user picks one.
    pub fn apply_models(&mut self, response: &ModelListResponse) {
        self.models = response
            .data
            .iter()
            .map(|model| ModelEntry {
                id: model.id.clone(),
                display_name: model.display_name.clone(),
            })
            .collect();
        if self.current_model.is_none() {
            self.current_model = response
                .data
                .iter()
                .find(|model| !model.hidden)
                .map(|model| model.display_name.clone());
        }
    }

    /// Projects the login state from an `account/read` response.
    pub fn apply_account(&mut self, response: &GetAccountResponse) {
        self.account = match &response.account {
            Some(Account::Chatgpt { email, plan_type }) => AccountBadge::Chatgpt {
                email: email.clone(),
                plan: format!("{plan_type:?}").to_lowercase(),
            },
            Some(Account::ApiKey { .. }) => AccountBadge::ApiKey,
            _ => AccountBadge::SignedOut,
        };
    }

    /// Surfaces an `error` notification as the active banner.
    pub fn raise_error(&mut self, notification: &ErrorNotification) {
        self.error = Some(ErrorBanner {
            message: notification.error.message.clone(),
            will_retry: notification.will_retry,
        });
    }

    /// Surfaces a plain message as the active banner (warnings, auth
    /// recovery, deprecation notices).
    pub fn raise_banner(&mut self, message: String) {
        self.error = Some(ErrorBanner {
            message,
            will_retry: false,
        });
    }

    /// Replaces the transient status-bar notice; the next notice wins.
    pub fn apply_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    /// Writes a login badge projected outside the board (e.g. from an
    /// `account/updated` notification).
    pub fn set_account(&mut self, badge: AccountBadge) {
        self.account = badge;
    }

    /// Records the active thread's working directory from a `thread/start`
    /// or `thread/resume` response.
    pub fn apply_cwd(&mut self, cwd: String) {
        self.cwd = Some(cwd);
    }

    /// Drops the banner; returns whether one was showing.
    pub fn dismiss_error(&mut self) -> bool {
        self.error.take().is_some()
    }

    /// Records the user's model pick by catalog id; returns whether the id
    /// was in the catalog.
    pub fn select_model(&mut self, id: &str) -> bool {
        match self.models.iter().find(|model| model.id == id) {
            Some(model) => {
                self.current_model = Some(model.display_name.clone());
                true
            }
            None => false,
        }
    }

    /// Aligns the displayed model with the configured top-level `model`,
    /// so the status bar shows what turns actually use instead of the
    /// catalog seed. Catalog ids map to their display name; unknown ids
    /// (third-party models) show as-is.
    pub fn sync_model(&mut self, model: &str) {
        if model.is_empty() {
            return;
        }
        let name = self
            .models
            .iter()
            .find(|entry| entry.id == model)
            .map(|entry| entry.display_name.clone())
            .unwrap_or_else(|| model.to_string());
        self.current_model = Some(name);
    }

    /// Catalog rows in server order.
    pub fn models(&self) -> &[ModelEntry] {
        &self.models
    }

    /// The model name the status bar displays.
    pub fn current_model(&self) -> Option<&str> {
        self.current_model.as_deref()
    }

    /// The login badge.
    pub fn account(&self) -> &AccountBadge {
        &self.account
    }

    /// The active banner, if any.
    pub fn error(&self) -> Option<&ErrorBanner> {
        self.error.as_ref()
    }

    /// The transient notice, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The active thread's working directory, once known.
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }
}
