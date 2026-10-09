//! Tests for the persisted contact allowlist.
#![allow(clippy::expect_used)]

use super::AllowList;

#[test]
fn missing_file_seeds_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let list = AllowList::load(dir.path(), Some("scanner-user"));
    assert!(list.is_allowed("scanner-user"));

    let reloaded = AllowList::load(dir.path(), None);
    assert!(reloaded.is_allowed("scanner-user"));
}

#[test]
fn missing_file_without_seed_starts_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let list = AllowList::load(dir.path(), None);
    assert!(!list.is_allowed("anyone"));
    assert!(list.entries().is_empty());
}

#[test]
fn grant_persists_and_reports_new_entries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut list = AllowList::load(dir.path(), None);
    assert!(list.grant("alice").expect("grant"));
    assert!(!list.grant("alice").expect("grant again"));

    let reloaded = AllowList::load(dir.path(), None);
    assert!(reloaded.is_allowed("alice"));
    assert_eq!(reloaded.entries(), vec!["alice"]);
}

#[test]
fn corrupt_file_degrades_to_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let runtime_dir = dir.path().join("weixin-bridge").join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mkdir");
    std::fs::write(runtime_dir.join("allowlist.json"), "nonsense").expect("write");

    let list = AllowList::load(dir.path(), None);
    assert!(!list.is_allowed("alice"));
}
