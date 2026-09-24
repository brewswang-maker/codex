//! Newline-delimited JSON-RPC frame codec.
//!
//! The wire format intentionally has no `jsonrpc` field; see
//! `codex-rs/app-server-protocol/src/rpc.rs#L1-L2` for the authoritative note.

use codex_app_server_protocol::JSONRPCMessage;

/// Serializes a message into a single-line JSON frame (without the newline).
pub fn encode_line(message: &JSONRPCMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

/// Parses one wire line into a [`JSONRPCMessage`].
pub fn decode_line(line: &str) -> Result<JSONRPCMessage, serde_json::Error> {
    serde_json::from_str(line)
}
