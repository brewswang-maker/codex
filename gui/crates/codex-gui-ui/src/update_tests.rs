#![allow(clippy::expect_used, clippy::panic)]

use super::update;
use crate::message::AppMode;
use crate::message::MenuId;
use crate::message::Message;
use crate::message::QuestScenario;
use crate::message::Reaction;
use crate::message::SidebarTab;
use crate::state::State;
use crate::state::Status;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::ThreadListResponse;
use codex_gui_bridge::Flags;
use codex_gui_bridge::GuiEvent;
use codex_gui_core::AccountBadge;
use codex_gui_core::GitInfo;
use codex_gui_core::GitStatus;
use codex_gui_core::QuestStatus;
use codex_gui_core::SettingField;
use codex_gui_core::SkillRow;
use iced_swdir_tree::DirectoryTreeEvent;
use iced_swdir_tree::SelectionMode;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;
use std::path::PathBuf;

fn notification(payload: serde_json::Value) -> GuiEvent {
    GuiEvent::Notification(
        from_value::<ServerNotification>(payload).expect("notification payload decodes"),
    )
}

fn thread_json(id: &str, preview: &str, cwd: &str) -> serde_json::Value {
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
        "cwd": cwd,
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

fn seed_sessions(state: &mut State, threads: Vec<serde_json::Value>) {
    let response: ThreadListResponse = from_value(json!({
        "data": threads,
        "nextCursor": null,
        "backwardsCursor": null
    }))
    .expect("list response decodes");
    let _ = update(state, Message::SessionsLoaded(Ok(response)));
}

#[test]
fn plan_notification_reaches_the_transcript() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/plan/updated",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "explanation": null,
                "plan": [{"step": "reproduce", "status": "inProgress"}]
            }
        }))),
    );

    let plan = state.transcript.plan().expect("plan is live");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].status, "in progress");
}

#[test]
fn warning_surfaces_as_the_banner() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "warning",
            "params": {"threadId": null, "message": "rate limit approaching"}
        }))),
    );

    let banner = state.status_board.error().expect("banner is up");
    assert_eq!(banner.message, "rate limit approaching");
    assert!(!banner.will_retry);
}

#[test]
fn reroute_surfaces_as_the_status_notice() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "model/rerouted",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "fromModel": "gpt-5",
                "toModel": "gpt-5-mini",
                "reason": "highRiskCyberActivity"
            }
        }))),
    );

    assert_eq!(
        state.status_board.notice(),
        Some("model rerouted: gpt-5 -> gpt-5-mini")
    );
}

#[test]
fn account_update_projects_the_chatgpt_badge() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "account/updated",
            "params": {"authMode": "chatgpt", "planType": "pro"}
        }))),
    );

    assert_eq!(
        state.status_board.account(),
        &AccountBadge::Chatgpt {
            email: None,
            plan: "pro".to_string(),
        }
    );
}

#[test]
fn reconnect_resets_runtime_state_and_bumps_the_epoch() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Disconnected("exit code 1".to_string());

    let _ = update(&mut state, Message::Reconnect);

    assert_eq!(state.connection_epoch, 1);
    assert_eq!(state.status, Status::Bootstrapping);
    assert!(state.client.is_none());
    assert!(state.thread_id.is_none());
    assert!(state.transcript.entries().is_empty());
    assert!(state.transcript.plan().is_none());
    assert!(
        state.approvals.pending().is_empty(),
        "stale approvals are dropped"
    );
}

#[test]
fn failed_turn_start_hands_the_prompt_back_and_raises_the_banner() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Thinking;

    let _ = update(
        &mut state,
        Message::TurnSettled {
            prompt: "hello".to_string(),
            images: Vec::new(),
            result: Err(codex_gui_bridge::Error::Request {
                code: -32000,
                message: "not logged in".to_string(),
            }),
        },
    );

    assert_eq!(state.composer, "hello");
    assert_eq!(state.status, Status::Ready);
    let banner = state.status_board.error().expect("banner is up");
    assert!(banner.message.contains("not logged in"));
}

#[test]
fn failed_turn_start_hands_the_attachments_back() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Thinking;

    let _ = update(
        &mut state,
        Message::TurnSettled {
            prompt: "look at this".to_string(),
            images: vec![PathBuf::from("/tmp/shot.png")],
            result: Err(codex_gui_bridge::Error::Request {
                code: -32000,
                message: "denied".to_string(),
            }),
        },
    );

    assert_eq!(state.composer, "look at this");
    assert_eq!(
        state.attachments.images,
        vec![PathBuf::from("/tmp/shot.png")]
    );
}

#[test]
fn dropped_images_attach_and_other_files_are_ignored() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::FileDropped(PathBuf::from("/tmp/shot.png")),
    );
    let _ = update(
        &mut state,
        Message::FileDropped("/tmp/notes.txt".to_string().into()),
    );
    // The same image twice stays a single chip.
    let _ = update(
        &mut state,
        Message::FileDropped(PathBuf::from("/tmp/shot.png")),
    );

    assert_eq!(
        state.attachments.images,
        vec![PathBuf::from("/tmp/shot.png")]
    );
    assert!(!state.attachments.hovered, "the drop settles the hover");
}

#[test]
fn attachment_chips_are_dismissible_by_position() {
    let mut state = State::new(Flags::default_app_server());
    state.attachments.images = vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")];

    let _ = update(&mut state, Message::AttachmentRemoved(0));
    let _ = update(&mut state, Message::AttachmentRemoved(9));

    assert_eq!(state.attachments.images, vec![PathBuf::from("/tmp/b.png")]);
}

#[test]
fn picked_folder_reopens_the_thread_inside_it() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("stale".to_string());
    state.status = Status::Ready;
    let stale = std::env::temp_dir().join("sim-stale-workspace.txt");
    let _ = state.editor.open(stale.clone());
    state.quest.workspace_menu = true;

    let _ = update(
        &mut state,
        Message::FolderPicked(Some(PathBuf::from("/tmp/proj"))),
    );

    // The stale thread is dropped and a fresh one requested; without a
    // client the start call itself is a no-op, leaving the bootstrapping
    // status on. The reply carries the confirmed cwd for the status bar.
    assert!(state.thread_id.is_none());
    assert!(state.transcript.entries().is_empty());
    assert_eq!(state.status, Status::Bootstrapping);
    // Previews from the old workspace go away with it, and the picker
    // closes along with the choice.
    assert_eq!(state.editor.tabs, Vec::<PathBuf>::new());
    assert_eq!(state.editor.active, None);
    assert!(!state.quest.workspace_menu);
}

#[test]
fn cancelled_folder_pick_keeps_the_live_session() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;

    let _ = update(&mut state, Message::FolderPicked(None));

    assert_eq!(state.thread_id, Some("live".to_string()));
    assert_eq!(state.status, Status::Ready);
}

#[test]
fn sidebar_tab_toggles_between_threads_and_files() {
    let mut state = State::new(Flags::default_app_server());
    // Qoder's editor shell opens on the file tree.
    assert_eq!(state.sidebar_tab, SidebarTab::Files);

    let _ = update(&mut state, Message::SidebarToggled);
    assert_eq!(state.sidebar_tab, SidebarTab::Threads);

    let _ = update(&mut state, Message::SidebarToggled);
    assert_eq!(state.sidebar_tab, SidebarTab::Files);
}

#[test]
fn tree_file_selection_opens_the_central_preview() {
    let mut state = State::new(Flags::default_app_server());
    let file = temp_file("codex-gui-tree-select", b"hello preview");
    let event = DirectoryTreeEvent::Selected(file.clone(), false, SelectionMode::Replace);

    let _ = update(&mut state, Message::Tree(event));

    assert_eq!(state.editor.active, Some(file.clone()));
    assert_eq!(state.editor.tabs, vec![file.clone()]);
    assert_eq!(
        state.editor.bodies.get(&file),
        Some(&crate::state::FileBody::Loading)
    );

    // Re-selecting the same file keeps a single tab and just focuses it.
    let _ = update(
        &mut state,
        Message::Tree(DirectoryTreeEvent::Selected(
            file.clone(),
            false,
            SelectionMode::Replace,
        )),
    );
    assert_eq!(state.editor.tabs, vec![file.clone()]);
    assert_eq!(state.editor.active, Some(file.clone()));

    let _ = std::fs::remove_file(&file);
}

#[test]
fn closing_the_active_preview_tab_activates_the_left_neighbor() {
    let mut state = State::new(Flags::default_app_server());
    let first = PathBuf::from("/tmp/first.rs");
    let second = PathBuf::from("/tmp/second.rs");
    assert!(state.editor.open(first.clone()));
    assert!(state.editor.open(second.clone()));

    let _ = update(&mut state, Message::FilePreviewClosed(second.clone()));

    assert_eq!(state.editor.tabs, vec![first.clone()]);
    assert_eq!(state.editor.active, Some(first));
    assert!(!state.editor.bodies.contains_key(&second));
}

#[test]
fn closing_an_inactive_preview_tab_keeps_the_active_one() {
    let mut state = State::new(Flags::default_app_server());
    let first = PathBuf::from("/tmp/first.rs");
    let second = PathBuf::from("/tmp/second.rs");
    assert!(state.editor.open(first.clone()));
    assert!(state.editor.open(second.clone()));

    let _ = update(&mut state, Message::FilePreviewClosed(first));

    assert_eq!(state.editor.tabs, vec![second.clone()]);
    assert_eq!(state.editor.active, Some(second));
}

/// A unique temporary file for tree-event fixtures.
fn temp_file(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{tag}-{}.txt", std::process::id()));
    std::fs::write(&path, bytes).expect("temp fixture writes");
    path
}

#[test]
fn activity_bar_tab_selection_sets_the_sidebar_pane() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::SidebarTab(SidebarTab::Files));
    assert_eq!(state.sidebar_tab, SidebarTab::Files);

    let _ = update(&mut state, Message::SidebarTab(SidebarTab::Threads));
    assert_eq!(state.sidebar_tab, SidebarTab::Threads);
}

#[test]
fn every_activity_bar_pane_is_reachable() {
    let mut state = State::new(Flags::default_app_server());
    for tab in [
        SidebarTab::Search,
        SidebarTab::SourceControl,
        SidebarTab::Wiki,
        SidebarTab::Debug,
        SidebarTab::Remote,
        SidebarTab::Extensions,
    ] {
        let _ = update(&mut state, Message::SidebarTab(tab));
        assert_eq!(state.sidebar_tab, tab);
    }
}

#[test]
fn mode_toggle_switches_between_editor_and_quest() {
    let mut state = State::new(Flags::default_app_server());
    assert_eq!(state.mode, AppMode::Editor);

    let _ = update(&mut state, Message::ModeToggled);
    assert_eq!(state.mode, AppMode::Quest);

    let _ = update(&mut state, Message::ModeToggled);
    assert_eq!(state.mode, AppMode::Editor);
}

#[test]
fn menu_toggle_opens_switches_and_closes_dropdowns() {
    let mut state = State::new(Flags::default_app_server());
    assert_eq!(state.menu, None);

    let _ = update(&mut state, Message::MenuToggled(MenuId::File));
    assert_eq!(state.menu, Some(MenuId::File), "first click opens");

    let _ = update(&mut state, Message::MenuToggled(MenuId::View));
    assert_eq!(state.menu, Some(MenuId::View), "another title takes over");

    let _ = update(&mut state, Message::MenuToggled(MenuId::View));
    assert_eq!(state.menu, None, "same title closes");

    let _ = update(&mut state, Message::MenuToggled(MenuId::Help));
    let _ = update(&mut state, Message::MenuClosed);
    assert_eq!(state.menu, None, "Esc dismisses the dropdown");
}

#[test]
fn mode_toggle_dismisses_the_open_menu() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::MenuToggled(MenuId::View));
    let _ = update(&mut state, Message::ModeToggled);

    assert_eq!(state.menu, None);
    assert_eq!(state.mode, AppMode::Quest);
}

#[test]
fn composer_cleared_empties_the_buffer() {
    let mut state = State::new(Flags::default_app_server());
    state.composer = String::from("draft text");

    let _ = update(&mut state, Message::ComposerCleared);

    assert!(state.composer.is_empty());
}

#[test]
fn copy_message_records_the_feedback_target() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::CopyMessage {
            id: String::from("m1"),
            text: String::from("hello world"),
        },
    );

    assert_eq!(state.copied_id.as_deref(), Some("m1"));

    // A later copy moves the feedback along.
    let _ = update(
        &mut state,
        Message::CopyMessage {
            id: String::from("m2"),
            text: String::from("again"),
        },
    );
    assert_eq!(state.copied_id.as_deref(), Some("m2"));
}

#[test]
fn reaction_toggle_replaces_then_clears() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::ReactionToggled {
            id: String::from("m1"),
            reaction: Reaction::Up,
        },
    );
    assert_eq!(state.reactions.get("m1"), Some(&Reaction::Up));

    // The opposite reaction replaces rather than stacking.
    let _ = update(
        &mut state,
        Message::ReactionToggled {
            id: String::from("m1"),
            reaction: Reaction::Down,
        },
    );
    assert_eq!(state.reactions.get("m1"), Some(&Reaction::Down));

    // Pressing the same reaction again clears it.
    let _ = update(
        &mut state,
        Message::ReactionToggled {
            id: String::from("m1"),
            reaction: Reaction::Down,
        },
    );
    assert!(!state.reactions.contains_key("m1"));
}

#[test]
fn surfaced_user_messages_gain_arrival_timestamps() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "item/started",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "startedAtMs": 0,
                "item": {
                    "type": "userMessage",
                    "id": "um-1",
                    "clientId": null,
                    "content": [{"type": "text", "text": "hello"}]
                }
            }
        }))),
    );

    assert!(state.user_times.contains_key("um-1"), "timestamp stamped");
}

#[test]
fn palette_toggle_and_query_reset_the_selection() {
    let mut state = State::new(Flags::default_app_server());
    assert!(!state.palette.open);

    let _ = update(&mut state, Message::PaletteToggled);
    assert!(state.palette.open);

    let _ = update(&mut state, Message::PaletteQueryChanged(String::from("se")));
    assert_eq!(state.palette.query, "se");

    let _ = update(&mut state, Message::PaletteToggled);
    assert!(!state.palette.open);
    assert!(state.palette.query.is_empty(), "closing clears the query");
    assert_eq!(state.palette.selected, 0, "closing resets the cursor");
}

#[test]
fn git_info_lands_in_state() {
    let mut state = State::new(Flags::default_app_server());

    let info = GitInfo {
        branch: Some(String::from("main")),
        changes: Default::default(),
    };
    let _ = update(&mut state, Message::GitInfoArrived(info));

    assert_eq!(state.git.branch.as_deref(), Some("main"));
}

#[test]
fn picked_folder_resets_the_tree_root() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::FolderPicked(Some(PathBuf::from("/tmp/proj"))),
    );

    assert_eq!(state.tree.root_path(), std::path::Path::new("/tmp/proj"));
    assert_eq!(state.status, Status::Bootstrapping);
}

#[test]
fn tree_events_route_back_through_the_tree_message() {
    let mut state = State::new(Flags::default_app_server());

    // The async scan task created by the widget is never driven here; the
    // update pass itself must be side-effect-free for the dispatch to hold.
    let _ = update(
        &mut state,
        Message::Tree(DirectoryTreeEvent::Toggled(PathBuf::from("/tmp"))),
    );

    assert_eq!(
        state.sidebar_tab,
        SidebarTab::Files,
        "tree events leave the tab state alone"
    );
}

#[test]
fn saving_model_setup_reopens_the_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("stale".to_string());
    state.settings.apply_config(&json!({"config": {}}));
    state.settings.select_provider_preset("deepseek");
    state
        .settings
        .set_field(SettingField::ProviderApiKey, "sk-test".to_string());

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // The stale thread is dropped and a fresh one requested; without a
    // client the start call itself is a no-op, leaving the bootstrapping
    // status on.
    assert!(state.thread_id.is_none());
    assert!(state.transcript.entries().is_empty());
    assert_eq!(state.status, Status::Bootstrapping);
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; fresh session started with the new model setup")
    );
}

#[test]
fn picking_a_skill_appends_the_mention_and_closes_the_picker() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.rows = vec![SkillRow {
        name: "mock-skill".to_string(),
        description: "the test skill".to_string(),
        scope: SkillScope::User,
        path: "/skills/mock-skill/SKILL.md".to_string().into(),
        enabled: true,
    }];
    state.skills.picker_open = true;
    state.composer = "review ".to_string();

    let _ = update(&mut state, Message::SkillPicked("mock-skill".to_string()));

    assert_eq!(state.composer, "review $mock-skill ");
    assert!(!state.skills.picker_open);
}

#[test]
fn skills_changed_notification_refreshes_the_inventory() {
    let mut state = State::new(Flags::default_app_server());
    state.client = None; // load_skills degrades to a no-op without a client

    let _ = update(
        &mut state,
        Message::Event(notification(
            json!({"method": "skills/changed", "params": {}}),
        )),
    );

    // The refresh path ran without touching the transcript.
    assert!(state.transcript.entries().is_empty());
}

#[test]
fn command_card_toggle_expands_then_collapses() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::CommandCardToggled(String::from("c1")));
    assert!(state.expanded_commands.contains("c1"));

    let _ = update(&mut state, Message::CommandCardToggled(String::from("c1")));
    assert!(!state.expanded_commands.contains("c1"));
}

#[test]
fn token_usage_notification_lands_on_the_current_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.client = None;
    state.thread_id = Some(String::from("thread-1"));

    let usage_payload = json!({
        "totalTokens": 12_400,
        "inputTokens": 10_000,
        "cachedInputTokens": 2_000,
        "cacheWriteInputTokens": 0,
        "outputTokens": 400,
        "reasoningOutputTokens": 0,
    });
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/tokenUsage/updated",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "tokenUsage": {
                    "total": usage_payload,
                    "last": usage_payload,
                    "modelContextWindow": 272_000
                }
            }
        }))),
    );

    let usage = state.token_usage.as_ref().expect("usage stored");
    assert_eq!(usage.total.total_tokens, 12_400);
    assert_eq!(usage.model_context_window, Some(272_000));
}

#[test]
fn token_usage_notification_from_other_threads_is_ignored() {
    let mut state = State::new(Flags::default_app_server());
    state.client = None;
    state.thread_id = Some(String::from("thread-1"));

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/tokenUsage/updated",
            "params": {
                "threadId": "thread-2",
                "turnId": "turn-1",
                "tokenUsage": {
                    "total": {"totalTokens": 1, "inputTokens": 1, "cachedInputTokens": 0,
                        "cacheWriteInputTokens": 0, "outputTokens": 0, "reasoningOutputTokens": 0},
                    "last": {"totalTokens": 1, "inputTokens": 1, "cachedInputTokens": 0,
                        "cacheWriteInputTokens": 0, "outputTokens": 0, "reasoningOutputTokens": 0},
                    "modelContextWindow": null
                }
            }
        }))),
    );

    assert!(state.token_usage.is_none());
}

#[test]
fn turn_diff_notification_lands_on_the_current_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.client = None;
    state.thread_id = Some(String::from("thread-1"));

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/diff/updated",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "diff": "diff --git a/x b/x\n+line\n"
            }
        }))),
    );
    assert_eq!(
        state.turn_diff.as_deref(),
        Some("diff --git a/x b/x\n+line\n")
    );

    // A stale turn's diff must not overwrite the live one.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/diff/updated",
            "params": {
                "threadId": "thread-2",
                "turnId": "turn-9",
                "diff": "stale"
            }
        }))),
    );
    assert_eq!(
        state.turn_diff.as_deref(),
        Some("diff --git a/x b/x\n+line\n")
    );
}

#[test]
fn diff_overlay_toggles_and_selects_files() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::DiffOverlayToggled);
    assert!(state.diff_overlay_open);

    let _ = update(&mut state, Message::DiffFileSelected(2));
    assert_eq!(state.diff_selected, Some(2));

    let _ = update(&mut state, Message::DiffOverlayToggled);
    assert!(!state.diff_overlay_open);
}

#[test]
fn git_file_toggles_map_working_tree_indexes() {
    let mut state = State::new(Flags::default_app_server());
    state
        .git
        .changes
        .insert(String::from("a.txt"), GitStatus::Modified);
    state
        .git
        .changes
        .insert(String::from("b.txt"), GitStatus::Added);

    let _ = update(&mut state, Message::GitFilesSelectAll(true));
    assert_eq!(state.git_overlay.selected.len(), 2);

    let _ = update(&mut state, Message::GitFileToggled(0));
    assert!(!state.git_overlay.selected.contains(&0));
    assert!(state.git_overlay.selected.contains(&1));

    let _ = update(&mut state, Message::GitFilesSelectAll(false));
    assert!(state.git_overlay.selected.is_empty());
}

#[test]
fn draft_thread_agent_message_fills_the_commit_message() {
    let mut state = State::new(Flags::default_app_server());
    state.client = None;
    state.git_overlay.draft_thread = Some(String::from("draft-t"));
    state.git_overlay.draft_diff = Some(String::from("diff"));
    state.git_overlay.status = crate::state::GitOverlayStatus::Drafting;

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "item/completed",
            "params": {
                "item": {"type": "agentMessage", "id": "d1", "text": "feat: add a thing"},
                "threadId": "draft-t",
                "turnId": "turn-d",
                "completedAtMs": 2
            }
        }))),
    );

    assert_eq!(state.git_overlay.message, "feat: add a thing");
    assert!(state.git_overlay.draft_thread.is_none());
    assert_eq!(
        state.git_overlay.status,
        crate::state::GitOverlayStatus::Idle
    );
    // The draft message never reached the main transcript.
    assert!(state.transcript.entries().is_empty());
}

#[test]
fn draft_thread_deltas_stay_out_of_the_main_transcript() {
    let mut state = State::new(Flags::default_app_server());
    state.git_overlay.draft_thread = Some(String::from("draft-t"));

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "item/started",
            "params": {
                "item": {"type": "agentMessage", "id": "d1", "text": ""},
                "threadId": "draft-t",
                "turnId": "turn-d",
                "startedAtMs": 1
            }
        }))),
    );
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "item/agentMessage/delta",
            "params": {"threadId": "draft-t", "turnId": "turn-d", "itemId": "d1", "delta": "hi"}
        }))),
    );

    assert!(state.transcript.entries().is_empty());
    assert!(state.markdowns.is_empty());
}

#[test]
fn draft_turn_completed_without_a_message_fails_the_draft() {
    let mut state = State::new(Flags::default_app_server());
    state.git_overlay.draft_thread = Some(String::from("draft-t"));
    state.git_overlay.status = crate::state::GitOverlayStatus::Drafting;

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/completed",
            "params": {
                "threadId": "draft-t",
                "turn": {
                    "id": "turn-d",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": null,
                    "completedAt": null,
                    "durationMs": null
                }
            }
        }))),
    );

    assert!(state.git_overlay.draft_thread.is_none());
    assert_eq!(
        state.git_overlay.status,
        crate::state::GitOverlayStatus::Failed(String::from("draft turn produced no message"))
    );
}

#[test]
fn main_thread_traffic_still_flows_while_a_draft_is_running() {
    let mut state = State::new(Flags::default_app_server());
    state.git_overlay.draft_thread = Some(String::from("draft-t"));

    // The live thread's plan still reaches the transcript mid-draft.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/plan/updated",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "explanation": null,
                "plan": [{"step": "reproduce", "status": "inProgress"}]
            }
        }))),
    );

    assert!(state.transcript.plan().is_some());
}

#[test]
fn turn_completed_reopens_the_composer_on_the_live_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some(String::from("t1"));
    state.status = Status::Thinking;
    state.composer = String::from("still typed text");

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/completed",
            "params": {
                "threadId": "t1",
                "turn": {
                    "id": "turn-1",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": null,
                    "completedAt": null,
                    "durationMs": null
                }
            }
        }))),
    );

    assert_eq!(state.status, Status::Ready);
}

#[test]
fn turn_completed_on_other_threads_leaves_the_status_alone() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some(String::from("t1"));
    state.status = Status::Thinking;

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/completed",
            "params": {
                "threadId": "other",
                "turn": {
                    "id": "turn-9",
                    "items": [],
                    "status": "completed",
                    "error": null,
                    "startedAt": null,
                    "completedAt": null,
                    "durationMs": null
                }
            }
        }))),
    );

    assert_eq!(state.status, Status::Thinking);
}

#[test]
fn quest_create_and_scenario_pick_starts_with_the_template() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.thread_id = Some(String::from("thread-old"));

    let _ = update(&mut state, Message::QuestCreatePressed);
    assert!(state.quest.picker_open, "create raises the scenario picker");

    let _ = update(
        &mut state,
        Message::QuestScenarioPicked(Some(QuestScenario::Spec)),
    );
    assert!(!state.quest.picker_open, "picking closes the picker");
    assert_eq!(state.quest.scenario, Some(QuestScenario::Spec));
    assert_eq!(state.thread_id, None, "a fresh thread is opened");
    assert_eq!(state.composer, QuestScenario::Spec.template());
}

#[test]
fn quest_scenario_pick_keeps_an_existing_composer_draft() {
    let mut state = State::new(Flags::default_app_server());
    state.composer = String::from("my draft");

    let _ = update(
        &mut state,
        Message::QuestScenarioPicked(Some(QuestScenario::Tool)),
    );

    assert_eq!(state.composer, "my draft", "drafts stay untouched");
    assert_eq!(state.quest.scenario, Some(QuestScenario::Tool));
}

#[test]
fn quest_scenario_cancel_leaves_the_thread_alone() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.thread_id = Some(String::from("thread-1"));
    state.quest.picker_open = true;

    let _ = update(&mut state, Message::QuestScenarioPicked(None));

    assert!(!state.quest.picker_open);
    assert_eq!(state.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(state.quest.scenario, None);
}

#[test]
fn quest_menu_toggles_and_arms_the_delete_confirmation() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "first", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::QuestMenuToggled(String::from("thread-1")),
    );
    assert_eq!(state.quest.menu.thread_id.as_deref(), Some("thread-1"));
    assert!(!state.quest.menu.confirm_delete, "the menu opens unarmed");

    let _ = update(&mut state, Message::QuestDeleteArmed);
    assert!(state.quest.menu.confirm_delete, "the first press arms");

    let _ = update(
        &mut state,
        Message::QuestDeleteRequested(String::from("thread-1")),
    );
    assert_eq!(
        state.quest.menu.thread_id, None,
        "confirming closes the menu"
    );
    assert!(
        state.sessions.threads().is_empty(),
        "the row drops immediately"
    );
}

#[test]
fn quest_menu_toggle_on_the_open_row_closes_it() {
    let mut state = State::new(Flags::default_app_server());
    let _ = update(&mut state, Message::QuestMenuToggled(String::from("t1")));
    let _ = update(&mut state, Message::QuestMenuToggled(String::from("t1")));
    assert_eq!(state.quest.menu.thread_id, None);
}

#[test]
fn quest_rename_dialog_seeds_the_draft_and_cancels() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "fix login", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::QuestRenameRequested(String::from("thread-1")),
    );
    let rename = state.quest.rename.as_ref().expect("the dialog is open");
    assert_eq!(rename.draft, "fix login", "the draft seeds from the label");

    let _ = update(
        &mut state,
        Message::QuestRenameDraftChanged(String::from("renamed")),
    );
    assert_eq!(state.quest.rename.as_ref().expect("open").draft, "renamed");

    let _ = update(&mut state, Message::QuestRenameCancelled);
    assert!(state.quest.rename.is_none(), "cancel closes the dialog");
}

#[test]
fn quest_rename_confirm_trims_and_applies_the_name() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "fix login", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::QuestRenameRequested(String::from("thread-1")),
    );
    let _ = update(
        &mut state,
        Message::QuestRenameDraftChanged(String::from("  polished  ")),
    );
    let _ = update(&mut state, Message::QuestRenameConfirmed);
    assert!(state.quest.rename.is_none(), "confirm closes the dialog");

    let _ = update(
        &mut state,
        Message::QuestRenamed {
            thread_id: String::from("thread-1"),
            name: String::from("polished"),
            result: Ok(json!(null)),
        },
    );
    assert_eq!(state.sessions.threads()[0].label(), "polished");
}

#[test]
fn quest_rename_with_an_empty_draft_keeps_the_dialog_open() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "fix login", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::QuestRenameRequested(String::from("thread-1")),
    );
    let _ = update(
        &mut state,
        Message::QuestRenameDraftChanged(String::from("   ")),
    );
    let _ = update(&mut state, Message::QuestRenameConfirmed);
    assert!(
        state.quest.rename.is_some(),
        "an empty name never clears the label"
    );
}

#[test]
fn quest_board_toggles_and_picks_a_project_tab() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![
            thread_json("thread-1", "alpha", "/repo/a"),
            thread_json("thread-2", "beta", "/repo/b"),
        ],
    );

    let _ = update(&mut state, Message::QuestBoardToggled);
    assert!(state.quest.board_open);

    let _ = update(
        &mut state,
        Message::QuestBoardProjectPicked(Some(String::from("/repo/b"))),
    );
    assert_eq!(state.quest.board_project.as_deref(), Some("/repo/b"));

    let _ = update(&mut state, Message::QuestOverlaysClosed);
    assert!(!state.quest.board_open, "Esc dismisses the board");
}

#[test]
fn quest_workspace_menu_toggles_and_esc_closes() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::QuestWorkspaceToggled);
    assert!(state.quest.workspace_menu, "the chip opens the dropdown");

    let _ = update(&mut state, Message::QuestOverlaysClosed);
    assert!(!state.quest.workspace_menu, "Esc dismisses the dropdown");
}

#[test]
fn quest_list_expanded_toggles_the_row_cap() {
    let mut state = State::new(Flags::default_app_server());
    assert!(!state.quest.list_expanded);

    let _ = update(&mut state, Message::QuestListExpanded);
    assert!(state.quest.list_expanded, "show-more expands the list");

    let _ = update(&mut state, Message::QuestListExpanded);
    assert!(!state.quest.list_expanded, "a second press collapses it");
}

#[test]
fn mode_toggle_closes_the_quest_overlays() {
    let mut state = State::new(Flags::default_app_server());
    state.mode = AppMode::Quest;
    state.quest.picker_open = true;
    state.quest.board_open = true;

    let _ = update(&mut state, Message::ModeToggled);

    assert_eq!(state.mode, AppMode::Editor);
    assert!(!state.quest.picker_open);
    assert!(!state.quest.board_open);
}

#[test]
fn quest_status_notification_updates_the_row() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "running", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/status/changed",
            "params": {
                "threadId": "thread-1",
                "status": {"type": "active", "activeFlags": ["waitingOnUserInput"]}
            }
        }))),
    );

    assert_eq!(state.sessions.threads()[0].status, QuestStatus::Waiting);
}

#[test]
fn quest_name_notification_renames_the_row() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(
        &mut state,
        vec![thread_json("thread-1", "old name", "/repo/a")],
    );

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/name/updated",
            "params": {"threadId": "thread-1", "threadName": "new name"}
        }))),
    );

    assert_eq!(state.sessions.threads()[0].label(), "new name");
}

#[test]
fn thread_deleted_notification_drops_the_row() {
    let mut state = State::new(Flags::default_app_server());
    seed_sessions(&mut state, vec![thread_json("thread-1", "gone", "/repo/a")]);

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/deleted",
            "params": {"threadId": "thread-1"}
        }))),
    );

    assert!(state.sessions.threads().is_empty());
}
