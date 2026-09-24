//! Round-trip coverage of the persisted recent-projects store.

use super::RecentProjects;

#[test]
fn remember_deduplicates_moves_to_front_and_caps() {
    let mut store = RecentProjects {
        path: std::path::PathBuf::from("/tmp/nonexistent-recent.json"),
        entries: Vec::new(),
    };

    for path in ["/a", "/b", "/c"] {
        store.remember(path);
    }
    assert_eq!(store.entries(), ["/c", "/b", "/a"]);

    // Re-opening /a bumps it back to the front without duplicating.
    store.remember("/a");
    assert_eq!(store.entries(), ["/a", "/c", "/b"]);
}

#[test]
fn remember_caps_the_visible_history() {
    let mut store = RecentProjects {
        path: std::path::PathBuf::from("/tmp/nonexistent-recent.json"),
        entries: Vec::new(),
    };

    for index in 0..super::MAX_ENTRIES + 3 {
        store.remember(&format!("/p{index}"));
    }
    assert_eq!(store.entries().len(), super::MAX_ENTRIES);
    assert_eq!(store.entries().first().map(String::as_str), Some("/p10"));
}

#[test]
fn json_round_trip_through_a_temp_file() {
    let path = std::env::temp_dir().join("codex-gui-recent-projects-test.json");
    let _ignored = std::fs::remove_file(&path);

    let mut store = RecentProjects::load(path.clone());
    store.remember("/work/gui");
    store.save();

    let reloaded = RecentProjects::load(path.clone());
    assert_eq!(reloaded.entries(), ["/work/gui"]);

    let _ignored = std::fs::remove_file(&path);
}
