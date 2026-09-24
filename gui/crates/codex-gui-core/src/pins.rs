//! Pinned threads, persisted as local JSON.
//!
//! The store lives at `$HOME/.local/state/codex-gui/pinned-threads.json`
//! (XDG state home when set), next to the recent-projects store. Pins are
//! keyed by thread id and outlive both restarts and thread-list pagination.

use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The persisted set of pinned thread ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinnedThreads {
    path: PathBuf,
    ids: BTreeSet<String>,
}

impl PinnedThreads {
    /// Loads the store from its JSON file; any failure yields an empty
    /// store that still remembers where to save.
    pub fn load(path: PathBuf) -> Self {
        let ids = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("pinned")?
                    .as_array()?
                    .iter()
                    .map(|entry| entry.as_str().map(String::from))
                    .collect::<Option<Vec<String>>>()
            })
            .unwrap_or_default();

        Self {
            path,
            ids: ids.into_iter().collect(),
        }
    }

    /// The default on-disk location (`$XDG_STATE_HOME` or `$HOME` based).
    pub fn default_path() -> PathBuf {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("codex-gui")
            .join("pinned-threads.json")
    }

    /// Flips the pin state of `thread_id`; returns whether it is pinned
    /// afterwards.
    pub fn toggle(&mut self, thread_id: &str) -> bool {
        if !self.ids.remove(thread_id) {
            self.ids.insert(String::from(thread_id));
        }
        self.ids.contains(thread_id)
    }

    /// Whether `thread_id` is currently pinned.
    pub fn is_pinned(&self, thread_id: &str) -> bool {
        self.ids.contains(thread_id)
    }

    /// Pinned thread ids, sorted for stable sidebar ordering.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.ids.iter().map(String::as_str)
    }

    /// The pinned ids as a set, for the sidebar grouping helper.
    pub fn id_set(&self) -> &BTreeSet<String> {
        &self.ids
    }

    /// Rewrites the JSON file; failures are swallowed (pins are a
    /// convenience, not state the app depends on).
    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ignored = std::fs::create_dir_all(parent);
        }
        let pinned: Vec<&str> = self.ids.iter().map(String::as_str).collect();
        let payload = json!({ "pinned": pinned });
        let _ignored = std::fs::write(&self.path, payload.to_string());
    }
}

#[cfg(test)]
#[path = "pins_tests.rs"]
mod tests;
