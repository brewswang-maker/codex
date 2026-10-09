//! The runtime state files the GUI panel reads: `runtime/bridge-state.json`
//! (a liveness mark removed on clean shutdown) and
//! `runtime/pending-contacts.json` (contacts that talked while not
//! allowlisted). Both live beside the allowlist the panel also edits, so the
//! GUI and the bridge coordinate through the filesystem only — no sockets.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// The runtime directory shared with the allowlist and contacts mapping.
pub fn runtime_dir(codex_home: &Path) -> PathBuf {
    codex_home.join("weixin-bridge").join("runtime")
}

/// Liveness mark: written at startup, removed on a clean shutdown. The GUI
/// treats a missing file (or a dead pid) as "bridge not running".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeState {
    pub bot_id: String,
    pub pid: u32,
    pub started_at_ms: i64,
}

/// A contact that tried to talk while not allowlisted; the panel offers a
/// one-click grant for these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingContact {
    pub contact: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

/// Publishes the running mark; fails only when the file cannot be written.
pub fn write_running(codex_home: &Path, bot_id: &str) -> Result<()> {
    let state = BridgeState {
        bot_id: bot_id.to_string(),
        pid: std::process::id(),
        started_at_ms: now_ms(),
    };
    let path = runtime_dir(codex_home).join("bridge-state.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&state)?;
    write_atomic(&path, &text)?;
    Ok(())
}

/// Removes the running mark; best-effort, a missing file is fine.
pub fn clear_running(codex_home: &Path) {
    let _ = std::fs::remove_file(runtime_dir(codex_home).join("bridge-state.json"));
}

/// Records (or refreshes) an unauthorized contact; best-effort — the chat
/// flow must not break because the panel state failed to persist.
pub fn record_pending(codex_home: &Path, contact: &str) {
    let mut pending = read_pending(codex_home);
    let now = now_ms();
    match pending.iter_mut().find(|entry| entry.contact == contact) {
        Some(entry) => entry.last_seen_ms = now,
        None => pending.push(PendingContact {
            contact: contact.to_string(),
            first_seen_ms: now,
            last_seen_ms: now,
        }),
    }
    let _ = std::fs::create_dir_all(runtime_dir(codex_home));
    let path = runtime_dir(codex_home).join("pending-contacts.json");
    if let Ok(text) = serde_json::to_string_pretty(&pending) {
        let _ = write_atomic(&path, &text);
    }
}

/// Drops an authorized contact from the pending list; best-effort.
pub fn drop_pending(codex_home: &Path, contact: &str) {
    let pending: Vec<PendingContact> = read_pending(codex_home)
        .into_iter()
        .filter(|entry| entry.contact != contact)
        .collect();
    let path = runtime_dir(codex_home).join("pending-contacts.json");
    if let Ok(text) = serde_json::to_string_pretty(&pending) {
        let _ = write_atomic(&path, &text);
    }
}

/// Reads the pending list; a missing or corrupt file reads as empty.
pub fn read_pending(codex_home: &Path) -> Vec<PendingContact> {
    std::fs::read_to_string(runtime_dir(codex_home).join("pending-contacts.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_millis() as i64)
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&temp, text)?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "bridge_state_tests.rs"]
mod tests;
