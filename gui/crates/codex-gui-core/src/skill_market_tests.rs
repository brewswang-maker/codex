//! Headless coverage for the market tab.
#![allow(clippy::expect_used)]

use crate::skill_market::MarketDetail;
use crate::skill_market::MarketPlugin;
use crate::skill_market::MarketSelector;
use crate::skill_market::MarketSkill;
use crate::skill_market::SkillMarket;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginReadResponse;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;
use std::path::PathBuf;

/// A wire-shaped `plugin/list` payload: one local marketplace with an
/// uninstalled git plugin and an installed local one, plus a remote-only
/// marketplace.
fn list_response() -> PluginListResponse {
    from_value(json!({
        "marketplaces": [
            {
                "name": "local-market",
                "path": "/tmp/market/local-market.json",
                "interface": {"displayName": "Local"},
                "plugins": [
                    plugin_json(
                        "pdf-tools@local-market",
                        "pdf-tools",
                        json!({"type": "git", "url": "https://github.com/example/pdf-tools.git", "path": null, "refName": null, "sha": null}),
                        false,
                        false,
                    ),
                    plugin_json(
                        "office-kit@local-market",
                        "office-kit",
                        json!({"type": "local", "path": "/tmp/market/office-kit"}),
                        true,
                        true,
                    ),
                ]
            },
            {
                "name": "official",
                "path": null,
                "interface": null,
                "plugins": [
                    plugin_json(
                        "writer@official",
                        "writer",
                        json!({"type": "remote"}),
                        false,
                        false,
                    )
                ]
            }
        ],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": []
    }))
    .expect("the wire payload decodes")
}

fn plugin_json(
    id: &str,
    name: &str,
    source: serde_json::Value,
    installed: bool,
    enabled: bool,
) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "source": source,
        "installed": installed,
        "enabled": enabled,
        "installPolicy": "AVAILABLE",
        "authPolicy": "ON_INSTALL",
        "interface": {
            "displayName": name,
            "shortDescription": format!("{name} 提示"),
            "longDescription": null,
            "developerName": null,
            "category": null,
            "capabilities": [],
            "websiteUrl": null,
            "privacyPolicyUrl": null,
            "termsOfServiceUrl": null,
            "defaultPrompt": null,
            "brandColor": null,
            "composerIcon": null,
            "composerIconUrl": null,
            "logo": null,
            "logoDark": null,
            "logoUrl": null,
            "logoUrlDark": null,
            "screenshots": [],
            "screenshotUrls": []
        }
    })
}

#[test]
fn apply_list_flattens_marketplaces_and_keeps_selectors() {
    let mut market = SkillMarket::default();

    market.apply_list(&list_response());

    assert!(market.loaded);
    assert_eq!(
        market.plugins,
        vec![
            MarketPlugin {
                id: "pdf-tools@local-market".to_string(),
                name: "pdf-tools".to_string(),
                wire_name: "pdf-tools".to_string(),
                marketplace: "local-market".to_string(),
                marketplace_path: Some(PathBuf::from("/tmp/market/local-market.json")),
                description: Some("pdf-tools 提示".to_string()),
                source_label: "Git".to_string(),
                installed: false,
                enabled: false,
            },
            MarketPlugin {
                id: "office-kit@local-market".to_string(),
                name: "office-kit".to_string(),
                wire_name: "office-kit".to_string(),
                marketplace: "local-market".to_string(),
                marketplace_path: Some(PathBuf::from("/tmp/market/local-market.json")),
                description: Some("office-kit 提示".to_string()),
                source_label: "本地".to_string(),
                installed: true,
                enabled: true,
            },
            MarketPlugin {
                id: "writer@official".to_string(),
                name: "writer".to_string(),
                wire_name: "writer".to_string(),
                marketplace: "official".to_string(),
                marketplace_path: None,
                description: Some("writer 提示".to_string()),
                source_label: "远程".to_string(),
                installed: false,
                enabled: false,
            },
        ]
    );

    let local = market.plugins[0].marketplace_selector();
    assert_eq!(
        local,
        MarketSelector::Path(PathBuf::from("/tmp/market/local-market.json"))
    );
    let remote = market.plugins[2].marketplace_selector();
    assert_eq!(remote, MarketSelector::RemoteName("official".to_string()));
}

#[test]
fn apply_list_keeps_the_wire_name_for_localized_rows() {
    let mut market = SkillMarket::default();
    let response: PluginListResponse = from_value(json!({
        "marketplaces": [
            {
                "name": "openai-api-curated",
                "path": "/tmp/market/api_marketplace.json",
                "interface": null,
                "plugins": [
                    plugin_json(
                        "superpowers@openai-api-curated",
                        "superpowers",
                        json!({"type": "local", "path": "/tmp/market/superpowers"}),
                        false,
                        false,
                    )
                ]
            }
        ],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": []
    }))
    .expect("the wire payload decodes");

    market.apply_list(&response);

    let plugin = &market.plugins[0];
    // The row renders the Chinese display copy, but `plugin/read` and
    // `plugin/install` must keep addressing the raw manifest name.
    assert_eq!(plugin.name, "Superpowers");
    assert_eq!(plugin.wire_name, "superpowers");
    assert_eq!(plugin.description.as_deref(), Some("规划、开发和调试代码"));
}

#[test]
fn toggle_detail_swaps_between_rows_and_collapses() {
    let mut market = SkillMarket::default();
    market.apply_list(&list_response());

    market.toggle_detail("pdf-tools@local-market");
    assert_eq!(
        market.detail_loading.as_deref(),
        Some("pdf-tools@local-market")
    );

    market.apply_detail(&read_response("pdf-tools@local-market"));
    assert_eq!(market.detail_loading, None);
    assert_eq!(
        market.detail,
        Some(MarketDetail {
            plugin_id: "pdf-tools@local-market".to_string(),
            skills: vec![
                MarketSkill {
                    name: "pdf-split".to_string(),
                    description: "split pdfs".to_string(),
                },
                MarketSkill {
                    name: "pdf-merge".to_string(),
                    description: "merge pdfs".to_string(),
                },
            ],
        })
    );

    market.toggle_detail("pdf-tools@local-market");
    assert_eq!(market.detail, None, "the same row collapses");
    market.detail_loading = Some("writer@official".to_string());
    market.apply_detail(&read_response("writer@official"));
    assert_eq!(
        market
            .detail
            .as_ref()
            .map(|detail| detail.plugin_id.clone()),
        Some("writer@official".to_string())
    );
}

#[test]
fn stale_detail_responses_are_dropped() {
    let mut market = SkillMarket::default();
    market.apply_list(&list_response());
    market.toggle_detail("writer@official");

    market.apply_detail(&read_response("pdf-tools@local-market"));

    assert_eq!(market.detail, None, "the stale detail never lands");
    assert_eq!(market.detail_loading.as_deref(), Some("writer@official"));
}

#[test]
fn installed_and_uninstalled_mirror_onto_the_row() {
    let mut market = SkillMarket::default();
    market.apply_list(&list_response());
    market.toggle_detail("pdf-tools@local-market");
    market.apply_detail(&read_response("pdf-tools@local-market"));

    market.apply_installed("pdf-tools@local-market");
    let row = &market.plugins[0];
    assert!(row.installed && row.enabled, "the install flips the row");
    assert!(market.detail.is_some(), "install keeps the expanded detail");

    market.apply_uninstalled("pdf-tools@local-market");
    let row = &market.plugins[0];
    assert!(!row.installed && !row.enabled, "the uninstall flips back");
    assert_eq!(market.detail, None, "uninstall collapses the detail");
}

/// A wire-shaped `plugin/read` payload carrying two bundled skills.
fn read_response(plugin_id: &str) -> PluginReadResponse {
    let marketplace = plugin_id.rsplit('@').next().unwrap_or_default();
    from_value(json!({
        "plugin": {
            "marketplaceName": marketplace,
            "marketplacePath": null,
            "summary": plugin_json(plugin_id, plugin_id, json!({"type": "remote"}), false, false),
            "shareUrl": null,
            "description": null,
            "skills": [
                {"name": "pdf-split", "description": "split pdfs", "shortDescription": null, "interface": null, "path": null, "enabled": true},
                {"name": "pdf-merge", "description": "merge pdfs", "shortDescription": null, "interface": null, "path": null, "enabled": true}
            ],
            "onboardingSkill": null,
            "hooks": [],
            "apps": [],
            "appTemplates": [],
            "mcpServers": [],
            "scheduledTasks": null
        }
    }))
    .expect("the read payload decodes")
}

/// A wire-shaped `plugin/list` payload whose marketplace ships a curated
/// plugin.
fn curated_list_response() -> PluginListResponse {
    from_value(json!({
        "marketplaces": [{
            "name": "openai-api-curated",
            "path": "/tmp/market/api_marketplace.json",
            "interface": {"displayName": "Codex official"},
            "plugins": [plugin_json(
                "game-studio@openai-api-curated",
                "game-studio",
                json!({"type": "local", "path": "/tmp/market/game-studio"}),
                false,
                false,
            )]
        }],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": []
    }))
    .expect("the wire payload decodes")
}

#[test]
fn apply_list_localizes_curated_plugins() {
    let mut market = SkillMarket::default();

    market.apply_list(&curated_list_response());

    assert_eq!(market.plugins[0].name, "游戏工作室");
    assert_eq!(
        market.plugins[0].description.as_deref(),
        Some("设计、原型化并发布浏览器游戏")
    );
}
