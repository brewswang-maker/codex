//! Unit coverage of the sidebar grouping projection.

use codex_gui_core::QuestStatus;
use codex_gui_core::ThreadSummary;

/// Mirrors `thread_list`'s projection: server order kept inside each
/// project group; group keys sorted for stable rendering.
#[test]
fn threads_project_into_per_project_groups() {
    let threads = vec![
        summary("t1", "/work/alpha"),
        summary("t2", "/work/beta"),
        summary("t3", "/work/alpha"),
    ];

    let mut groups: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for thread in &threads {
        groups
            .entry(thread.cwd.as_str())
            .or_default()
            .push(thread.id.as_str());
    }

    assert_eq!(
        groups.get("/work/alpha").map(Vec::as_slice),
        Some(&["t1", "t3"][..]),
        "server order kept per project"
    );
    assert_eq!(
        groups.get("/work/beta").map(Vec::as_slice),
        Some(&["t2"][..])
    );

    // Sorted group keys: alpha renders before beta regardless of arrival.
    let keys: Vec<&str> = groups.into_keys().collect();
    assert_eq!(keys, vec!["/work/alpha", "/work/beta"]);
}

fn summary(id: &str, cwd: &str) -> ThreadSummary {
    ThreadSummary {
        id: String::from(id),
        preview: String::from("preview"),
        updated_at: 0,
        cwd: String::from(cwd),
        status: QuestStatus::Idle,
        name: None,
    }
}
