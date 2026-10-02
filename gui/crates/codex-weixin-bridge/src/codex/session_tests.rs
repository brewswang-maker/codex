//! Tests for the persisted contact-to-thread mapping.

use super::SessionStore;

#[test]
fn missing_file_starts_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = SessionStore::load(dir.path());
    assert_eq!(store.thread_for("alice"), None);
}

#[test]
fn bind_persists_across_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = SessionStore::load(dir.path());
    store.bind("alice", "thread-1").expect("bind");

    let reloaded = SessionStore::load(dir.path());
    assert_eq!(reloaded.thread_for("alice"), Some("thread-1"));
}

#[test]
fn unbind_removes_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = SessionStore::load(dir.path());
    store.bind("alice", "thread-1").expect("bind");
    assert!(store.unbind("alice").expect("unbind"));

    let reloaded = SessionStore::load(dir.path());
    assert_eq!(reloaded.thread_for("alice"), None);
    // Unbinding again reports false.
    assert!(!store.unbind("alice").expect("unbind again"));
}

#[test]
fn rebind_replaces_thread() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = SessionStore::load(dir.path());
    store.bind("alice", "thread-1").expect("bind");
    store.bind("alice", "thread-2").expect("rebind");

    let reloaded = SessionStore::load(dir.path());
    assert_eq!(reloaded.thread_for("alice"), Some("thread-2"));
}

#[test]
fn corrupt_file_degrades_to_empty_and_recovers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let runtime_dir = dir.path().join("weixin-bridge").join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mkdir");
    std::fs::write(runtime_dir.join("contacts.json"), "{ not json").expect("write");

    let mut store = SessionStore::load(dir.path());
    assert_eq!(store.thread_for("anyone"), None);
    // A fresh bind still works after the corrupted load.
    store.bind("anyone", "thread-x").expect("bind after corrupt");
    let reloaded = SessionStore::load(dir.path());
    assert_eq!(reloaded.thread_for("anyone"), Some("thread-x"));
}
