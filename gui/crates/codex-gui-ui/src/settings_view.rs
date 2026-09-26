//! The settings panel: drafts `config.toml` values through the config RPC,
//! signs in with an API key, and switches the model provider.

use crate::message::Message;
use crate::state::State;
use crate::theme::dim;
use codex_app_server_protocol::McpAuthStatus;
use codex_app_server_protocol::McpServerConnectionStatus;
use codex_app_server_protocol::SkillScope;
use codex_gui_core::PROVIDER_PRESETS;
use codex_gui_core::SettingField;
use codex_gui_core::options;
use codex_gui_core::provider_preset;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::pick_list;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the full-screen settings panel; the view swaps the whole chat
/// surface out for it while it is open.
pub fn panel(state: &State) -> Element<'_, Message> {
    let mut panel = column![
        row![
            text("Settings").size(20),
            Space::new().width(Fill),
            button(text("Close")).on_press(Message::SettingsToggled),
        ]
        .spacing(12),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if !state.settings.loaded {
        panel = panel.push(text("loading…"));
        return panel.into();
    }

    panel = panel.push(text("Account").size(14));
    panel = panel.push(account_row(state));
    panel = panel.push(api_key_row(state));

    panel = panel.push(text("Model provider").size(14));
    panel = panel.push(preset_row(state));
    if provider_id_editable(state) {
        panel = panel.push(field_row(state, SettingField::ProviderId, "provider id"));
    }
    for (field, label) in PROVIDER_FIELDS {
        panel = panel.push(field_row(state, *field, label));
    }

    panel = panel.push(text("General").size(14));
    for (field, label) in GENERAL_FIELDS {
        panel = panel.push(field_row(state, *field, label));
    }

    panel = panel.push(text("Skills").size(14));
    for skill_row in skill_rows(state) {
        panel = panel.push(skill_row);
    }

    panel = panel.push(text("MCP servers").size(14));
    for mcp_row in mcp_rows(state) {
        panel = panel.push(mcp_row);
    }
    if let Some(banner) = mcp_login_banner(state) {
        panel = panel.push(banner);
    }

    if let Some(notice) = &state.settings.notice {
        panel = panel.push(text(notice.clone()));
    }
    panel = panel.push(
        button(text("Save"))
            .padding([6, 16])
            .on_press(Message::SettingsSave),
    );

    scrollable(panel).width(Fill).height(Fill).into()
}

/// The signed-in badge plus the sign-out action.
fn account_row(state: &State) -> Element<'_, Message> {
    let badge = state
        .settings
        .signed_in_label
        .clone()
        .unwrap_or_else(|| "signed out".to_string());
    let mut account = row![
        text("account").width(160),
        text(badge),
        Space::new().width(Fill),
    ]
    .spacing(12);
    if state.settings.signed_in_label.is_some() {
        account = account.push(button(text("sign out")).on_press(Message::LogoutSubmit));
    }
    account.into()
}

/// The API-key entry feeding `account/login/start {type: apiKey}`.
fn api_key_row(state: &State) -> Element<'_, Message> {
    row![
        text("API key").width(160),
        text_input("paste your API key", &state.settings.api_key_draft)
            .secure(true)
            .on_input(Message::AccountApiKeyChanged)
            .width(Fill),
        button(text("sign in")).on_press(Message::LoginSubmit),
    ]
    .spacing(12)
    .into()
}

/// The provider preset picker over the built-in vendor list, plus the
/// billing-type picker for vendors exposing several endpoint types.
fn preset_row(state: &State) -> Element<'_, Message> {
    let names: Vec<String> = PROVIDER_PRESETS
        .iter()
        .map(|preset| preset.name.to_string())
        .collect();
    let selected =
        provider_preset(&state.settings.provider.id).map(|preset| preset.name.to_string());
    let mut picker = row![
        text("provider preset").width(160),
        pick_list(names, selected, |name| {
            Message::ProviderPresetSelected(preset_id(&name))
        })
        .width(Fill),
    ]
    .spacing(12);
    if let Some(billing) = billing_row(state) {
        picker = picker.push(billing);
    }
    picker.into()
}

/// The billing picker; shown only when the drafted vendor has several
/// endpoint types, each resolving to its own base URL and model.
fn billing_row(state: &State) -> Option<Element<'_, Message>> {
    let preset = provider_preset(&state.settings.provider.id)?;
    if preset.entries.len() < 2 {
        return None;
    }
    let labels: Vec<String> = preset
        .entries
        .iter()
        .map(|entry| entry.billing.label().to_string())
        .collect();
    let selected = state
        .settings
        .billing
        .map(|billing| billing.label().to_string());
    Some(
        row![
            text("billing type").width(160),
            pick_list(labels, selected, Message::ProviderBillingSelected).width(Fill),
        ]
        .spacing(12)
        .into(),
    )
}

/// Maps a display name back to its preset id.
fn preset_id(name: &str) -> String {
    PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.name == name)
        .map(|preset| preset.id.to_string())
        .unwrap_or_default()
}

/// The provider-id row only shows when the id is not a built-in preset;
/// custom providers need an explicit table key before anything is written.
fn provider_id_editable(state: &State) -> bool {
    state.settings.provider.id.is_empty() || provider_preset(&state.settings.provider.id).is_none()
}

/// The provider form fields in panel order, with their display labels.
const PROVIDER_FIELDS: &[(SettingField, &str)] = &[
    (SettingField::ProviderBaseUrl, "base URL"),
    (SettingField::ProviderModel, "provider model"),
    (SettingField::ProviderApiKey, "provider API key"),
];

/// The top-level config fields in panel order, with their display labels.
const GENERAL_FIELDS: &[(SettingField, &str)] = &[
    (SettingField::Model, "model"),
    (SettingField::ApprovalPolicy, "approval policy"),
    (SettingField::SandboxMode, "sandbox mode"),
    (SettingField::ReasoningEffort, "reasoning effort"),
    (SettingField::Verbosity, "verbosity"),
    (SettingField::WebSearch, "web search"),
];

/// One toggle per discovered skill; flips write straight through
/// `skills/config/write` without the draft/save cycle.
fn skill_rows(state: &State) -> Vec<Element<'_, Message>> {
    if state.skills.rows.is_empty() {
        return vec![text("no skills discovered").style(dim).into()];
    }
    state
        .skills
        .rows
        .iter()
        .map(|skill| {
            row![
                checkbox(skill.enabled)
                    .label(format!("${}", skill.name))
                    .on_toggle(move |toggled| Message::SkillEnabledChanged {
                        name: skill.name.clone(),
                        enabled: toggled,
                    }),
                text(scope_label(skill.scope)).style(dim).width(60),
                text(skill.description.clone()).style(dim).width(Fill),
            ]
            .spacing(12)
            .into()
        })
        .collect()
}

/// The wire scope as a display label.
fn scope_label(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::User => "user",
        SkillScope::Repo => "repo",
        SkillScope::System => "system",
        SkillScope::Admin => "admin",
    }
}

/// The MCP inventory rows (name, connection state, auth state, tool
/// count) plus the sign-in action for servers stuck on authentication;
/// the list loads when the panel opens.
fn mcp_rows(state: &State) -> Vec<Element<'_, Message>> {
    if !state.mcp_loaded {
        return vec![text("loading…").style(dim).into()];
    }
    if state.mcp_servers.is_empty() {
        return vec![text("no MCP servers configured").style(dim).into()];
    }
    state
        .mcp_servers
        .iter()
        .map(|server| {
            let needs_login = matches!(server.auth_status, McpAuthStatus::NotLoggedIn)
                || matches!(
                    server.runtime_status.as_ref(),
                    Some(McpServerConnectionStatus::AuthenticationRequired)
                );
            let mut line = row![
                text(server.name.clone()).width(160),
                text(mcp_connection_label(server.runtime_status.as_ref()))
                    .style(dim)
                    .width(120),
                text(mcp_auth_label(&server.auth_status))
                    .style(dim)
                    .width(90),
                text(format!("{} tools", server.tools.len()))
                    .style(dim)
                    .width(Fill),
            ]
            .spacing(12);
            if needs_login {
                line = line.push(
                    button(text("sign in"))
                        .on_press(Message::McpLoginRequested(server.name.clone())),
                );
            }
            line.into()
        })
        .collect()
}

/// The pending OAuth banner: the authorization URL, a copy action, and a
/// dismiss action; it clears itself once the login-completed
/// notification arrives.
fn mcp_login_banner(state: &State) -> Option<Element<'_, Message>> {
    let login = state.mcp_login.as_ref()?;
    Some(
        column![
            text(format!(
                "sign in to {}: open this link in a browser",
                login.name
            )),
            text(login.url.clone()).style(dim),
            row![
                button(text("copy link")).on_press(Message::McpLoginLinkCopied),
                button(text("dismiss")).on_press(Message::McpLoginDismissed),
            ]
            .spacing(8),
        ]
        .spacing(6)
        .into(),
    )
}

/// The MCP connection state as a display label.
fn mcp_connection_label(status: Option<&McpServerConnectionStatus>) -> &'static str {
    match status {
        Some(McpServerConnectionStatus::Connected) => "connected",
        Some(McpServerConnectionStatus::Starting) => "starting…",
        Some(McpServerConnectionStatus::NotStarted) => "not started",
        Some(McpServerConnectionStatus::AuthenticationRequired) => "auth required",
        Some(McpServerConnectionStatus::Failed) => "failed",
        Some(McpServerConnectionStatus::Cancelled) => "cancelled",
        Some(McpServerConnectionStatus::Disabled) => "disabled",
        None => "…",
    }
}

/// The MCP auth state as a display label.
fn mcp_auth_label(status: &McpAuthStatus) -> &'static str {
    match status {
        McpAuthStatus::Unknown => "…",
        McpAuthStatus::Unsupported => "-",
        McpAuthStatus::NotLoggedIn => "not logged in",
        McpAuthStatus::BearerToken => "bearer token",
        McpAuthStatus::OAuth => "oauth",
    }
}

/// One labeled editor row: a pick list for fields with fixed candidates,
/// free text for everything else. The provider key renders masked.
fn field_row<'a>(
    state: &'a State,
    field: SettingField,
    label: &'static str,
) -> Element<'a, Message> {
    let value = state.settings.value(field);
    let secure = field == SettingField::ProviderApiKey;
    let editor: Element<'_, Message> = if options(field).is_empty() {
        text_input(label, value)
            .secure(secure)
            .on_input(move |updated| Message::SettingEdited {
                field,
                value: updated,
            })
            .padding(6)
            .width(Fill)
            .into()
    } else {
        let selected = if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        };
        pick_list(options(field), selected, move |updated| {
            Message::SettingEdited {
                field,
                value: updated,
            }
        })
        .width(Fill)
        .into()
    };

    row![text(label).width(160), editor].spacing(12).into()
}
