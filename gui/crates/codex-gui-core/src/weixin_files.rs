//! WeChat bridge runtime files: the panel projection (bridge liveness,
//! login state, pending authorizations) and the grant write.
//!
//! The GUI and the bridge daemon coordinate purely through the files under
//! `~/.codex/weixin-bridge/`; nothing here talks to the process itself.

use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

/// The bridge's directory under the codex home.
pub const WEIXIN_BRIDGE_DIR: &str = "weixin-bridge";

/// What the panel shows, projected from the bridge's runtime files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeixinPanelData {
    /// `bridge-state.json` present and its pid alive.
    pub running: bool,
    /// `credentials/account.json` present.
    pub logged_in: bool,
    /// The logged-in bot id, when credentials exist.
    pub bot_id: Option<String>,
    /// Contacts that talked while unauthorized, oldest grant candidate last.
    pub pending: Vec<PendingRow>,
    /// Current allowlist entries.
    pub allowed: Vec<String>,
}

/// One unauthorized contact seen by the bridge; wire shape matches the
/// bridge's `PendingContact` (camelCase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRow {
    pub contact: String,
    pub last_seen_ms: i64,
}

/// Reads the panel projection. `pid_alive` decides whether the liveness
/// mark's pid is still running (prod checks `/proc`; tests inject).
pub fn read_panel(codex_home: &Path, pid_alive: impl Fn(u32) -> bool) -> WeixinPanelData {
    let dir = codex_home.join(WEIXIN_BRIDGE_DIR);

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BridgeState {
        pid: u32,
    }
    let running = std::fs::read_to_string(dir.join("runtime/bridge-state.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<BridgeState>(&text).ok())
        .map(|state| pid_alive(state.pid))
        .unwrap_or(false);

    #[derive(Deserialize)]
    struct Account {
        #[serde(default, rename = "account_id")]
        account_id: Option<String>,
    }
    let account = std::fs::read_to_string(dir.join("credentials/account.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Account>(&text).ok());
    let logged_in = account.is_some();
    let bot_id = account.and_then(|account| account.account_id);

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PendingFile {
        contact: String,
        last_seen_ms: i64,
    }
    let pending: Vec<PendingRow> =
        std::fs::read_to_string(dir.join("runtime/pending-contacts.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<PendingFile>>(&text).ok())
            .map(|rows| {
                rows.into_iter()
                    .map(|row| PendingRow {
                        contact: row.contact,
                        last_seen_ms: row.last_seen_ms,
                    })
                    .collect()
            })
            .unwrap_or_default();

    #[derive(Deserialize)]
    struct AllowFile {
        #[serde(default)]
        allowed: Vec<String>,
    }
    let allowed: Vec<String> = std::fs::read_to_string(dir.join("runtime/allowlist.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<AllowFile>(&text).ok())
        .map(|file| file.allowed)
        .unwrap_or_default();

    WeixinPanelData {
        running,
        logged_in,
        bot_id,
        pending,
        allowed,
    }
}

/// Replaces a shared runtime JSON file atomically so the bridge's periodic
/// reload never observes a partial write.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("json"),
        std::process::id()
    ));
    std::fs::write(&temp, text).map_err(|err| err.to_string())?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            Err(err.to_string())
        }
    }
}

/// Grants a contact: appends to `allowlist.json` (dedup) and drops them
/// from `pending-contacts.json`. Returns a user-facing error string.
pub fn grant_contact(codex_home: &Path, contact: &str) -> Result<(), String> {
    if contact.trim().is_empty() {
        return Err("联系人标识为空".to_string());
    }
    let runtime = codex_home.join(WEIXIN_BRIDGE_DIR).join("runtime");
    let write_json = |value: &serde_json::Value| {
        std::fs::create_dir_all(&runtime).map_err(|err| err.to_string())?;
        serde_json::to_string_pretty(value).map_err(|err| err.to_string())
    };

    let allow_path = runtime.join("allowlist.json");
    let mut allowed: Vec<String> = std::fs::read_to_string(&allow_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| serde_json::from_value::<Vec<String>>(value["allowed"].clone()).ok())
        .unwrap_or_default();
    if !allowed.iter().any(|entry| entry == contact) {
        allowed.push(contact.to_string());
    }
    let text = write_json(&serde_json::json!({ "allowed": allowed }))?;
    write_atomic(&allow_path, &text)?;

    let pending_path = runtime.join("pending-contacts.json");
    let pending: Vec<PendingRow> = std::fs::read_to_string(&pending_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .map(|rows: Vec<PendingRow>| {
            rows.into_iter()
                .filter(|row| row.contact != contact)
                .collect()
        })
        .unwrap_or_default();
    if let Ok(text) = serde_json::to_string_pretty(&pending) {
        let _ = write_atomic(&pending_path, &text);
    }
    Ok(())
}

#[cfg(test)]
#[path = "weixin_files_tests.rs"]
mod tests;
