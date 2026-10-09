//! Per-contact thread mapping plus thin wrappers around the app-server RPCs
//! the bridge needs (`thread/start`, `thread/resume`, `turn/start`,
//! `turn/interrupt`).
//!
//! The mapping persists in `~/.codex/weixin-bridge/runtime/contacts.json` so
//! a bridge restart resumes the same codex threads instead of forking new
//! ones; the threads themselves live in the shared `~/.codex` store, which is
//! also what makes them visible in the GUI thread list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codex_app_server_protocol::{
    AskForApproval, ClientInfo, ClientRequest, InitializeCapabilities, InitializeParams,
    SandboxMode, SortDirection, ThreadResumeInitialTurnsPageParams, ThreadResumeParams,
    ThreadSource, ThreadStartParams, TurnInterruptParams, TurnItemsView, TurnStartParams,
    UserInput,
};
use codex_gui_bridge::{Client, Flags, GuiEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::{BridgeError, Result};

/// One contact's bound codex thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactRecord {
    pub thread_id: String,
    /// Unix milliseconds of the last bind/reset.
    pub updated_at_ms: i64,
}

/// On-disk shape of `runtime/contacts.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContactsFile {
    #[serde(default)]
    pub contacts: HashMap<String, ContactRecord>,
}

/// Loads and saves the contact-to-thread mapping.
pub struct SessionStore {
    path: PathBuf,
    file: ContactsFile,
}

impl SessionStore {
    /// Loads `runtime/contacts.json` under the bridge state root; a missing
    /// or unreadable file means "start empty" (the mapping is rebuildable).
    pub fn load(codex_home: &Path) -> Self {
        let path = codex_home
            .join("weixin-bridge")
            .join("runtime")
            .join("contacts.json");
        let file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, file }
    }

    pub fn thread_for(&self, contact: &str) -> Option<&str> {
        self.file
            .contacts
            .get(contact)
            .map(|r| r.thread_id.as_str())
    }

    /// Binds (or rebinds) a contact and persists immediately.
    pub fn bind(&mut self, contact: &str, thread_id: &str) -> Result<()> {
        self.file.contacts.insert(
            contact.to_string(),
            ContactRecord {
                thread_id: thread_id.to_string(),
                updated_at_ms: unix_now_ms(),
            },
        );
        self.save()
    }

    /// Drops a contact's binding (`/new`); the thread itself stays in
    /// `~/.codex` and remains visible in the GUI. Returns whether a binding
    /// existed.
    pub fn unbind(&mut self, contact: &str) -> Result<bool> {
        let removed = self.file.contacts.remove(contact).is_some();
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| BridgeError::StateDir {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let payload = serde_json::to_string_pretty(&self.file)?;
        std::fs::write(&self.path, payload)?;
        Ok(())
    }
}

/// Connection to the spawned `codex app-server` child, specialized for the
/// bridge's per-contact thread lifecycle.
#[derive(Clone)]
pub struct CodexBridge {
    client: Client,
}

impl CodexBridge {
    /// Spawns `codex app-server` and completes the `initialize` handshake.
    /// `app_server` overrides the backend binary; `None` resolves `codex`
    /// from `PATH`.
    /// Returns the handle plus the event stream (notifications, approvals,
    /// disconnects).
    pub async fn connect(app_server: Option<PathBuf>) -> Result<(Self, mpsc::Receiver<GuiEvent>)> {
        let mut flags = Flags::default_app_server();
        if let Some(program) = app_server {
            flags.program = program;
        }
        let (client, events) = codex_gui_bridge::start(flags).await?;
        client
            .call(|request_id| ClientRequest::Initialize {
                request_id,
                params: InitializeParams {
                    client_info: ClientInfo {
                        name: "codex-weixin-bridge".to_string(),
                        title: Some("Codex Weixin Bridge".to_string()),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    },
                    capabilities: Some(InitializeCapabilities {
                        experimental_api: true,
                        ..InitializeCapabilities::default()
                    }),
                },
            })
            .await?;
        Ok((Self { client }, events))
    }

    /// Opens a fresh thread; returns its id from `thread.id`. `model` pins
    /// the thread to a `/model` pick (applies for the thread's lifetime).
    pub async fn start_thread(&self, cwd: Option<String>, model: Option<String>) -> Result<String> {
        let value = self
            .client
            .call(|request_id| ClientRequest::ThreadStart {
                request_id,
                params: ThreadStartParams {
                    cwd,
                    model,
                    // Mirrors the GUI posture: workspace edits flow without
                    // per-file prompts, anything outside still asks.
                    approval_policy: Some(AskForApproval::OnRequest),
                    sandbox: Some(SandboxMode::WorkspaceWrite),
                    // Marks the thread as WeChat-originated so the GUI can
                    // badge it and the Quest rail can group it.
                    thread_source: Some(ThreadSource::Feature("weixin".to_string())),
                    ..ThreadStartParams::default()
                },
            })
            .await?;
        thread_id_from(&value)
            .ok_or_else(|| BridgeError::Other("thread/start 响应缺少 thread.id".to_string()))
    }

    /// Reattaches to a persisted thread with the same approval posture as a
    /// fresh start (without it an older thread keeps the server-default
    /// policy that prompts on every patch). One history page is requested to
    /// match the GUI resume shape; the bridge never renders history.
    pub async fn resume_thread(&self, thread_id: &str) -> Result<()> {
        self.client
            .call(|request_id| ClientRequest::ThreadResume {
                request_id,
                params: ThreadResumeParams {
                    thread_id: thread_id.to_string(),
                    approval_policy: Some(AskForApproval::OnRequest),
                    sandbox: Some(SandboxMode::WorkspaceWrite),
                    exclude_turns: true,
                    initial_turns_page: Some(ThreadResumeInitialTurnsPageParams {
                        limit: Some(1),
                        sort_direction: Some(SortDirection::Asc),
                        items_view: Some(TurnItemsView::Full),
                    }),
                    ..ThreadResumeParams::default()
                },
            })
            .await?;
        Ok(())
    }

    /// Submits one text turn; returns the turn id from `turn.id`.
    pub async fn submit_turn(&self, thread_id: &str, text: &str) -> Result<String> {
        let value = self
            .client
            .call(|request_id| ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id: thread_id.to_string(),
                    input: vec![UserInput::Text {
                        text: text.to_string(),
                        text_elements: Vec::new(),
                    }],
                    ..TurnStartParams::default()
                },
            })
            .await?;
        Ok(value
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// Interrupts a running turn (`/stop`).
    pub async fn interrupt(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        self.client
            .call(|request_id| ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                },
            })
            .await?;
        Ok(())
    }

    /// Raw handle for the runtime's approval replies and event pump.
    pub fn client(&self) -> &Client {
        &self.client
    }
}

fn thread_id_from(value: &Value) -> Option<String> {
    value
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
