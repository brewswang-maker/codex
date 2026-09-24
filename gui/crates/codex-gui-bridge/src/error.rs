//! Error type shared by the bridge modules.

use codex_app_server_protocol::JSONRPCErrorError;

/// Failures surfaced by the app-server bridge.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to launch the `codex app-server` binary.
    #[error("failed to spawn {program:?}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    /// A newline-delimited JSON frame could not be read or written.
    #[error("io error on the app-server pipe: {0}")]
    Io(#[from] std::io::Error),

    /// A wire frame did not decode into [`JSONRPCMessage`].
    #[error("malformed JSON-RPC frame: {0}")]
    Json(#[from] serde_json::Error),

    /// The app-server process closed the pipe.
    #[error("app-server connection closed")]
    Closed,

    /// The pending request did not get a response in time.
    #[error("request timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// The server answered the request with a JSON-RPC error object.
    #[error("request failed ({code}): {message}")]
    Request { code: i64, message: String },

    /// An upstream response carried an id that never matched a pending call.
    #[error("received a response with no matching pending request (id {0})")]
    OrphanResponse(String),
}

impl From<&JSONRPCErrorError> for Error {
    fn from(value: &JSONRPCErrorError) -> Self {
        Self::Request {
            code: value.code,
            message: value.message.clone(),
        }
    }
}
