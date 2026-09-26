//! Unit coverage of the pinned-thread store.
#![allow(clippy::expect_used, clippy::panic)]

use super::PinnedThreads;

#[test]
fn toggle_flips_membership_and_reports_new_state() {
    let mut pins = PinnedThreads::default();
    assert!(!pins.is_pinned("t1"));

    assert!(pins.toggle("t1"));
    assert!(pins.is_pinned("t1"));

    assert!(!pins.toggle("t1"));
    assert!(!pins.is_pinned("t1"));
}

#[test]
fn ids_iterate_sorted_for_stable_ordering() {
    let mut pins = PinnedThreads::default();
    pins.toggle("t3");
    pins.toggle("t1");
    pins.toggle("t2");

    let ids: Vec<&str> = pins.ids().collect();
    assert_eq!(ids, vec!["t1", "t2", "t3"]);
}

#[test]
fn save_then_load_roundtrips_through_the_json_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("pins").join("pinned-threads.json");

    let mut pins = PinnedThreads::load(path.clone());
    pins.toggle("t-a");
    pins.toggle("t-b");
    pins.toggle("t-a");
    pins.save();

    let reloaded = PinnedThreads::load(path);
    assert_eq!(reloaded, pins);
    assert!(reloaded.is_pinned("t-b"));
    assert!(!reloaded.is_pinned("t-a"));
}

#[test]
fn load_of_a_missing_or_invalid_file_yields_an_empty_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.json");
    assert!(PinnedThreads::load(path.clone()).ids().next().is_none());

    std::fs::write(&path, "not json").expect("write junk");
    assert!(PinnedThreads::load(path).ids().next().is_none());
}
