//! The multi-vendor model menu: a Qoder-style catalog that groups the
//! models of every vendor entry the user can switch between.
//!
//! Three sources merge here: the built-in vendor presets
//! ([`crate::settings::PROVIDER_PRESETS`]) seed the groups, the providers
//! configured in `config.toml` mark a group usable (and add custom
//! gateways), and the live `model/list` catalog replaces the suggestion
//! list of the active provider with its real models.

use crate::settings::PROVIDER_PRESETS;

/// One selectable model row inside a vendor group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// The id sent as the config `model`.
    pub slug: String,
    /// Human-readable name; falls back to the slug.
    pub name: String,
    /// One-line description for the row.
    pub description: String,
}

/// One provider entry configured in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredProvider {
    /// The `[model_providers.<id>]` key, written as `model_provider`.
    pub id: String,
    /// Optional `name` field; falls back to the id.
    pub name: Option<String>,
    /// The provider's base URL, matched against the presets' endpoints.
    pub base_url: String,
}

/// One vendor and its selectable models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorGroup {
    /// Preset id, or the custom provider's config key.
    pub id: String,
    /// Display name of the vendor.
    pub name: String,
    /// The provider key picked from this group; `None` while unconfigured.
    pub provider_id: Option<String>,
    /// Whether a matching provider exists in the config.
    pub configured: bool,
    /// Models in display order.
    pub models: Vec<ModelChoice>,
}

/// The merged vendor catalog behind the model menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VendorCatalog {
    groups: Vec<VendorGroup>,
    current_provider: Option<String>,
    current_model: Option<String>,
}

impl VendorCatalog {
    /// The preset skeleton: one group per built-in vendor, carrying the
    /// suggested model ids from its plan entries.
    pub fn skeleton() -> Self {
        let mut groups = Vec::new();
        for preset in PROVIDER_PRESETS {
            if preset.id.is_empty() {
                // The custom slot renders only once the user configures it.
                continue;
            }
            let mut seen = Vec::new();
            let mut models = Vec::new();
            for entry in preset.entries {
                if seen.contains(&entry.model) {
                    continue;
                }
                seen.push(entry.model);
                models.push(ModelChoice {
                    slug: entry.model.to_string(),
                    name: entry.model.to_string(),
                    description: String::new(),
                });
            }
            groups.push(VendorGroup {
                id: preset.id.to_string(),
                name: preset.name.to_string(),
                provider_id: None,
                configured: false,
                models,
            });
        }
        Self {
            groups,
            current_provider: None,
            current_model: None,
        }
    }

    /// Applies the effective config: records the current selection and
    /// marks (or appends) the providers the user has configured. The
    /// preset groups rebuild from the skeleton first, so repeated calls
    /// stay idempotent.
    pub fn apply_config(
        &mut self,
        model: Option<&str>,
        model_provider: Option<&str>,
        providers: &[ConfiguredProvider],
    ) {
        self.current_model = model.map(str::to_string);
        self.current_provider = model_provider.map(str::to_string);
        let mut rebuilt = Self::skeleton();
        rebuilt.current_model = self.current_model.clone();
        rebuilt.current_provider = self.current_provider.clone();
        self.groups = rebuilt.groups;

        for provider in providers {
            if let Some(group) = self
                .groups
                .iter_mut()
                .find(|group| preset_matches(&group.id, &provider.base_url))
            {
                group.configured = true;
                group.provider_id = Some(provider.id.clone());
            } else {
                self.groups.push(VendorGroup {
                    id: provider.id.clone(),
                    name: provider.name.clone().unwrap_or_else(|| provider.id.clone()),
                    provider_id: Some(provider.id.clone()),
                    configured: true,
                    models: Vec::new(),
                });
            }
        }
    }

    /// Replaces the attached vendor's suggestion list with the live
    /// catalog rows; an empty catalog keeps the suggestions.
    ///
    /// The catalog describes the active provider, but a static
    /// `model_catalog_json` override can hand over another vendor's list;
    /// attaching it to the active group would show the wrong models under
    /// the wrong vendor. The rows go to the group they belong to (matching
    /// slugs) whenever the active group's own suggestions say otherwise.
    pub fn attach_catalog(&mut self, entries: &[(String, String, String)]) {
        if entries.is_empty() {
            return;
        }
        let current_index = self.current_provider.as_deref().and_then(|provider_id| {
            self.groups
                .iter()
                .position(|group| group.provider_id.as_deref() == Some(provider_id))
        });
        let current_accepts = current_index.is_some_and(|index| {
            let group = &self.groups[index];
            group.models.is_empty() || catalog_overlaps(group, entries)
        });
        let owner = if current_accepts {
            current_index
        } else {
            self.groups
                .iter()
                .position(|group| catalog_overlaps(group, entries))
                .or(current_index)
        };
        let Some(index) = owner else {
            return;
        };
        self.groups[index].models = entries
            .iter()
            .map(|(slug, name, description)| ModelChoice {
                slug: slug.clone(),
                name: name.clone(),
                description: description.clone(),
            })
            .collect();
    }

    /// The vendor groups in display order.
    pub fn groups(&self) -> &[VendorGroup] {
        &self.groups
    }

    /// The effective `(provider, model)` pair, once the config has landed.
    pub fn current(&self) -> (Option<&str>, Option<&str>) {
        (
            self.current_provider.as_deref(),
            self.current_model.as_deref(),
        )
    }
}

/// Whether the catalog rows mention at least one of the group's slugs;
/// an overlap identifies the group the rows belong to.
fn catalog_overlaps(group: &VendorGroup, entries: &[(String, String, String)]) -> bool {
    group
        .models
        .iter()
        .any(|choice| entries.iter().any(|(slug, _, _)| slug == &choice.slug))
}

/// Whether a configured provider belongs to the preset group `group_id`:
/// the config key matches the preset id, or the provider's base URL is
/// one of the preset's endpoints (host-level comparison).
fn preset_matches(group_id: &str, base_url: &str) -> bool {
    let Some(preset) = PROVIDER_PRESETS.iter().find(|preset| preset.id == group_id) else {
        return false;
    };
    let Some(host) = host_of(base_url) else {
        return false;
    };
    preset
        .entries
        .iter()
        .any(|entry| host_of(entry.base_url).as_deref() == Some(host.as_str()))
}

/// The lowercased host of a URL without pulling in a URL crate.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

#[cfg(test)]
#[path = "model_vendors_tests.rs"]
mod tests;
