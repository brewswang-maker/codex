//! The Skill market tab: installable plugins from the configured
//! marketplaces, flattened into one list, plus the install lifecycle.
//!
//! The market speaks the official plugin/marketplace RPC family; a market
//! row is a plugin (the install unit), whose bundled skills show after a
//! `plugin/read`.

use crate::plugin_i18n;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginReadResponse;
use codex_app_server_protocol::PluginSource;
use std::path::PathBuf;

/// One market row: a plugin inside one marketplace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketPlugin {
    /// `PluginSummary.id` (`name@marketplace`).
    pub id: String,
    /// Display name of the plugin (Chinese copy for curated catalogs).
    pub name: String,
    /// Raw plugin name exactly as the marketplace manifest spells it; this
    /// is the name `plugin/read` and `plugin/install` address.
    pub wire_name: String,
    /// Name of the owning marketplace.
    pub marketplace: String,
    /// Marketplace file path for local marketplaces; `None` for the
    /// remote catalog, which is addressed by name instead.
    pub marketplace_path: Option<PathBuf>,
    /// One-line description when the plugin declares one.
    pub description: Option<String>,
    /// How the plugin is delivered (local / git / npm / remote).
    pub source_label: String,
    /// Whether a local copy is installed.
    pub installed: bool,
    /// Whether the installed copy is enabled.
    pub enabled: bool,
}

impl MarketPlugin {
    /// The marketplace selector sent with `plugin/read`/`plugin/install`:
    /// a local marketplace addresses by path, the remote catalog by name.
    pub fn marketplace_selector(&self) -> MarketSelector {
        match &self.marketplace_path {
            Some(path) => MarketSelector::Path(path.clone()),
            None => MarketSelector::RemoteName(self.marketplace.clone()),
        }
    }
}

/// How one market row addresses its marketplace on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketSelector {
    /// A local marketplace file path.
    Path(PathBuf),
    /// The remote catalog marketplace name.
    RemoteName(String),
}

/// The skills one plugin ships, after a `plugin/read`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketDetail {
    /// The plugin this detail belongs to.
    pub plugin_id: String,
    /// The plugin's bundled skills.
    pub skills: Vec<MarketSkill>,
}

/// One bundled skill of a market plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketSkill {
    pub name: String,
    pub description: String,
}

/// The market tab's state: the flattened catalog, the expanded plugin's
/// detail, the marketplace-source draft, and the install lifecycle.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SkillMarket {
    /// Whether a `plugin/list` has populated the catalog.
    pub loaded: bool,
    /// Every plugin across every marketplace.
    pub plugins: Vec<MarketPlugin>,
    /// The expanded plugin's skills, when loaded.
    pub detail: Option<MarketDetail>,
    /// The plugin whose detail is being fetched.
    pub detail_loading: Option<String>,
    /// The marketplace-source input behind "添加市场".
    pub source_draft: String,
    /// The plugin whose install is in flight.
    pub busy_plugin: Option<String>,
}

impl SkillMarket {
    /// Replaces the catalog with a fresh `plugin/list` response.
    pub fn apply_list(&mut self, response: &PluginListResponse) {
        self.plugins = response
            .marketplaces
            .iter()
            .flat_map(|marketplace| {
                let marketplace_name = marketplace.name.clone();
                let path = marketplace.path.clone().map(PathBuf::from);
                marketplace.plugins.iter().map(move |plugin| {
                    let (name, description) = plugin_i18n::localize(
                        &plugin.name,
                        plugin
                            .interface
                            .as_ref()
                            .and_then(|interface| interface.short_description.as_deref()),
                    );
                    MarketPlugin {
                        id: plugin.id.clone(),
                        name,
                        wire_name: plugin.name.clone(),
                        marketplace: marketplace_name.clone(),
                        marketplace_path: path.clone(),
                        description,
                        source_label: source_label(&plugin.source),
                        installed: plugin.installed,
                        enabled: plugin.enabled,
                    }
                })
            })
            .collect();
        self.loaded = true;
    }

    /// Toggles the expanded plugin: collapsing clears the detail, expanding
    /// asks the caller to fetch one.
    pub fn toggle_detail(&mut self, plugin_id: &str) {
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.plugin_id == plugin_id)
        {
            self.detail = None;
            self.detail_loading = None;
        } else {
            self.detail = None;
            self.detail_loading = Some(plugin_id.to_string());
        }
    }

    /// Stores the fetched detail when it matches the expanded row.
    pub fn apply_detail(&mut self, response: &PluginReadResponse) {
        let plugin_id = response.plugin.summary.id.clone();
        if self.detail_loading.as_deref() != Some(plugin_id.as_str()) {
            return;
        }
        self.detail_loading = None;
        self.detail = Some(MarketDetail {
            plugin_id,
            skills: response
                .plugin
                .skills
                .iter()
                .map(|skill| MarketSkill {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                })
                .collect(),
        });
    }

    /// Mirrors a settled install onto the matching row.
    pub fn apply_installed(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.iter_mut().find(|plugin| plugin.id == id) {
            plugin.installed = true;
            plugin.enabled = true;
        }
    }

    /// Mirrors a settled uninstall onto the matching row.
    pub fn apply_uninstalled(&mut self, id: &str) {
        if let Some(plugin) = self.plugins.iter_mut().find(|plugin| plugin.id == id) {
            plugin.installed = false;
            plugin.enabled = false;
        }
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.plugin_id == id)
        {
            self.detail = None;
        }
    }
}

/// The delivery channel of one plugin as a short badge.
fn source_label(source: &PluginSource) -> String {
    match source {
        PluginSource::Local { .. } => "本地",
        PluginSource::Git { .. } => "Git",
        PluginSource::Npm { .. } => "npm",
        PluginSource::Remote => "远程",
    }
    .to_string()
}
