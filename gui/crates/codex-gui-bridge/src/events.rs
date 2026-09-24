//! How to launch the backend, and what the GUI observes from it.

use crate::client::Client;
use codex_app_server_protocol::{ServerNotification, ServerRequest};
use std::path::PathBuf;

/// Launch configuration for the `codex app-server` child process.
///
/// The default mirrors the documented CLI entry point: `codex app-server`
/// speaking newline-delimited JSON-RPC over stdio.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Flags {
    /// Backend binary; resolved from `PATH` when relative.
    pub program: PathBuf,
    /// Extra arguments after the subcommand (kept empty by default).
    pub args: Vec<String>,
}

impl Flags {
    /// Default launch flags: run `codex app-server` from `PATH`.
    pub fn default_app_server() -> Self {
        Self {
            program: PathBuf::from("codex"),
            args: Vec::new(),
        }
    }
}

/// Events streamed from the app-server connection to the UI.
///
/// The size skew comes from the fat protocol payload types; boxing them would
/// contaminate every match site for a low-frequency event stream.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum GuiEvent {
    /// The child process was spawned; the handle issues requests on the pipe.
    Connected(Client),
    /// A server-initiated notification (item updates, deltas, status...).
    Notification(ServerNotification),
    /// A server-initiated request that *must* be answered, e.g. approvals.
    ServerRequest(ServerRequest),
    /// A frame arrived that could not be interpreted; kept for diagnostics.
    Undecodable(String),
    /// The child process exited; the payload is its status description.
    Disconnected(String),
}
