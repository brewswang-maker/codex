//! Tests for the settings panel state machine.
#![allow(clippy::expect_used, clippy::panic)]

use super::Billing;
use super::SettingField;
use super::Settings;
use codex_app_server_protocol::GetAccountResponse;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn apply_config_seeds_the_draft_from_snake_case_keys() {
    let mut settings = Settings::default();
    settings.apply_config(&json!({
        "config": {
            "model": "gpt-5",
            "approval_policy": "on-request",
            "model_reasoning_effort": "high"
        },
        "origins": {}
    }));

    assert_eq!(
        settings.draft,
        super::SettingsDraft {
            model: "gpt-5".to_string(),
            approval_policy: "on-request".to_string(),
            sandbox_mode: String::new(),
            reasoning_effort: "high".to_string(),
            verbosity: String::new(),
            web_search: String::new(),
        }
    );
    assert!(settings.loaded);
    // Editing a value the server did not set keeps the draft empty.
    assert_eq!(settings.value(SettingField::WebSearch), "");
}

#[test]
fn edits_cover_only_changed_non_empty_fields() {
    let mut settings = Settings::default();
    settings.apply_config(&json!({"config": {"model": "gpt-5", "web_search": "cached"}}));
    settings.set_field(SettingField::Model, "gpt-5-mini".to_string());
    settings.set_field(SettingField::Verbosity, "low".to_string());
    // Unchanged and unset drafts never produce writes.
    settings.set_field(SettingField::WebSearch, "cached".to_string());
    settings.set_field(SettingField::SandboxMode, String::new());

    assert_eq!(
        settings.edits(),
        vec![
            ("model".to_string(), json!("gpt-5-mini")),
            ("model_verbosity".to_string(), json!("low")),
        ]
    );
}

#[test]
fn provider_preset_seeds_the_form_and_edits_aggregate_the_table() {
    let mut settings = Settings::default();
    settings.apply_config(&json!({"config": {"model": "gpt-5"}}));

    settings.select_provider_preset("deepseek");
    assert_eq!(settings.provider.id, "deepseek");
    assert_eq!(settings.provider.base_url, "https://api.deepseek.com");
    assert_eq!(settings.provider.model, "deepseek-v4-flash");

    settings.set_field(SettingField::ProviderApiKey, "sk-test".to_string());
    assert_eq!(
        settings.edits(),
        vec![
            (
                "model_providers.deepseek".to_string(),
                json!({
                    "name": "DeepSeek",
                    "base_url": "https://api.deepseek.com",
                    "experimental_bearer_token": "sk-test"
                })
            ),
            ("model_provider".to_string(), json!("deepseek")),
            ("model".to_string(), json!("deepseek-v4-flash")),
        ]
    );
    // Provider saves need a fresh conversation thread.
    assert!(settings.touches_model_setup());
}

#[test]
fn custom_provider_without_an_id_produces_no_edits() {
    let mut settings = Settings::default();
    settings.apply_config(&json!({"config": {}}));
    settings.select_provider_preset("");
    settings.set_field(
        SettingField::ProviderBaseUrl,
        "https://x.example".to_string(),
    );

    // The custom preset needs an explicit id before anything can be written.
    assert_eq!(settings.edits(), vec![]);

    // Typing the id on the provider-id field unblocks the table write.
    settings.set_field(SettingField::ProviderId, "acme".to_string());
    assert_eq!(
        settings.edits(),
        vec![
            (
                "model_providers.acme".to_string(),
                json!({"name": "acme", "base_url": "https://x.example"})
            ),
            ("model_provider".to_string(), json!("acme")),
        ]
    );
}

#[test]
fn billing_pick_reseeds_the_provider_endpoint() {
    let mut settings = Settings::default();
    settings.apply_config(&json!({"config": {}}));

    settings.select_provider_preset("kimi");
    assert_eq!(settings.provider.base_url, "https://api.moonshot.cn/v1");
    assert_eq!(settings.billing, Some(Billing::PayAsYouGo));

    // The coding plan resolves to its dedicated endpoint and model.
    settings.select_provider_billing(Billing::CodingPlan);
    assert_eq!(settings.provider.base_url, "https://api.kimi.com/coding/v1");
    assert_eq!(settings.provider.model, "kimi-for-coding");
    assert_eq!(settings.billing, Some(Billing::CodingPlan));

    // Vendors without the picked billing type keep their current form.
    settings.select_provider_preset("deepseek");
    settings.select_provider_billing(Billing::CodingPlan);
    assert_eq!(settings.provider.base_url, "https://api.deepseek.com");
    assert_eq!(settings.billing, Some(Billing::PayAsYouGo));

    // GLM plans share one endpoint by official design; only the key swaps.
    settings.select_provider_preset("glm");
    settings.select_provider_billing(Billing::CodingPlan);
    assert_eq!(
        settings.provider.base_url,
        "https://open.bigmodel.cn/api/v1"
    );
    assert_eq!(settings.provider.model, "glm-5.3");
    assert_eq!(settings.billing, Some(Billing::CodingPlan));
    assert_eq!(
        settings.notice.as_deref(),
        Some("Same endpoint; the billing type only changes the API key")
    );

    // The loaded config derives the drafted billing type.
    settings.apply_config(&json!({"config": {
        "model": "kimi-for-coding",
        "model_provider": "kimi",
        "model_providers": {"kimi": {"base_url": "https://api.kimi.com/coding/v1"}}
    }}));
    assert_eq!(settings.billing, Some(Billing::CodingPlan));

    // An untouched draft never forces a new thread.
    assert!(!settings.touches_model_setup());
}

#[test]
fn account_read_seeds_the_signed_in_label() {
    let mut settings = Settings::default();

    let api_key: GetAccountResponse =
        serde_json::from_value(json!({"account": {"type": "apiKey"}, "requiresOpenaiAuth": false}))
            .expect("apiKey account decodes");
    settings.apply_account(&api_key);
    assert_eq!(settings.signed_in_label.as_deref(), Some("API key"));

    let chatgpt: GetAccountResponse = serde_json::from_value(json!({
        "account": {"type": "chatgpt", "email": "a@b.c", "planType": "pro"},
        "requiresOpenaiAuth": false
    }))
    .expect("chatgpt account decodes");
    settings.apply_account(&chatgpt);
    assert_eq!(settings.signed_in_label.as_deref(), Some("a@b.c"));

    let signed_out: GetAccountResponse =
        serde_json::from_value(json!({"account": null, "requiresOpenaiAuth": false}))
            .expect("signed-out account decodes");
    settings.apply_account(&signed_out);
    assert_eq!(settings.signed_in_label, None);
}

#[test]
fn login_outcome_updates_the_badge_and_notice() {
    let mut settings = Settings::default();

    settings.apply_login_outcome(Ok(true));
    assert_eq!(settings.signed_in_label.as_deref(), Some("API key"));
    assert_eq!(settings.notice.as_deref(), Some("Signed in with API key"));

    settings.apply_login_outcome(Ok(false));
    assert_eq!(settings.signed_in_label, None);
    assert_eq!(settings.notice.as_deref(), Some("Signed out"));

    settings.apply_login_outcome(Err("invalid key".to_string()));
    assert_eq!(
        settings.notice.as_deref(),
        Some("Login failed: invalid key")
    );
}

#[test]
fn save_outcome_lands_in_the_notice() {
    let mut settings = Settings::default();
    settings.apply_saved(Ok(()));
    assert_eq!(settings.notice.as_deref(), Some("Saved to config.toml"));

    settings.apply_saved(Err("layer is read-only".to_string()));
    assert_eq!(
        settings.notice.as_deref(),
        Some("Save failed: layer is read-only")
    );
}

#[test]
fn toggle_reports_opening_and_resets_the_notice() {
    let mut settings = Settings {
        notice: Some("Saved to config.toml".to_string()),
        ..Settings::default()
    };

    assert!(settings.toggle());
    assert!(settings.open);
    // A fresh open starts without a stale save notice.
    assert_eq!(settings.notice, None);

    assert!(!settings.toggle());
    assert!(!settings.open);
}
