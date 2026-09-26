#![allow(clippy::expect_used, clippy::panic)]

use super::Sessions;
use crate::QuestStatus;
use crate::SessionHistory;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadResumeResponse;
use pretty_assertions::assert_eq;
use serde_json::json;

fn thread_json(id: &str, preview: &str) -> serde_json::Value {
    json!({
        "id": id,
        "extra": null,
        "sessionId": "session-1",
        "forkedFromId": null,
        "parentThreadId": null,
        "preview": preview,
        "ephemeral": false,
        "modelProvider": "mock",
        "model": null,
        "reasoningEffort": null,
        "createdAt": 1,
        "updatedAt": 2,
        "recencyAt": null,
        "status": {"type": "idle"},
        "path": null,
        "cwd": "/tmp",
        "cliVersion": "0.0.0",
        "originator": null,
        "source": "cli",
        "canAcceptDirectInput": null,
        "threadSource": null,
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": null,
        "daybreakEnabled": null,
        "turns": []
    })
}

fn list_response(threads: Vec<serde_json::Value>, next_cursor: Option<&str>) -> ThreadListResponse {
    serde_json::from_value(json!({
        "data": threads,
        "nextCursor": next_cursor,
        "backwardsCursor": null
    }))
    .expect("list response decodes")
}

#[test]
fn apply_list_projects_thread_payloads() {
    let mut sessions = Sessions::default();
    sessions.apply_list(&list_response(
        vec![
            thread_json("thread-1", "first"),
            thread_json("thread-2", "second"),
        ],
        Some("cursor-2"),
    ));

    let previews: Vec<_> = sessions
        .threads()
        .iter()
        .map(|thread| thread.preview.as_str())
        .collect();
    assert_eq!(previews, ["first", "second"]);
    assert!(sessions.has_more());
}

#[test]
fn apply_list_without_cursor_says_no_more_pages() {
    let mut sessions = Sessions::default();
    sessions.apply_list(&list_response(vec![thread_json("thread-1", "only")], None));

    assert_eq!(sessions.threads().len(), 1);
    assert!(!sessions.has_more());
}

#[test]
fn remove_drops_only_the_matching_thread() {
    let mut sessions = Sessions::default();
    sessions.apply_list(&list_response(
        vec![
            thread_json("thread-1", "keep"),
            thread_json("thread-2", "drop"),
        ],
        None,
    ));

    assert!(sessions.remove("thread-2"));
    assert!(!sessions.remove("thread-2"), "second removal is a no-op");

    let ids: Vec<_> = sessions.threads().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["thread-1"]);
}

#[test]
fn session_history_projection_carries_id_and_turns() {
    let thread = thread_json("thread-9", "history");
    let resume: ThreadResumeResponse = serde_json::from_value(json!({
        "thread": thread,
        "model": "mock-model",
        "modelProvider": "mock",
        "serviceTier": null,
        "disabledPluginIds": [],
        "cwd": "/tmp",
        "runtimeWorkspaceRoots": [],
        "instructionSources": [],
        "approvalPolicy": "on-request",
        "approvalsReviewer": "user",
        "sandbox": {"type": "readOnly"},
        "activePermissionProfile": null,
        "reasoningEffort": null,
        "collaborationMode": null,
        "multiAgentMode": "explicitRequestOnly",
        "initialTurnsPage": null,
        "turnsBackwardsCursor": null,
        "itemsBackwardsCursor": null
    }))
    .expect("resume response decodes");

    let history = SessionHistory::from_resume(&resume);
    assert_eq!(history.thread_id, "thread-9");
    assert_eq!(history.model_provider, "mock");
    assert_eq!(history.model, "mock-model");
    assert_eq!(history.turns.len(), 0);
}

#[test]
fn organized_sidebar_partitions_pinned_and_filters_blanks() {
    use crate::ThreadSummary;
    use crate::organized_sidebar;
    use std::collections::BTreeSet;

    let threads = vec![
        ThreadSummary {
            id: String::from("t1"),
            preview: String::from("fix login bug"),
            updated_at: 1,
            cwd: String::from("/repo/a"),
            status: QuestStatus::Idle,
            name: None,
        },
        ThreadSummary {
            id: String::from("t2"),
            preview: String::from("write docs"),
            updated_at: 2,
            cwd: String::from("/repo/b"),
            status: QuestStatus::Idle,
            name: None,
        },
        ThreadSummary {
            id: String::from("t3"),
            preview: String::from("login tests"),
            updated_at: 3,
            cwd: String::from("/repo/a"),
            status: QuestStatus::Idle,
            name: None,
        },
    ];

    // Blank filter keeps everything; pinned ids move out of the groups.
    let mut pins = BTreeSet::new();
    pins.insert(String::from("t3"));
    let (pinned, groups) = organized_sidebar(&threads, &pins, "  ");
    assert_eq!(
        pinned.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["t3"]
    );
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups["/repo/a"]
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["t1"]
    );

    // The filter matches preview text case-insensitively.
    let (pinned, groups) = organized_sidebar(&threads, &pins, "LOGIN");
    assert_eq!(
        pinned.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["t3"]
    );
    assert_eq!(
        groups["/repo/a"]
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["t1"]
    );
    assert!(!groups.contains_key("/repo/b"), "docs thread filtered out");

    // The filter also matches the working directory.
    let (pinned, groups) = organized_sidebar(&threads, &pins, "/repo/b");
    assert!(pinned.is_empty());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups["/repo/b"][0].id, "t2");
}

#[test]
fn apply_list_projects_lifecycle_status_and_name() {
    let mut sessions = Sessions::default();
    let mut running = thread_json("thread-run", "running task");
    running["status"] = json!({"type": "active", "activeFlags": ["waitingOnApproval"]});
    running["name"] = json!("Renamed quest");
    sessions.apply_list(&list_response(vec![running], None));

    let thread = &sessions.threads()[0];
    assert_eq!(thread.status, QuestStatus::Waiting);
    assert_eq!(thread.name.as_deref(), Some("Renamed quest"));
    assert_eq!(thread.label(), "Renamed quest");
}

#[test]
fn status_projection_prefers_waiting_flags_over_active() {
    use codex_app_server_protocol::ThreadActiveFlag;
    use codex_app_server_protocol::ThreadStatus;

    assert_eq!(
        QuestStatus::from(&ThreadStatus::Idle),
        QuestStatus::Idle,
        "idle threads park"
    );
    assert_eq!(
        QuestStatus::from(&ThreadStatus::NotLoaded),
        QuestStatus::Idle,
        "not-loaded threads park"
    );
    assert_eq!(
        QuestStatus::from(&ThreadStatus::SystemError),
        QuestStatus::Error,
        "system errors surface as errors"
    );
    assert_eq!(
        QuestStatus::from(&ThreadStatus::Active {
            active_flags: Vec::new(),
        }),
        QuestStatus::Running,
        "a plain active run is running"
    );
    assert_eq!(
        QuestStatus::from(&ThreadStatus::Active {
            active_flags: vec![ThreadActiveFlag::WaitingOnUserInput],
        }),
        QuestStatus::Waiting,
        "a waiting flag wins over the active run"
    );
}

#[test]
fn apply_status_and_name_touch_only_the_matching_row() {
    let mut sessions = Sessions::default();
    sessions.apply_list(&list_response(
        vec![
            thread_json("thread-1", "first"),
            thread_json("thread-2", "second"),
        ],
        None,
    ));

    assert!(sessions.apply_status("thread-2", QuestStatus::Running));
    assert!(!sessions.apply_status("thread-9", QuestStatus::Running));
    assert!(sessions.apply_name("thread-2", Some(String::from("renamed"))));
    assert!(!sessions.apply_name("thread-9", None));

    assert_eq!(sessions.threads()[0].status, QuestStatus::Idle);
    assert_eq!(sessions.threads()[1].status, QuestStatus::Running);
    assert_eq!(sessions.threads()[1].label(), "renamed");
}

#[test]
fn blank_previews_fall_back_to_the_untitled_label() {
    let mut sessions = Sessions::default();
    sessions.apply_list(&list_response(vec![thread_json("thread-1", "")], None));

    assert_eq!(sessions.threads()[0].label(), "(untitled)");
}
