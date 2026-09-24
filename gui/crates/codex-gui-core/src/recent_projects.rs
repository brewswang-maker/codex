//! The welcome page's recent-projects list, persisted as local JSON.
//!
//! The store lives at `$HOME/.local/state/codex-gui/recent-projects.json`
//! (XDG state home when set); every write rewrites the whole file, which is
//! bounded by [`MAX_ENTRIES`].

use serde_json::json;
use std::path::PathBuf;

/// Entries kept on disk; the welcome page shows at most this many.
pub const MAX_ENTRIES: usize = 8;

/// The persisted, most-recent-first list of opened project directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProjects {
    path: PathBuf,
    entries: Vec<String>,
}

impl RecentProjects {
    /// Loads the store from its JSON file; any failure yields an empty
    /// store that still remembers where to save.
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| {
                value
                    .get("projects")?
                    .as_array()?
                    .iter()
                    .map(|entry| entry.as_str().map(String::from))
                    .collect::<Option<Vec<String>>>()
            })
            .unwrap_or_default();

        Self { path, entries }
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
            .join("recent-projects.json")
    }

    /// Moves `cwd` to the front (deduplicated), capped at [`MAX_ENTRIES`].
    pub fn remember(&mut self, cwd: &str) {
        self.entries.retain(|entry| entry != cwd);
        self.entries.insert(0, String::from(cwd));
        self.entries.truncate(MAX_ENTRIES);
    }

    /// Most-recent-first project paths.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Rewrites the JSON file; failures are swallowed (the list is a
    /// convenience, not state the app depends on).
    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ignored = std::fs::create_dir_all(parent);
        }
        let payload = json!({ "projects": self.entries });
        let _ignored = std::fs::write(&self.path, payload.to_string());
    }
}

#[cfg(test)]
#[path = "recent_projects_tests.rs"]
mod tests;
