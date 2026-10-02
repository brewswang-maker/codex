//! Session bridge: maps WeChat contacts onto codex threads and pumps
//! progress back into the chat.
//!
//! - [`session`] — persisted contact-to-thread mapping + app-server RPCs
//! - [`progress`] — notification-to-text shaping (summaries, final reply)
//! - [`runtime`] — the daemon loop tying the WeChat long poll to codex

pub mod progress;
pub mod runtime;
pub mod session;
