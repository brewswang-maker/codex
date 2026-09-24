//! Throttled buffering for streaming text deltas.
//!
//! Agent messages and command output arrive as many tiny deltas; reparsing
//! and re-rendering on every single one would waste work. Deltas accumulate
//! here per item, and the UI drains the buffer on a fixed timer tick, so
//! parsing and rendering happen at most once per tick.

use std::collections::HashMap;

/// Collects streaming text deltas per item id until they are drained.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarkdownStream {
    pending: HashMap<String, String>,
}

impl MarkdownStream {
    /// Buffers one delta for the given item.
    pub fn push(&mut self, item_id: impl Into<String>, delta: &str) {
        self.pending
            .entry(item_id.into())
            .or_default()
            .push_str(delta);
    }

    /// Takes everything buffered so far; item order is unspecified.
    pub fn drain(&mut self) -> Vec<(String, String)> {
        self.pending.drain().collect()
    }

    /// Drops buffered text for the given item; used when a completed
    /// snapshot supersedes whatever was still in flight.
    pub fn cancel(&mut self, item_id: &str) {
        let _ = self.pending.remove(item_id);
    }

    /// Whether any buffered text is waiting to be rendered.
    pub fn is_active(&self) -> bool {
        !self.pending.is_empty()
    }
}
