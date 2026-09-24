//! Unit coverage of the transcript search.

use super::SearchField;
use super::search;
use crate::transcript::Entry;

fn user(id: &str, text: &str) -> Entry {
    Entry::UserMessage {
        id: String::from(id),
        text: String::from(text),
        images: Vec::new(),
    }
}

fn agent(id: &str, text: &str) -> Entry {
    Entry::AgentMessage {
        id: String::from(id),
        text: String::from(text),
    }
}

fn command(id: &str, command_line: &str, output: &str) -> Entry {
    Entry::CommandExecution {
        id: String::from(id),
        command: String::from(command_line),
        output: String::from(output),
        exit_code: Some(0),
        duration_ms: Some(12),
    }
}

#[test]
fn empty_query_yields_no_hits() {
    let entries = vec![user("u1", "hello world")];
    assert!(search(&entries, "").is_empty());
    assert!(search(&entries, "   ").is_empty());
}

#[test]
fn matching_is_case_insensitive() {
    let entries = vec![agent("a1", "The Render Pipeline is lazy")];
    let hits = search(&entries, "render pipeline");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry_id, "a1");
    assert_eq!(hits[0].field, SearchField::AgentText);
}

#[test]
fn one_hit_per_entry_in_transcript_order() {
    let entries = vec![
        user("u1", "fix the login bug"),
        agent("a1", "working on the login bug"),
        command("c1", "cargo test", "login test passed"),
    ];
    let hits = search(&entries, "login");
    assert_eq!(hits.len(), 3);
    assert_eq!(
        hits.iter().map(|hit| hit.entry_index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn command_search_checks_the_command_line_before_the_output() {
    let entries = vec![command("c1", "rg PATTERN src", "unrelated output")];
    let hits = search(&entries, "pattern");
    assert_eq!(hits[0].field, SearchField::Command);

    let hits = search(&entries, "unrelated");
    assert_eq!(hits[0].field, SearchField::Output);
}

#[test]
fn file_change_search_matches_paths_only() {
    let entry = Entry::FileChange {
        id: String::from("f1"),
        changes: vec![crate::transcript::FileChangeRecord {
            path: String::from("crates/app/src/main.rs"),
            kind: String::from("update"),
            diff: String::from("+++ this diff body mentions needle but is not searched"),
        }],
    };
    let hits = search(&[entry], "main.rs");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].field, SearchField::FilePath);

    let entry = Entry::FileChange {
        id: String::from("f1"),
        changes: vec![crate::transcript::FileChangeRecord {
            path: String::from("src/lib.rs"),
            kind: String::from("update"),
            diff: String::from("needle only in the diff"),
        }],
    };
    assert!(search(&[entry], "needle").is_empty());
}

#[test]
fn snippet_carries_context_windows_and_match_offset() {
    let long = format!("{}TARGET{}", "x".repeat(100), "y".repeat(200));
    let entries = vec![user("u1", &long)];
    let hits = search(&entries, "target");
    let hit = &hits[0];
    assert_eq!(hit.field, SearchField::UserText);
    // Leading ellipsis plus the trimmed context window.
    assert!(hit.snippet.starts_with('…'));
    assert_eq!(&hit.snippet[hit.match_start..hit.match_start + 6], "TARGET");
}

#[test]
fn snippet_of_a_prefix_match_has_no_leading_ellipsis() {
    // The tail must exceed SNIPPET_AFTER for the trailing ellipsis to show.
    let body = format!("cargo build --release {}", "y".repeat(120));
    let entries = vec![user("u1", &body)];
    let hits = search(&entries, "cargo");
    let hit = &hits[0];
    assert!(!hit.snippet.starts_with('…'));
    assert_eq!(hit.match_start, 0);
    assert!(hit.snippet.ends_with('…'));
}

#[test]
fn multibyte_text_keeps_char_boundaries() {
    let entries = vec![user(
        "u1",
        "你好世界，这是一个用于测试的多字节文本片段，目标词在这里",
    )];
    let hits = search(&entries, "目标词");
    let hit = &hits[0];
    assert_eq!(
        hit.snippet
            .get(hit.match_start..hit.match_start + "目标词".len()),
        Some("目标词")
    );
}

#[test]
fn non_matching_entries_are_skipped() {
    let entries = vec![
        user("u1", "nothing here"),
        agent("a1", "find the needle here"),
    ];
    let hits = search(&entries, "needle");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry_index, 1);
    assert_eq!(hits[0].entry_id, "a1");
}
