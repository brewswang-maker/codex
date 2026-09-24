//! Transcript reducer tests over real protocol notification payloads.
#![allow(clippy::expect_used, clippy::panic)]

use super::transcript::Entry;
use super::transcript::Transcript;
use codex_app_server_protocol::ServerNotification;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;

fn notification(payload: serde_json::Value) -> ServerNotification {
    from_value(payload).expect("notification payload decodes")
}

fn agent_started(id: &str) -> ServerNotification {
    notification(json!({
        "method": "item/started",
        "params": {
            "item": {"type": "agentMessage", "id": id, "text": ""},
            "threadId": "thread-1",
            "turnId": "turn-1",
            "startedAtMs": 1
        }
    }))
}

fn agent_delta(id: &str, delta: &str) -> ServerNotification {
    notification(json!({
        "method": "item/agentMessage/delta",
        "params": {"threadId": "thread-1", "turnId": "turn-1", "itemId": id, "delta": delta}
    }))
}

fn agent_completed(id: &str, text: &str) -> ServerNotification {
    notification(json!({
        "method": "item/completed",
        "params": {
            "item": {"type": "agentMessage", "id": id, "text": text},
            "threadId": "thread-1",
            "turnId": "turn-1",
            "completedAtMs": 2
        }
    }))
}

fn user_started(id: &str, text: &str) -> ServerNotification {
    notification(json!({
        "method": "item/started",
        "params": {
            "item": {
                "type": "userMessage",
                "id": id,
                "content": [{"type": "text", "text": text, "textElements": []}]
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "startedAtMs": 0
        }
    }))
}

#[test]
fn user_message_collects_local_image_paths() {
    let mut transcript = Transcript::default();

    transcript.apply(&notification(json!({
        "method": "item/started",
        "params": {
            "item": {
                "type": "userMessage",
                "id": "u2",
                "content": [
                    {"type": "text", "text": "what is this?", "textElements": []},
                    {"type": "localImage", "path": "/tmp/shot.png"}
                ]
            },
            "threadId": "thread-1",
            "turnId": "turn-2",
            "startedAtMs": 0
        }
    })));

    assert_eq!(
        transcript.entries(),
        &[Entry::UserMessage {
            id: "u2".to_string(),
            text: "what is this?".to_string(),
            images: vec!["/tmp/shot.png".to_string().into()],
        }],
    );
}

#[test]
fn streams_deltas_into_one_entry() {
    let mut transcript = Transcript::default();

    transcript.apply(&agent_started("m1"));
    transcript.apply(&agent_delta("m1", "Hello "));
    transcript.apply(&agent_delta("m1", "world"));

    assert_eq!(
        transcript.entries(),
        &[Entry::AgentMessage {
            id: "m1".to_string(),
            text: "Hello world".to_string(),
        },],
    );
}

#[test]
fn user_message_content_joins_text_fragments() {
    let mut transcript = Transcript::default();

    transcript.apply(&user_started("u1", "fix the bug"));

    assert_eq!(
        transcript.entries(),
        &[Entry::UserMessage {
            id: "u1".to_string(),
            text: "fix the bug".to_string(),
            images: Vec::new(),
        },],
    );
}

#[test]
fn completed_snapshot_wins_over_deltas() {
    let mut transcript = Transcript::default();

    transcript.apply(&agent_started("m1"));
    transcript.apply(&agent_delta("m1", "partial"));
    transcript.apply(&agent_completed("m1", "the final text"));

    assert_eq!(
        transcript.entries(),
        &[Entry::AgentMessage {
            id: "m1".to_string(),
            text: "the final text".to_string(),
        },],
    );
}

#[test]
fn delta_before_start_materializes_entry() {
    let mut transcript = Transcript::default();

    transcript.apply(&agent_delta("m1", "orphan"));
    transcript.apply(&agent_completed("m1", "orphaned text"));

    assert_eq!(
        transcript.entries(),
        &[Entry::AgentMessage {
            id: "m1".to_string(),
            text: "orphaned text".to_string(),
        },],
    );
}

#[test]
fn unrelated_notifications_are_ignored() {
    let mut transcript = Transcript::default();

    transcript.apply(&notification(json!({
        "method": "turn/plan/updated",
        "params": {"threadId": "thread-1", "turnId": "turn-1", "plan": []}
    })));
    transcript.apply(&notification(json!({
        "method": "thread/status/changed",
        "params": {"threadId": "thread-1", "status": {"type": "idle"}}
    })));

    assert!(transcript.entries().is_empty());
}

fn command_started(id: &str, command: &str) -> ServerNotification {
    notification(json!({
        "method": "item/started",
        "params": {
            "item": {
                "type": "commandExecution",
                "id": id,
                "command": command,
                "cwd": "/tmp",
                "status": "inProgress",
                "commandActions": []
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "startedAtMs": 1
        }
    }))
}

fn command_output_delta(id: &str, delta: &str) -> ServerNotification {
    notification(json!({
        "method": "item/commandExecution/outputDelta",
        "params": {"threadId": "thread-1", "turnId": "turn-1", "itemId": id, "delta": delta}
    }))
}

fn command_completed(id: &str, output: &str) -> ServerNotification {
    notification(json!({
        "method": "item/completed",
        "params": {
            "item": {
                "type": "commandExecution",
                "id": id,
                "command": "ls",
                "cwd": "/tmp",
                "status": "completed",
                "commandActions": [],
                "aggregatedOutput": output,
                "exitCode": 0,
                "durationMs": 5
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "completedAtMs": 2
        }
    }))
}

#[test]
fn command_execution_streams_output() {
    let mut transcript = Transcript::default();

    transcript.apply(&command_started("c1", "ls -la"));
    transcript.apply(&command_output_delta("c1", "file1"));
    transcript.apply(&command_output_delta("c1", " file2"));

    assert_eq!(
        transcript.entries(),
        &[Entry::CommandExecution {
            id: "c1".to_string(),
            command: "ls -la".to_string(),
            output: "file1 file2".to_string(),
            exit_code: None,
            duration_ms: None,
        },],
    );
}

#[test]
fn command_completed_snapshot_wins_over_deltas() {
    let mut transcript = Transcript::default();

    transcript.apply(&command_started("c1", "ls -la"));
    transcript.apply(&command_output_delta("c1", "partial"));
    transcript.apply(&command_completed("c1", "final output"));

    assert_eq!(
        transcript.entries(),
        &[Entry::CommandExecution {
            id: "c1".to_string(),
            command: "ls -la".to_string(),
            output: "final output".to_string(),
            // The completed snapshot carries the final status fields.
            exit_code: Some(0),
            duration_ms: Some(5),
        },],
    );
}

#[test]
fn orphan_command_output_is_skipped() {
    let mut transcript = Transcript::default();

    transcript.apply(&command_output_delta("ghost", "no such command"));

    assert!(transcript.entries().is_empty());
}

#[test]
fn replay_replaces_content_with_persisted_snapshots() {
    use codex_app_server_protocol::Turn;

    let mut transcript = Transcript::default();
    transcript.apply(&user_started("live-u", "live question"));
    transcript.apply(&agent_started("live-a"));
    transcript.apply(&agent_delta("live-a", "partial"));

    let turns: Vec<Turn> = from_value(json!([
        {
            "id": "turn-h1",
            "items": [
                {
                    "type": "userMessage",
                    "id": "hist-u",
                    "content": [{"type": "text", "text": "old question", "textElements": []}]
                },
                {"type": "agentMessage", "id": "hist-a", "text": "old answer"}
            ],
            "status": "completed",
            "error": null,
            "startedAt": null,
            "completedAt": null,
            "durationMs": null
        }
    ]))
    .expect("turns decode");

    transcript.replay(&turns);

    assert_eq!(
        transcript.entries(),
        &[
            Entry::UserMessage {
                id: "hist-u".to_string(),
                text: "old question".to_string(),
                images: Vec::new(),
            },
            Entry::AgentMessage {
                id: "hist-a".to_string(),
                text: "old answer".to_string(),
            },
        ],
    );
}

#[test]
fn replay_finalizes_command_snapshots() {
    use codex_app_server_protocol::Turn;

    let mut transcript = Transcript::default();
    let turns: Vec<Turn> = from_value(json!([
        {
            "id": "turn-h1",
            "items": [
                {
                    "type": "commandExecution",
                    "id": "hist-c",
                    "command": "ls -la",
                    "cwd": "/tmp",
                    "status": "completed",
                    "commandActions": [],
                    "aggregatedOutput": "file1",
                    "exitCode": 0,
                    "durationMs": 5
                }
            ],
            "status": "completed",
            "error": null,
            "startedAt": null,
            "completedAt": null,
            "durationMs": null
        }
    ]))
    .expect("turns decode");

    transcript.replay(&turns);

    assert_eq!(
        transcript.entries(),
        &[Entry::CommandExecution {
            id: "hist-c".to_string(),
            command: "ls -la".to_string(),
            output: "file1".to_string(),
            exit_code: Some(0),
            duration_ms: Some(5),
        },],
    );
}

fn plan_updated(steps: serde_json::Value) -> ServerNotification {
    notification(json!({
        "method": "turn/plan/updated",
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "explanation": "shipping the fix",
            "plan": steps
        }
    }))
}

#[test]
fn plan_notification_builds_display_steps() {
    let mut transcript = Transcript::default();

    transcript.apply(&plan_updated(json!([
        {"step": "reproduce", "status": "completed"},
        {"step": "patch", "status": "inProgress"},
        {"step": "test", "status": "pending"}
    ])));

    let plan = transcript.plan().expect("plan is live");
    assert_eq!(plan.explanation.as_deref(), Some("shipping the fix"));
    assert_eq!(
        plan.steps
            .iter()
            .map(|step| (step.step.as_str(), step.status.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("reproduce", "completed"),
            ("patch", "in progress"),
            ("test", "pending")
        ]
    );
}

#[test]
fn plan_is_replaced_and_replay_clears_it() {
    let mut transcript = Transcript::default();

    transcript.apply(&plan_updated(json!([{"step": "old", "status": "pending"}])));
    transcript.apply(&plan_updated(
        json!([{"step": "new", "status": "inProgress"}]),
    ));
    assert_eq!(transcript.plan().expect("plan is live").steps.len(), 1);

    transcript.replay(&[]);
    assert!(transcript.plan().is_none(), "replay clears the plan");
}

#[test]
fn diff_totals_count_plus_and_minus_lines_across_entries() {
    let mut transcript = Transcript::default();
    transcript.apply(&file_change_completed("f1"));
    // Second entry whose diff mixes headers, real changes, and context.
    transcript.apply(&notification(json!({
        "method": "item/completed",
        "params": {
            "item": {
                "type": "fileChange",
                "id": "f9",
                "changes": [
                    {"path": "lib.rs", "kind": {"type": "update", "movePath": null}, "diff": "@@ -1 +1 @@\n+added line\n-removed line\n context\n"}
                ],
                "status": "completed"
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "completedAtMs": 3
        }
    })));

    // f1's add snapshot carries one content line; f9 adds one + and one -.
    assert_eq!(transcript.diff_totals(), (2, 1));
}

#[test]
fn diff_totals_count_add_and_delete_content_lines() {
    // Add/delete snapshots carry raw file contents rather than a unified
    // diff (what apply_patch emits), so every non-empty line counts.
    let mut transcript = Transcript::default();
    transcript.apply(&notification(json!({
        "method": "item/completed",
        "params": {
            "item": {
                "type": "fileChange",
                "id": "f10",
                "changes": [
                    {"path": "new.txt", "kind": {"type": "add"}, "diff": "hello\nworld\n"},
                    {"path": "old.txt", "kind": {"type": "delete"}, "diff": "gone line\n"}
                ],
                "status": "completed"
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "completedAtMs": 4
        }
    })));

    assert_eq!(transcript.diff_totals(), (2, 1));
}

fn file_change_started(id: &str) -> ServerNotification {
    notification(json!({
        "method": "item/started",
        "params": {
            "item": {
                "type": "fileChange",
                "id": id,
                "changes": [
                    {"path": "src/main.rs", "kind": {"type": "add"}, "diff": "@@ -0,0 +1 @@\n+fn main() {}\n"},
                    {"path": "README.md", "kind": {"type": "update", "movePath": null}, "diff": "@@ -1 +1 @@\n-old\n+new\n"}
                ],
                "status": "inProgress"
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "startedAtMs": 1
        }
    }))
}

fn file_change_completed(id: &str) -> ServerNotification {
    notification(json!({
        "method": "item/completed",
        "params": {
            "item": {
                "type": "fileChange",
                "id": id,
                "changes": [
                    {"path": "src/main.rs", "kind": {"type": "add"}, "diff": "final diff\n"},
                    {"path": "README.md", "kind": {"type": "update", "movePath": null}, "diff": "final readme diff\n"},
                    {"path": "old.txt", "kind": {"type": "delete"}, "diff": ""}
                ],
                "status": "completed"
            },
            "threadId": "thread-1",
            "turnId": "turn-1",
            "completedAtMs": 2
        }
    }))
}

#[test]
fn file_change_items_land_as_one_summary_entry() {
    let mut transcript = Transcript::default();

    transcript.apply(&file_change_started("f1"));
    let entries = transcript.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(changes_of(&entries[0]).len(), 2);

    // The completed snapshot replaces the started list (and may add files).
    transcript.apply(&file_change_completed("f1"));
    let entries = transcript.entries();
    assert_eq!(entries.len(), 1, "no duplicate entry on completion");
    let changes = changes_of(&entries[0]);
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].kind, "add");
    assert_eq!(changes[1].kind, "update");
    assert_eq!(changes[2].kind, "delete");
    assert_eq!(changes[0].diff, "final diff\n");
}

#[test]
fn file_change_completion_without_start_still_lands() {
    let mut transcript = Transcript::default();

    transcript.apply(&file_change_completed("f2"));

    let entries = transcript.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(changes_of(&entries[0]).len(), 3);
}

/// The change list of the first (only) file-change entry.
fn changes_of(entry: &Entry) -> &[super::transcript::FileChangeRecord] {
    match entry {
        Entry::FileChange { changes, .. } => changes,
        other => panic!("unexpected entry: {other:?}"),
    }
}
