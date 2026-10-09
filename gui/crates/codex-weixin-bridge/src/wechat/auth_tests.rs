//! Credential-store and login-helper tests (no network).
#![allow(clippy::expect_used, clippy::panic)]

use pretty_assertions::assert_eq;

use super::*;

fn temp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path());
    (dir, store)
}

fn sample_account() -> AccountData {
    AccountData {
        account_id: "b0f5860fdecb@im.bot".to_string(),
        token: "secret-token".to_string(),
        base_url: Some("https://redirect.example".to_string()),
        user_id: Some("me@im.wechat".to_string()),
        saved_at_ms: 1_736_888_888_000,
    }
}

#[test]
fn account_round_trips_through_the_store() {
    let (_guard, store) = temp_store();
    assert_eq!(store.load_account(), None);

    store
        .save_account(&sample_account())
        .expect("save should succeed");
    assert_eq!(store.load_account(), Some(sample_account()));
}

#[test]
fn account_file_permissions_are_restricted_on_unix() {
    let (_guard, store) = temp_store();
    store
        .save_account(&sample_account())
        .expect("save should succeed");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let file_mode = std::fs::metadata(store.root().join("credentials/account.json"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(file_mode & 0o777, 0o600);
        let dir_mode = std::fs::metadata(store.root().join("credentials"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(dir_mode & 0o777, 0o700);
    }
}

#[test]
fn clear_account_removes_credentials_and_cursor() {
    let (_guard, store) = temp_store();
    store
        .save_account(&sample_account())
        .expect("save should succeed");
    store.save_sync_buf("cursor").expect("save cursor");

    store.clear_account();
    assert_eq!(store.load_account(), None);
    assert_eq!(store.load_sync_buf(), "");
}

#[test]
fn sync_buf_round_trips_and_defaults_to_empty() {
    let (_guard, store) = temp_store();
    assert_eq!(store.load_sync_buf(), "");

    store
        .save_sync_buf("opaque-cursor-blob")
        .expect("save should succeed");
    assert_eq!(store.load_sync_buf(), "opaque-cursor-blob");
}

#[test]
fn load_client_requires_saved_credentials() {
    let (_guard, store) = temp_store();
    match store.load_client() {
        Err(BridgeError::NotLoggedIn) => {}
        other => panic!("expected NotLoggedIn, got {other:?}"),
    }

    store
        .save_account(&sample_account())
        .expect("save should succeed");
    let client = store.load_client().expect("client");
    assert_eq!(client.token(), Some("secret-token"));
    assert_eq!(client.base_url(), "https://redirect.example");
}

#[test]
fn account_ids_normalize_to_filesystem_safe_keys() {
    assert_eq!(
        normalize_account_id("b0f5860fdecb@im.bot"),
        "b0f5860fdecb-im-bot"
    );
    assert_eq!(normalize_account_id("ABCdef123"), "abcdef123");
    assert_eq!(normalize_account_id(""), "unknown-bot");
    assert_eq!(normalize_account_id("///"), "unknown-bot");
}
