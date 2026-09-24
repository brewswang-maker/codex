#![allow(clippy::expect_used, clippy::panic)]

use super::status::AccountBadge;
use super::status::StatusBoard;
use codex_app_server_protocol::ErrorNotification;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::ModelListResponse;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;

fn models_response() -> ModelListResponse {
    from_value(json!({
        "data": [
            {
                "id": "mock-model-1",
                "model": "mock-model",
                "upgrade": null,
                "upgradeInfo": null,
                "availabilityNux": null,
                "displayName": "Mock Model",
                "description": "The mock catalog entry",
                "hidden": false,
                "supportedReasoningEfforts": [],
                "defaultReasoningEffort": "medium",
                "multiAgentVersion": null,
                "isDefault": true
            },
            {
                "id": "hidden-model",
                "model": "hidden",
                "upgrade": null,
                "upgradeInfo": null,
                "availabilityNux": null,
                "displayName": "Hidden Model",
                "description": "",
                "hidden": true,
                "supportedReasoningEfforts": [],
                "defaultReasoningEffort": "medium",
                "multiAgentVersion": null,
                "isDefault": false
            }
        ],
        "nextCursor": null
    }))
    .expect("model list decodes")
}

#[test]
fn apply_models_projects_entries_and_seeds_default_display() {
    let mut board = StatusBoard::default();
    board.apply_models(&models_response());

    let names: Vec<_> = board
        .models()
        .iter()
        .map(|model| model.display_name.as_str())
        .collect();
    assert_eq!(names, ["Mock Model", "Hidden Model"]);
    // The first non-hidden entry seeds the status bar.
    assert_eq!(board.current_model(), Some("Mock Model"));
}

#[test]
fn apply_models_keeps_user_selection_across_refreshes() {
    let mut board = StatusBoard::default();
    board.apply_models(&models_response());
    board.select_model("hidden-model");
    board.apply_models(&models_response());

    assert_eq!(board.current_model(), Some("Hidden Model"));
}

fn account_response(account: serde_json::Value) -> GetAccountResponse {
    from_value(json!({
        "account": account,
        "requiresOpenaiAuth": false,
        "workspaceRouting": null
    }))
    .expect("account response decodes")
}

#[test]
fn apply_account_projects_chatgpt_badge() {
    let mut board = StatusBoard::default();
    board.apply_account(&account_response(json!({
        "type": "chatgpt",
        "email": "dev@example.com",
        "planType": "pro"
    })));

    assert_eq!(
        board.account(),
        &AccountBadge::Chatgpt {
            email: Some("dev@example.com".to_string()),
            plan: "pro".to_string(),
        }
    );
}

#[test]
fn apply_account_maps_api_key_and_absence() {
    let mut board = StatusBoard::default();
    board.apply_account(&account_response(json!({"type": "apiKey"})));
    assert_eq!(board.account(), &AccountBadge::ApiKey);

    board.apply_account(&account_response(json!(null)));
    assert_eq!(board.account(), &AccountBadge::SignedOut);
}

fn error_notification(message: &str, will_retry: bool) -> ErrorNotification {
    from_value(json!({
        "error": {"message": message, "codexErrorInfo": null},
        "willRetry": will_retry,
        "threadId": "thread-1",
        "turnId": "turn-1"
    }))
    .expect("error notification decodes")
}

#[test]
fn error_banner_replaces_and_dismisses() {
    let mut board = StatusBoard::default();

    board.raise_error(&error_notification("first failure", true));
    assert_eq!(
        board.error().map(|banner| banner.message.as_str()),
        Some("first failure")
    );
    assert!(board.error().is_some_and(|banner| banner.will_retry));

    board.raise_error(&error_notification("second failure", false));
    assert_eq!(
        board.error().map(|banner| banner.message.as_str()),
        Some("second failure")
    );

    assert!(board.dismiss_error());
    assert!(!board.dismiss_error(), "no banner left to dismiss");
    assert!(board.error().is_none());
}

#[test]
fn notice_is_replaced_by_the_next_one() {
    let mut board = StatusBoard::default();

    assert!(board.notice().is_none());

    board.apply_notice("model rerouted: a -> b".to_string());
    assert_eq!(board.notice(), Some("model rerouted: a -> b"));

    board.apply_notice("guardian review started".to_string());
    assert_eq!(board.notice(), Some("guardian review started"));
}

#[test]
fn banner_helper_shows_plain_messages_without_retry() {
    let mut board = StatusBoard::default();

    board.raise_banner("authentication recovery started".to_string());

    let banner = board.error().expect("banner is up");
    assert_eq!(banner.message, "authentication recovery started");
    assert!(!banner.will_retry);
}

#[test]
fn set_account_writes_projected_badges() {
    let mut board = StatusBoard::default();

    board.set_account(AccountBadge::ApiKey);
    assert_eq!(board.account(), &AccountBadge::ApiKey);

    board.set_account(AccountBadge::SignedOut);
    assert_eq!(board.account(), &AccountBadge::SignedOut);
}

#[test]
fn sync_model_mirrors_the_configured_model() {
    let mut board = StatusBoard::default();
    board.apply_models(&models_response());

    // A catalog id shows its display name...
    board.sync_model("mock-model-1");
    assert_eq!(board.current_model(), Some("Mock Model"));

    // ...a third-party id shows as-is, replacing the catalog seed...
    board.sync_model("glm-5.3");
    assert_eq!(board.current_model(), Some("glm-5.3"));

    // ...and an empty config keeps whatever is displayed.
    board.sync_model("");
    assert_eq!(board.current_model(), Some("glm-5.3"));
}
