//! stdio JSON-RPC bridge between the GUI and a spawned `codex app-server`.
//!
//! The wire format mirrors `codex-rs/app-server-protocol/src/rpc.rs`: newline
//! delimited JSON objects *without* the `jsonrpc` field. [`ClientRequest`],
//! [`ServerNotification`], and [`ServerRequest`] are reused straight from the
//! protocol crate so the GUI stays in lockstep with the server types.
//!
//! The crate intentionally does not depend on `iced` so the codec, client, and
//! connection loop can be tested headlessly; the UI layer adapts
//! [`GuiEvent`]s into `iced::Subscription` messages.
//!
//! [`ClientRequest`]: codex_app_server_protocol::ClientRequest
//! [`ServerNotification`]: codex_app_server_protocol::ServerNotification
//! [`ServerRequest`]: codex_app_server_protocol::ServerRequest

mod client;
mod codec;
mod error;
mod events;
mod process;
mod run;

#[cfg(test)]
#[path = "codec_tests.rs"]
mod codec_tests;

pub use client::Client;
pub use codec::{decode_line, encode_line};
pub use error::Error;
pub use events::{Flags, GuiEvent};
pub use run::start;
