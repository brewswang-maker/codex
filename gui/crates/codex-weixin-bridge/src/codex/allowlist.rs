//! Contact allowlist persisted at `runtime/allowlist.json`.
//!
//! First contact is seeded from the login scanner's user id; everyone else
//! starts in a pending-authorization state until granted via the CLI, the
//! GUI panel (D4), or a direct edit of the file.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{BridgeError, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
struct AllowFile {
    #[serde(default)]
    allowed: Vec<String>,
}

pub struct AllowList {
    path: PathBuf,
    entries: BTreeSet<String>,
}

impl AllowList {
    /// Loads `runtime/allowlist.json` under the codex home. A missing file is
    /// seeded with `seed` (the QR scanner's user id) and saved back, so the
    /// first login user can talk to the bridge immediately.
    pub fn load(codex_home: &Path, seed: Option<&str>) -> Self {
        let path = codex_home
            .join("weixin-bridge")
            .join("runtime")
            .join("allowlist.json");
        let entries: BTreeSet<String> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<AllowFile>(&text).ok())
            .map(|file| file.allowed.into_iter().collect())
            .unwrap_or_default();
        let mut list = Self { path, entries };
        if let Some(seed) = seed.map(str::trim).filter(|s| !s.is_empty())
            && list.entries.insert(seed.to_string())
        {
            // Best-effort: a failed seed write must not block startup.
            let _ = list.save();
        }
        list
    }

    pub fn is_allowed(&self, contact: &str) -> bool {
        self.entries.contains(contact)
    }

    /// Re-reads the file so grants made by the GUI panel (or CLI) take
    /// effect on a running bridge without a restart. Corrupt files are
    /// ignored: the last good list keeps applying.
    pub fn reload(&mut self) {
        if let Ok(text) = std::fs::read_to_string(&self.path)
            && let Ok(file) = serde_json::from_str::<AllowFile>(&text)
        {
            self.entries = file.allowed.into_iter().collect();
        }
    }

    /// Grants a contact; returns whether this was a new entry.
    pub fn grant(&mut self, contact: &str) -> Result<bool> {
        let fresh = self.entries.insert(contact.to_string());
        if fresh {
            self.save()?;
        }
        Ok(fresh)
    }

    pub fn entries(&self) -> Vec<&str> {
        self.entries.iter().map(String::as_str).collect()
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| BridgeError::StateDir {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let file = AllowFile {
            allowed: self.entries.iter().cloned().collect(),
        };
        let text = serde_json::to_string_pretty(&file)?;
        let temp = self
            .path
            .with_extension(format!("json.tmp-{}", std::process::id()));
        std::fs::write(&temp, text)?;
        if let Err(error) = std::fs::rename(&temp, &self.path) {
            let _ = std::fs::remove_file(&temp);
            return Err(error.into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "allowlist_tests.rs"]
mod tests;
