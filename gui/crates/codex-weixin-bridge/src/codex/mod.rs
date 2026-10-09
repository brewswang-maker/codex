//! Session bridge: maps WeChat contacts onto codex threads and pumps
//! progress back into the chat.
//!
//! - [`session`] — persisted contact-to-thread mapping + app-server RPCs
//! - [`progress`] — notification-to-text shaping (summaries, final reply)
//! - [`approval`] — WeChat-side approval forwarding (`y`/`n`, 60s timeout)
//! - [`allowlist`] — persisted contact authorization
//! - [`bridge_state`] — runtime files the GUI panel reads (liveness, pending)
//! - [`control`] — `/model` `/resume` helpers and shared command text
//! - [`runtime`] — the daemon loop tying the WeChat long poll to codex

pub mod allowlist;
pub mod approval;
pub mod bridge_state;
pub mod control;
pub mod progress;
pub mod runtime;
pub mod session;
