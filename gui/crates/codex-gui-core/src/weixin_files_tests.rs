//! Tests for the WeChat bridge panel projection and grant write.
#![allow(clippy::expect_used)]

use super::PendingRow;
use super::WeixinPanelData;
use super::grant_contact;
use super::read_panel;
use std::path::PathBuf;

/// A scratch codex home seeded like the bridge leaves it.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wx-panel-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("weixin-bridge/runtime")).expect("runtime dir");
    std::fs::create_dir_all(dir.join("weixin-bridge/credentials")).expect("credentials dir");
    dir
}

fn write(path: &std::path::Path, text: &str) {
    std::fs::write(path, text).expect("write");
}

#[test]
fn missing_files_read_as_offline_and_logged_out() {
    let home = scratch("empty");
    let panel = read_panel(&home, |_| true);
    assert_eq!(panel, WeixinPanelData::default());
}

#[test]
fn live_pid_and_credentials_surface_in_the_projection() {
    let home = scratch("live");
    write(
        &home.join("weixin-bridge/runtime/bridge-state.json"),
        r#"{"botId":"wx@im.bot","pid":42,"startedAtMs":1}"#,
    );
    write(
        &home.join("weixin-bridge/credentials/account.json"),
        r#"{"account_id":"wx@im.bot","token":"t","saved_at_ms":1}"#,
    );
    write(
        &home.join("weixin-bridge/runtime/pending-contacts.json"),
        r#"[{"contact":"carol","firstSeenMs":5,"lastSeenMs":9}]"#,
    );
    write(
        &home.join("weixin-bridge/runtime/allowlist.json"),
        r#"{"allowed":["alice"]}"#,
    );

    let alive = read_panel(&home, |pid| pid == 42);
    assert!(alive.running);
    assert!(alive.logged_in);
    assert_eq!(alive.bot_id.as_deref(), Some("wx@im.bot"));
    assert_eq!(
        alive.pending,
        vec![PendingRow {
            contact: String::from("carol"),
            last_seen_ms: 9,
        }],
    );
    assert_eq!(alive.allowed, vec![String::from("alice")]);

    // A dead pid reports not-running despite the mark file.
    assert!(!read_panel(&home, |_| false).running);
}

#[test]
fn grant_appends_to_allowlist_and_clears_pending() {
    let home = scratch("grant");
    let runtime = home.join("weixin-bridge/runtime");
    write(&runtime.join("allowlist.json"), r#"{"allowed":["alice"]}"#);
    write(
        &runtime.join("pending-contacts.json"),
        r#"[{"contact":"carol","firstSeenMs":5,"lastSeenMs":9},{"contact":"dan","firstSeenMs":6,"lastSeenMs":9}]"#,
    );

    grant_contact(&home, "carol").expect("grant succeeds");

    let panel = read_panel(&home, |_| false);
    assert_eq!(
        panel.allowed,
        vec![String::from("alice"), String::from("carol")]
    );
    assert_eq!(
        panel.pending,
        vec![PendingRow {
            contact: String::from("dan"),
            last_seen_ms: 9,
        }],
    );

    // Granting twice is a no-op, not a duplicate entry.
    grant_contact(&home, "carol").expect("regrant succeeds");
    assert_eq!(read_panel(&home, |_| false).allowed.len(), 2);
}

#[test]
fn grant_creates_files_when_missing() {
    let home = scratch("grant-fresh");
    grant_contact(&home, "alice").expect("grant succeeds");
    let panel = read_panel(&home, |_| false);
    assert_eq!(panel.allowed, vec![String::from("alice")]);
    assert!(panel.pending.is_empty());
}
