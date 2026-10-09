//! Tests for the runtime state files the GUI panel reads.
#![allow(clippy::expect_used)]

use super::{
    BridgeState, PendingContact, clear_running, drop_pending, read_pending, record_pending,
    write_running,
};
use crate::error::Result;
use std::path::PathBuf;

/// A unique scratch codex home per test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wx-bridge-state-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn running_mark_round_trips_and_clears() -> Result<()> {
    let home = scratch("mark");
    write_running(&home, "bot-1")?;

    let path = super::runtime_dir(&home).join("bridge-state.json");
    let state: BridgeState = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(state.bot_id, "bot-1");
    assert_eq!(state.pid, std::process::id());

    clear_running(&home);
    assert!(!path.exists(), "clean shutdown removes the mark");
    Ok(())
}

#[test]
fn pending_contacts_dedupe_and_refresh() {
    let home = scratch("pending");
    record_pending(&home, "alice");
    record_pending(&home, "bob");
    record_pending(&home, "alice");

    let pending = read_pending(&home);
    assert_eq!(pending.len(), 2, "repeats refresh instead of duplicating");
    let alice = pending
        .iter()
        .find(|entry| entry.contact == "alice")
        .expect("alice is tracked");
    let bob = pending
        .iter()
        .find(|entry| entry.contact == "bob")
        .expect("bob is tracked");
    assert!(alice.last_seen_ms >= alice.first_seen_ms);
    assert_eq!(bob.first_seen_ms, bob.last_seen_ms);
}

#[test]
fn drop_pending_removes_only_the_target() {
    let home = scratch("drop");
    record_pending(&home, "alice");
    record_pending(&home, "bob");
    drop_pending(&home, "alice");

    let pending = read_pending(&home);
    assert_eq!(
        pending
            .iter()
            .map(|entry| entry.contact.as_str())
            .collect::<Vec<_>>(),
        vec!["bob"],
    );
}

#[test]
fn missing_files_read_as_empty() {
    let home = scratch("empty");
    assert!(read_pending(&home).is_empty());
}

#[test]
fn pending_json_shape_is_stable() {
    let entry = PendingContact {
        contact: "alice".to_string(),
        first_seen_ms: 1,
        last_seen_ms: 2,
    };
    let text = serde_json::to_string(&entry).expect("serializes");
    assert_eq!(
        text,
        r#"{"contact":"alice","firstSeenMs":1,"lastSeenMs":2}"#
    );
}
