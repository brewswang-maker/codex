#![allow(clippy::expect_used, clippy::panic)]

use super::update;
use crate::message::AppMode;
use crate::message::Bootstrap;
use crate::message::MenuId;
use crate::message::Message;
use crate::message::QuestScenario;
use crate::message::Reaction;
use crate::message::SidebarTab;
use crate::message::SkillImportTarget;
use crate::state::PendingModelApply;
use crate::state::State;
use crate::state::Status;
use codex_app_server_protocol::FuzzyFileSearchMatchType;
use codex_app_server_protocol::FuzzyFileSearchResponse;
use codex_app_server_protocol::FuzzyFileSearchResult;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::ListMcpServerStatusResponse;
use codex_app_server_protocol::McpAuthStatus;
use codex_app_server_protocol::McpServerConnectionStatus;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerOauthLoginResponse;
use codex_app_server_protocol::McpServerStatus;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginReadResponse;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::ThreadListResponse;
use codex_gui_bridge::Flags;
use codex_gui_bridge::GuiEvent;
use codex_gui_core::AccountBadge;
use codex_gui_core::GitInfo;
use codex_gui_core::GitStatus;
use codex_gui_core::QuestStatus;
use codex_gui_core::SessionHistory;
use codex_gui_core::SettingField;
use codex_gui_core::SkillAdd;
use codex_gui_core::SkillEdit;
use codex_gui_core::SkillNotice;
use codex_gui_core::SkillRow;
use codex_gui_core::SkillsTab;
use iced_swdir_tree::DirectoryTreeEvent;
use iced_swdir_tree::SelectionMode;
use pretty_assertions::assert_eq;
use serde_json::from_value;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

fn notification(payload: serde_json::Value) -> GuiEvent {
    GuiEvent::Notification(
        from_value::<ServerNotification>(payload).expect("notification payload decodes"),
    )
}

/// Seeds one visible transcript entry through the live event path.
fn seed_agent_message(state: &mut State, thread_id: &str) {
    let _ = update(
        state,
        Message::Event(notification(json!({
            "method": "item/started",
            "params": {
                "threadId": thread_id,
                "turnId": "turn-1",
                "startedAtMs": 0,
                "item": {"type": "agentMessage", "id": "am-1", "text": "hello"}
            }
        }))),
    );
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

/// One `thread/settings/updated` frame for the given thread and effective
/// model, shaped like the app-server's camelCase wire form.
fn thread_settings_json(thread_id: &str, model: &str) -> serde_json::Value {
    json!({
        "method": "thread/settings/updated",
        "params": {
            "threadId": thread_id,
            "threadSettings": {
                "disabledPluginIds": [],
                "cwd": "/tmp",
                "approvalPolicy": "on-request",
                "approvalsReviewer": "user",
                "sandboxPolicy": {"type": "workspaceWrite"},
                "activePermissionProfile": null,
                "model": model,
                "modelProvider": "bigmodel",
                "serviceTier": null,
                "effort": null,
                "summary": null,
                "collaborationMode": {
                    "mode": "default",
                    "settings": {
                        "model": model,
                        "reasoning_effort": null,
                        "developer_instructions": null
                    }
                },
                "multiAgentMode": "explicitRequestOnly",
                "personality": null
            }
        }
    })
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
fn guardian_warning_surfaces_as_the_banner() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "guardianWarning",
            "params": {"threadId": "thread-1", "message": "guardian flagged a command"}
        }))),
    );

    let banner = state.status_board.error().expect("banner is up");
    assert_eq!(banner.message, "guardian flagged a command");
}

#[test]
fn automatic_approval_guardian_notes_stay_out_of_the_banner() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "guardianWarning",
            "params": {
                "threadId": "thread-1",
                "message": "Automatic approval review approved (low risk)"
            }
        }))),
    );

    assert!(state.status_board.error().is_none());
}

#[test]
fn config_warning_carries_its_details_into_the_banner() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "configWarning",
            "params": {
                "summary": "unknown config key",
                "details": "unrecognized field `frobnicate`"
            }
        }))),
    );

    let banner = state.status_board.error().expect("banner is up");
    assert_eq!(
        banner.message,
        "unknown config key: unrecognized field `frobnicate`"
    );
}

#[test]
fn config_warning_banner_dismisses_via_the_banner_action() {
    let mut state = State::new(Flags::default_app_server());

    // The startup warning app-server sends after `initialize`.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "configWarning",
            "params": {
                "summary": "Project-local config, hooks, and exec policies are disabled",
                "details": "add the folder as a trusted project"
            }
        }))),
    );
    assert!(state.status_board.error().is_some(), "the warning is up");

    let _ = update(&mut state, Message::ErrorDismissed);

    assert!(
        state.status_board.error().is_none(),
        "the banner action clears it"
    );
}

#[test]
fn thread_settings_update_syncs_only_the_displayed_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("thread-1".to_string());

    let _ = update(
        &mut state,
        Message::Event(notification(thread_settings_json("thread-1", "glm-5.3"))),
    );
    assert_eq!(state.status_board.current_model(), Some("glm-5.3"));

    // Background threads' settings must not repaint the status bar.
    let _ = update(
        &mut state,
        Message::Event(notification(thread_settings_json(
            "thread-2",
            "other-model",
        ))),
    );
    assert_eq!(state.status_board.current_model(), Some("glm-5.3"));
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
    state.codex_home = Some(PathBuf::from("/tmp/codex-home"));
    state.active_turn = Some("turn-1".to_string());
    assert!(
        state.mentions.refresh("@lib").is_some(),
        "the popup is live"
    );
    state.mcp_servers = vec![mcp_server_status(
        "filesystem",
        McpServerConnectionStatus::Connected,
    )];
    state.mcp_loaded = true;

    let _ = update(&mut state, Message::Reconnect);

    assert_eq!(state.connection_epoch, 1);
    assert_eq!(state.status, Status::Bootstrapping);
    assert!(state.client.is_none());
    assert!(state.thread_id.is_none());
    assert!(state.codex_home.is_none(), "the skills root is dropped");
    assert!(state.transcript.entries().is_empty());
    assert!(state.transcript.plan().is_none());
    assert!(
        state.approvals.pending().is_empty(),
        "stale approvals are dropped"
    );
    assert!(state.active_turn.is_none(), "the live turn is dropped");
    assert!(!state.mentions.is_open(), "the mention popup is dropped");
    assert!(state.mcp_servers.is_empty(), "the MCP inventory is dropped");
    assert!(!state.mcp_loaded);
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
        commits: Vec::new(),
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
fn panel_model_edit_flips_the_running_thread_in_place() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm"
            }
        }))),
    );
    seed_agent_message(&mut state, "live");
    state
        .settings
        .set_field(SettingField::Model, "glm-5.3-flashx".to_string());

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // Same vendor: the panel edit flips the model without touching the
    // conversation.
    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Model switched to glm-5.3-flashx")
    );
}

#[test]
fn panel_provider_key_rewrite_forks_the_conversation() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm",
                "model_providers": {
                    "glm": {
                        "name": "GLM (智谱)",
                        "base_url": "https://open.bigmodel.cn/api/v1"
                    }
                }
            }
        }))),
    );
    seed_agent_message(&mut state, "live");
    state
        .settings
        .set_field(SettingField::ProviderApiKey, "sk-new".to_string());

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // Only the key changed, but a running thread pins its provider
    // snapshot: the fork re-applies the table with the history aboard.
    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert!(!state.transcript.entries().is_empty());
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; conversation moved to glm-5.3")
    );
}

#[test]
fn model_menu_toggles_open_and_closed() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::ModelMenuToggled);
    assert!(state.model_menu_open);

    let _ = update(&mut state, Message::ModelMenuClosed);
    assert!(!state.model_menu_open);
}

#[test]
fn picking_a_vendor_model_reopens_the_thread_on_the_new_setup() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("stale".to_string());
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm",
                "model_providers": {
                    "glm": {
                        "name": "GLM (智谱)",
                        "base_url": "https://open.bigmodel.cn/api/v1"
                    }
                }
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5-turbo".to_string(),
        },
    );
    assert!(state.model_switch_pending);
    assert!(!state.model_menu_open);

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // The pick only applies to a thread started afterwards.
    assert!(state.thread_id.is_none());
    assert_eq!(state.status, Status::Bootstrapping);
    assert!(!state.model_switch_pending);
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; fresh session started with the new model setup")
    );
}

#[test]
fn re_picking_the_live_vendor_model_leaves_the_thread_alone() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm"
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5.3".to_string(),
        },
    );

    assert!(!state.model_switch_pending);
    assert_eq!(state.thread_id.as_deref(), Some("live"));
}

#[test]
fn failed_vendor_switch_save_clears_the_pending_flag() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5-turbo".to_string(),
        },
    );
    assert!(state.model_switch_pending);

    let _ = update(
        &mut state,
        Message::SettingsSaved(Err(codex_gui_bridge::Error::Closed)),
    );

    assert!(!state.model_switch_pending);
    assert!(state.pending_model_apply.is_none());
    assert_eq!(state.thread_id.as_deref(), Some("live"));
}

#[test]
fn same_vendor_pick_flips_the_model_in_place() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm",
                "model_providers": {
                    "glm": {
                        "name": "GLM (智谱)",
                        "base_url": "https://open.bigmodel.cn/api/v1"
                    }
                }
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5.3-flashx".to_string(),
        },
    );
    assert_eq!(
        state.pending_model_apply,
        Some(PendingModelApply::InPlace {
            thread_id: "live".to_string(),
            model: "glm-5.3-flashx".to_string(),
        })
    );

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // The running thread keeps its conversation; the switch goes through
    // `thread/settings/update` instead of a restart.
    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert_eq!(state.status, Status::Ready);
    assert!(!state.model_switch_pending);
    assert!(state.pending_model_apply.is_none());
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Model switched to glm-5.3-flashx")
    );
}

#[test]
fn picking_another_vendor_without_history_reopens_the_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm"
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "deepseek".to_string(),
            slug: "deepseek-flash".to_string(),
        },
    );
    assert_eq!(
        state.pending_model_apply,
        Some(PendingModelApply::Retarget {
            provider_id: "deepseek".to_string(),
            model: "deepseek-flash".to_string(),
        })
    );

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // With nothing on screen to carry, the pick lands on a fresh one.
    assert!(state.thread_id.is_none());
    assert_eq!(state.status, Status::Bootstrapping);
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; fresh session started with the new model setup")
    );
}

#[test]
fn picking_another_vendor_carries_the_conversation_into_a_fork() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    seed_agent_message(&mut state, "live");
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "glm-5.3",
                "model_provider": "glm"
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "deepseek".to_string(),
            slug: "deepseek-flash".to_string(),
        },
    );

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // The run continues on the fork; the conversation stays on screen
    // until the fork reply lands (no empty-window gap).
    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert!(!state.transcript.entries().is_empty());
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; conversation moved to deepseek-flash")
    );
}

#[test]
fn a_settled_model_switch_fork_reopens_the_forked_thread() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    // The resume only takes effect with a client attached; its outbound
    // frames are never drained in the test.
    let (outbound, _frames) = tokio::sync::mpsc::channel(8);
    state.client = Some(codex_gui_bridge::Client::new(outbound));

    let _ = update(
        &mut state,
        Message::ModelSwitchForked(Ok("fork-1".to_string())),
    );

    // The resume is in flight; the fork is the thread on screen now.
    assert_eq!(state.thread_id.as_deref(), Some("fork-1"));
    assert_eq!(state.status, Status::Bootstrapping);
}

#[test]
fn a_failed_model_switch_fork_keeps_the_conversation() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    seed_agent_message(&mut state, "live");

    let _ = update(
        &mut state,
        Message::ModelSwitchForked(Err(codex_gui_bridge::Error::Closed)),
    );

    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert_eq!(state.status, Status::Ready);
    assert!(!state.transcript.entries().is_empty());
    let banner = state.status_board.error().expect("banner is up");
    assert!(banner.message.contains("model switch failed"));
}

#[test]
fn re_picking_the_bound_model_only_rewrites_the_config() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("live".to_string());
    state.status = Status::Ready;
    state.active_binding = Some(("glm".to_string(), "glm-5.3".to_string()));
    // The saved default drifted away from the live thread's pair.
    let _ = update(
        &mut state,
        Message::ConfigLoaded(Ok(json!({
            "config": {
                "model": "deepseek-flash",
                "model_provider": "deepseek"
            }
        }))),
    );

    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5.3".to_string(),
        },
    );
    assert!(state.model_switch_pending);
    assert!(state.pending_model_apply.is_none());

    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    // The thread already runs the pair; nothing is restarted or retargeted.
    assert_eq!(state.thread_id.as_deref(), Some("live"));
    assert_eq!(state.status, Status::Ready);
    assert!(!state.model_switch_pending);
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; new sessions will use the new model setup")
    );
}

#[test]
fn menu_pick_without_a_thread_only_records_the_default() {
    let mut state = State::new(Flags::default_app_server());
    // No live thread yet (e.g. the pick lands during bootstrap).
    let _ = update(
        &mut state,
        Message::VendorModelPicked {
            provider_id: "glm".to_string(),
            slug: "glm-5-turbo".to_string(),
        },
    );
    assert!(state.model_switch_pending);
    assert!(state.pending_model_apply.is_none());

    state.thread_id = Some("arrived".to_string());
    state.status = Status::Ready;
    let _ = update(&mut state, Message::SettingsSaved(Ok(json!({}))));

    assert_eq!(state.thread_id.as_deref(), Some("arrived"));
    assert_eq!(state.status, Status::Ready);
    assert_eq!(
        state.settings.notice.as_deref(),
        Some("Saved; new sessions will use the new model setup")
    );
}

#[test]
fn failed_in_place_switch_keeps_the_thread_and_raises_a_notice() {
    let mut state = State::new(Flags::default_app_server());
    state.settings.notice = None;

    let _ = update(
        &mut state,
        Message::ModelAppliedInPlace(Err(codex_gui_bridge::Error::Closed)),
    );

    assert!(
        state
            .settings
            .notice
            .as_deref()
            .is_some_and(|notice| notice.starts_with("model switch failed")),
        "the failure surfaces, got {:?}",
        state.settings.notice
    );
}

#[test]
fn thread_started_records_the_binding() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Bootstrap(Bootstrap::ThreadStarted(Ok(json!({
            "thread": {"id": "thread-1"},
            "model": "deepseek-flash",
            "modelProvider": "deepseek",
            "cwd": "/tmp"
        })))),
    );

    assert_eq!(state.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(
        state.active_binding,
        Some(("deepseek".to_string(), "deepseek-flash".to_string()))
    );
}

#[test]
fn session_resumed_records_the_binding_and_status_model() {
    let mut state = State::new(Flags::default_app_server());
    let history = SessionHistory {
        thread_id: "thread-2".to_string(),
        model_provider: "glm".to_string(),
        model: "glm-5.3".to_string(),
        cwd: "/tmp".to_string(),
        turns: Vec::new(),
    };

    let _ = update(&mut state, Message::SessionResumed(Ok(history)));

    assert_eq!(
        state.active_binding,
        Some(("glm".to_string(), "glm-5.3".to_string()))
    );
    assert_eq!(state.status_board.current_model(), Some("glm-5.3"));
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
        plugin_id: None,
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

/// One inventory row as a local, plugin, or system skill would list.
fn skill_row(name: &str, scope: SkillScope, plugin_id: Option<&str>) -> SkillRow {
    SkillRow {
        name: name.to_string(),
        description: format!("{name} description"),
        scope,
        path: PathBuf::from(format!("/tmp/skills/{name}/SKILL.md")),
        enabled: true,
        plugin_id: plugin_id.map(str::to_string),
    }
}

/// The one-plugin marketplace payload the market tests browse.
fn plugin_list_response(installed: bool) -> PluginListResponse {
    from_value(json!({
        "marketplaces": [{
            "name": "local-market",
            "path": "/tmp/marketplace.json",
            "interface": {"displayName": "本地市场"},
            "plugins": [plugin_summary_json(installed)]
        }],
        "marketplaceLoadErrors": [],
        "featuredPluginIds": []
    }))
    .expect("plugin list payload decodes")
}

/// The expanded plugin detail the market row reveals.
fn plugin_read_response() -> PluginReadResponse {
    from_value(json!({
        "plugin": {
            "marketplaceName": "local-market",
            "marketplacePath": "/tmp/marketplace.json",
            "summary": plugin_summary_json(false),
            "shareUrl": null,
            "description": "PDF utility skills",
            "skills": [
                {
                    "name": "pdf-split",
                    "description": "Split a PDF into pages",
                    "shortDescription": null,
                    "interface": null,
                    "path": null,
                    "enabled": true
                },
                {
                    "name": "pdf-merge",
                    "description": "Merge several PDFs into one",
                    "shortDescription": null,
                    "interface": null,
                    "path": null,
                    "enabled": true
                }
            ],
            "onboardingSkill": null,
            "hooks": [],
            "apps": [],
            "appTemplates": [],
            "mcpServers": [],
            "scheduledTasks": null
        }
    }))
    .expect("plugin read payload decodes")
}

/// A full `PluginSummary` payload; every non-optional key is present.
fn plugin_summary_json(installed: bool) -> serde_json::Value {
    json!({
        "id": "pdf-tools@local-market",
        "remotePluginId": null,
        "name": "pdf-tools",
        "shareContext": null,
        "source": {"type": "local", "path": "/tmp/plugins/pdf-tools"},
        "installed": installed,
        "enabled": installed,
        "installPolicy": "AVAILABLE",
        "installPolicySource": null,
        "authPolicy": "ON_USE",
        "interface": {
            "displayName": "PDF 工具",
            "shortDescription": "Split and merge PDF files",
            "longDescription": null,
            "developerName": null,
            "category": null,
            "capabilities": [],
            "websiteUrl": null,
            "privacyPolicyUrl": null,
            "termsOfServiceUrl": null,
            "defaultPrompt": null,
            "brandColor": null,
            "composerIcon": null,
            "composerIconUrl": null,
            "logo": null,
            "logoDark": null,
            "logoUrl": null,
            "logoUrlDark": null,
            "screenshots": [],
            "screenshotUrls": []
        },
        "keywords": []
    })
}

#[test]
fn skills_page_toggles_and_yields_to_settings() {
    let mut state = State::new(Flags::default_app_server());
    assert!(!state.skills.page_open);

    // Opening the page takes the surface away from settings.
    state.settings.open = true;
    let _ = update(&mut state, Message::SkillsPageToggled);
    assert!(state.skills.page_open);
    assert!(!state.settings.open, "the settings panel yields");

    // Closing drops the transient forms and the notice.
    state.skills.add = Some(SkillAdd::default());
    state.skills.confirm_delete = Some("local-skill".to_string());
    state.skills.notice = Some(SkillNotice::ok("hi"));
    let _ = update(&mut state, Message::SkillsPageToggled);
    assert!(!state.skills.page_open);
    assert!(state.skills.add.is_none());
    assert!(state.skills.confirm_delete.is_none());
    assert!(state.skills.notice.is_none());

    // The settings gear closes the page in return.
    state.skills.page_open = true;
    let _ = update(&mut state, Message::SettingsToggled);
    assert!(state.settings.open);
    assert!(!state.skills.page_open, "the Skills page yields");
}

#[test]
fn skills_tab_pick_switches_the_tab() {
    let mut state = State::new(Flags::default_app_server());
    assert_eq!(state.skills.tab, SkillsTab::Manage);

    let _ = update(&mut state, Message::SkillsTabPicked(SkillsTab::Market));
    assert_eq!(state.skills.tab, SkillsTab::Market);

    let _ = update(&mut state, Message::SkillsTabPicked(SkillsTab::Manage));
    assert_eq!(state.skills.tab, SkillsTab::Manage);
}

#[test]
fn skill_add_form_drafts_import_source_and_cancels() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::SkillAddOpened);
    assert!(state.skills.add.is_some(), "the add form opens");

    let _ = update(&mut state, Message::SkillAddNameChanged("demo".to_string()));
    let _ = update(
        &mut state,
        Message::SkillAddDescriptionChanged("a demo skill".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillAddLinkChanged("https://github.com/obra/superpowers".to_string()),
    );
    let add = state.skills.add.as_ref().expect("add form is open");
    assert_eq!(add.name, "demo");
    assert_eq!(add.description, "a demo skill");
    assert_eq!(add.link, "https://github.com/obra/superpowers");
    assert!(add.source.is_none());

    // The native picker's selection becomes the import source; cancellation
    // clears it again.
    let picked = PathBuf::from("/tmp/imported-skill");
    let _ = update(
        &mut state,
        Message::SkillAddSourcePicked(Some(picked.clone())),
    );
    assert_eq!(
        state.skills.add.as_ref().expect("form").source,
        Some(picked)
    );
    let _ = update(&mut state, Message::SkillAddSourcePicked(None));
    assert!(state.skills.add.as_ref().expect("form").source.is_none());

    let _ = update(&mut state, Message::SkillAddCancelled);
    assert!(state.skills.add.is_none());
}

#[test]
fn skill_add_import_request_targets_the_picker() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.open_add();

    // The import buttons only ask for the picker; no state changes yet.
    let _ = update(
        &mut state,
        Message::SkillAddImportRequested(SkillImportTarget::Directory),
    );
    let _ = update(
        &mut state,
        Message::SkillAddImportRequested(SkillImportTarget::File),
    );
    assert!(state.skills.add.is_some());
}

#[test]
fn skill_add_link_submit_marks_the_form_busy_until_settled() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.add = Some(SkillAdd {
        link: "https://github.com/obra/superpowers".to_string(),
        ..SkillAdd::default()
    });

    let _ = update(&mut state, Message::SkillAddSubmitted);
    assert!(
        state.skills.add.as_ref().expect("form").busy,
        "the running clone locks the form"
    );

    let _ = update(
        &mut state,
        Message::SkillMutationSettled(Err("clone failed".to_string())),
    );
    assert!(
        !state.skills.add.as_ref().expect("form").busy,
        "failure unlocks the form"
    );
}

#[test]
fn skill_mutation_failure_keeps_the_drafts_and_reports() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.add = Some(SkillAdd {
        name: "demo".to_string(),
        ..SkillAdd::default()
    });
    state.skills.editor = Some(SkillEdit {
        name: "mock-skill".to_string(),
        path: PathBuf::from("/tmp/skills/mock-skill/SKILL.md"),
        name_draft: "renamed".to_string(),
        description_draft: "draft".to_string(),
    });

    let _ = update(
        &mut state,
        Message::SkillMutationSettled(Err("disk full".to_string())),
    );

    assert!(state.skills.add.is_some(), "the add draft survives");
    assert!(state.skills.editor.is_some(), "the edit draft survives");
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert_eq!(notice.text, "操作失败：disk full");
}

#[test]
fn skill_mutation_success_closes_the_forms_and_reports() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.add = Some(SkillAdd {
        name: "demo".to_string(),
        ..SkillAdd::default()
    });
    state.skills.editor = Some(SkillEdit {
        name: "mock-skill".to_string(),
        path: PathBuf::from("/tmp/skills/mock-skill/SKILL.md"),
        name_draft: "renamed".to_string(),
        description_draft: "draft".to_string(),
    });

    let _ = update(
        &mut state,
        Message::SkillMutationSettled(Ok("已创建 skill `demo`".to_string())),
    );

    assert!(state.skills.add.is_none(), "success closes the add form");
    assert!(
        state.skills.editor.is_none(),
        "success closes the edit form"
    );
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(!notice.error);
    assert_eq!(notice.text, "已创建 skill `demo`");
}

#[test]
fn skill_edit_form_seeds_from_the_row_and_cancels() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.rows = vec![skill_row("mock-skill", SkillScope::User, None)];

    let _ = update(
        &mut state,
        Message::SkillEditOpened("mock-skill".to_string()),
    );
    let editor = state.skills.editor.as_ref().expect("the edit form opens");
    assert_eq!(editor.name_draft, "mock-skill");
    assert_eq!(editor.description_draft, "mock-skill description");
    assert_eq!(
        editor.path,
        PathBuf::from("/tmp/skills/mock-skill/SKILL.md")
    );

    let _ = update(
        &mut state,
        Message::SkillEditNameChanged("renamed".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillEditDescriptionChanged("new description".to_string()),
    );
    let editor = state.skills.editor.as_ref().expect("still open");
    assert_eq!(editor.name_draft, "renamed");
    assert_eq!(editor.description_draft, "new description");

    let _ = update(&mut state, Message::SkillEditCancelled);
    assert!(state.skills.editor.is_none());
}

#[test]
fn skill_delete_confirmation_arms_cancels_and_spares_system_skills() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.rows = vec![
        skill_row("local-skill", SkillScope::User, None),
        skill_row(
            "plugin-skill",
            SkillScope::User,
            Some("pdf-tools@local-market"),
        ),
        skill_row("sys-skill", SkillScope::System, None),
    ];

    // Arming asks for confirmation; cancelling backs out.
    let _ = update(
        &mut state,
        Message::SkillDeleteArmed("local-skill".to_string()),
    );
    assert_eq!(state.skills.confirm_delete.as_deref(), Some("local-skill"));
    let _ = update(&mut state, Message::SkillDeleteCancelled);
    assert!(state.skills.confirm_delete.is_none());

    // System skills are protected even after a confirmation.
    let _ = update(
        &mut state,
        Message::SkillDeleteArmed("sys-skill".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillDeleteConfirmed("sys-skill".to_string()),
    );
    assert!(state.skills.confirm_delete.is_none());
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert_eq!(notice.text, "系统级 skill 受保护，无法删除");

    // Confirming a local skill clears the gate; the removal is async.
    let _ = update(
        &mut state,
        Message::SkillDeleteArmed("local-skill".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillDeleteConfirmed("local-skill".to_string()),
    );
    assert!(state.skills.confirm_delete.is_none());

    // A plugin-shipped skill leaves by uninstalling its plugin.
    let _ = update(
        &mut state,
        Message::SkillDeleteArmed("plugin-skill".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillDeleteConfirmed("plugin-skill".to_string()),
    );
    assert!(state.skills.confirm_delete.is_none());
}

#[test]
fn market_load_lands_the_catalog_or_keeps_the_tab_usable() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::SkillMarketLoaded(Ok(plugin_list_response(false))),
    );
    assert!(state.skills.market.loaded);
    let plugin = &state.skills.market.plugins[0];
    assert_eq!(plugin.id, "pdf-tools@local-market");
    assert_eq!(plugin.name, "pdf-tools");
    assert_eq!(plugin.marketplace, "local-market");
    assert!(!plugin.installed);

    // A failure still unlocks the tab so the refresh action retries.
    let mut state = State::new(Flags::default_app_server());
    let _ = update(
        &mut state,
        Message::SkillMarketLoaded(Err(codex_gui_bridge::Error::Closed)),
    );
    assert!(state.skills.market.loaded);
    assert!(state.skills.market.plugins.is_empty());
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert!(notice.text.starts_with("市场加载失败："));
}

#[test]
fn market_plugin_expand_fetches_shows_and_collapses() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.market.apply_list(&plugin_list_response(false));

    // Expanding asks for the detail.
    let _ = update(
        &mut state,
        Message::SkillMarketPluginToggled("pdf-tools@local-market".to_string()),
    );
    assert_eq!(
        state.skills.market.detail_loading.as_deref(),
        Some("pdf-tools@local-market")
    );

    // A second press while loading cancels the fetch.
    let _ = update(
        &mut state,
        Message::SkillMarketPluginToggled("pdf-tools@local-market".to_string()),
    );
    assert!(state.skills.market.detail_loading.is_none());

    // The settled detail lands on the expanded row.
    let _ = update(
        &mut state,
        Message::SkillMarketPluginToggled("pdf-tools@local-market".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillMarketDetailLoaded(Ok(plugin_read_response())),
    );
    let detail = state.skills.market.detail.as_ref().expect("detail is up");
    assert_eq!(detail.plugin_id, "pdf-tools@local-market");
    assert_eq!(detail.skills.len(), 2);
    assert_eq!(detail.skills[0].name, "pdf-split");

    // Collapsing clears it again.
    let _ = update(
        &mut state,
        Message::SkillMarketPluginToggled("pdf-tools@local-market".to_string()),
    );
    assert!(state.skills.market.detail.is_none());

    // A failed detail fetch unlocks the row and reports.
    let _ = update(
        &mut state,
        Message::SkillMarketPluginToggled("pdf-tools@local-market".to_string()),
    );
    let _ = update(
        &mut state,
        Message::SkillMarketDetailLoaded(Err(codex_gui_bridge::Error::Closed)),
    );
    assert!(state.skills.market.detail_loading.is_none());
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert!(notice.text.starts_with("插件详情加载失败："));
}

#[test]
fn market_install_reports_both_outcomes() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.market.apply_list(&plugin_list_response(false));

    let _ = update(
        &mut state,
        Message::SkillMarketInstallRequested("pdf-tools@local-market".to_string()),
    );
    assert_eq!(
        state.skills.market.busy_plugin.as_deref(),
        Some("pdf-tools@local-market")
    );

    let _ = update(
        &mut state,
        Message::SkillInstalled {
            id: "pdf-tools@local-market".to_string(),
            result: Ok(json!({"authPolicy": "ON_USE", "appsNeedingAuth": []})),
        },
    );
    assert!(state.skills.market.busy_plugin.is_none());
    assert!(state.skills.market.plugins[0].installed);
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(!notice.error);
    assert_eq!(
        notice.text,
        "已安装 `pdf-tools@local-market`；首次使用时会请求授权"
    );

    // A failed install reports without mirroring onto the row.
    let mut state = State::new(Flags::default_app_server());
    state.skills.market.apply_list(&plugin_list_response(false));
    let _ = update(
        &mut state,
        Message::SkillInstalled {
            id: "pdf-tools@local-market".to_string(),
            result: Err(codex_gui_bridge::Error::Closed),
        },
    );
    assert!(state.skills.market.busy_plugin.is_none());
    assert!(!state.skills.market.plugins[0].installed);
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert!(notice.text.starts_with("安装失败："));
}

#[test]
fn market_uninstall_drops_the_plugin_skills() {
    let mut state = State::new(Flags::default_app_server());
    state.skills.market.apply_list(&plugin_list_response(true));
    state.skills.rows = vec![skill_row(
        "pdf-split",
        SkillScope::User,
        Some("pdf-tools@local-market"),
    )];

    let _ = update(
        &mut state,
        Message::SkillUninstallRequested("pdf-tools@local-market".to_string()),
    );
    assert_eq!(
        state.skills.market.busy_plugin.as_deref(),
        Some("pdf-tools@local-market")
    );

    let _ = update(
        &mut state,
        Message::SkillUninstalled {
            id: "pdf-tools@local-market".to_string(),
            result: Ok(json!({})),
        },
    );
    assert!(state.skills.market.busy_plugin.is_none());
    assert!(!state.skills.market.plugins[0].installed);
    assert!(
        state.skills.rows.is_empty(),
        "the plugin's skills leave the inventory"
    );
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(!notice.error);
    assert_eq!(
        notice.text,
        "已卸载 `pdf-tools@local-market`（移除 1 个 skill）"
    );

    // A failed uninstall reports without dropping rows.
    let mut state = State::new(Flags::default_app_server());
    state.skills.market.apply_list(&plugin_list_response(true));
    state.skills.rows = vec![skill_row(
        "pdf-split",
        SkillScope::User,
        Some("pdf-tools@local-market"),
    )];
    let _ = update(
        &mut state,
        Message::SkillUninstalled {
            id: "pdf-tools@local-market".to_string(),
            result: Err(codex_gui_bridge::Error::Closed),
        },
    );
    assert!(state.skills.market.plugins[0].installed);
    assert_eq!(state.skills.rows.len(), 1);
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert!(notice.text.starts_with("卸载失败："));
}

#[test]
fn market_source_draft_submits_and_reports() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::SkillMarketSourceChanged("owner/repo".to_string()),
    );
    assert_eq!(state.skills.market.source_draft, "owner/repo");

    // An empty draft is rejected before hitting the wire.
    state.skills.market.source_draft = "   ".to_string();
    let _ = update(&mut state, Message::SkillMarketSourceSubmitted);
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert_eq!(notice.text, "请先填写市场来源");

    // A settled add clears the draft and announces the discovered name.
    state.skills.market.source_draft = "owner/repo".to_string();
    let _ = update(
        &mut state,
        Message::SkillMarketSourceAdded(Ok(json!({
            "marketplaceName": "local-market",
            "installedRoot": "/tmp/marketplaces/local-market",
            "alreadyAdded": false
        }))),
    );
    assert!(state.skills.market.source_draft.is_empty());
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(!notice.error);
    assert_eq!(notice.text, "已添加市场 `local-market`");

    // An already-present marketplace says so instead.
    let _ = update(
        &mut state,
        Message::SkillMarketSourceAdded(Ok(json!({
            "marketplaceName": "local-market",
            "installedRoot": "/tmp/marketplaces/local-market",
            "alreadyAdded": true
        }))),
    );
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert_eq!(notice.text, "市场 `local-market` 已存在，正在刷新列表");

    let _ = update(
        &mut state,
        Message::SkillMarketSourceAdded(Err(codex_gui_bridge::Error::Closed)),
    );
    let notice = state.skills.notice.as_ref().expect("a notice is up");
    assert!(notice.error);
    assert!(notice.text.starts_with("添加市场失败："));
}

#[test]
fn bootstrap_extracts_the_codex_home_for_skill_mutations() {
    let mut state = State::new(Flags::default_app_server());
    assert!(state.codex_home.is_none());

    let _ = update(
        &mut state,
        Message::Bootstrap(Bootstrap::Initialized(Ok(json!({
            "userAgent": "mock",
            "codexHome": "/tmp/codex-home",
            "platformFamily": "unix",
            "platformOs": "linux"
        })))),
    );

    assert_eq!(state.codex_home, Some(PathBuf::from("/tmp/codex-home")));
}

#[test]
fn turn_started_tracks_the_live_turn_for_the_stop_button() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("thread-1".to_string());
    state.status = Status::Thinking;

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/started",
            "params": {"threadId": "thread-1", "turn": turn_json("turn-1", "inProgress")}
        }))),
    );
    assert_eq!(state.active_turn.as_deref(), Some("turn-1"));

    // A foreign thread's turn must not hijack the stop button.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/started",
            "params": {"threadId": "thread-2", "turn": turn_json("turn-9", "inProgress")}
        }))),
    );
    assert_eq!(state.active_turn.as_deref(), Some("turn-1"));

    // A still-running completion notice leaves the turn live.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/completed",
            "params": {"threadId": "thread-1", "turn": turn_json("turn-1", "inProgress")}
        }))),
    );
    assert_eq!(state.active_turn.as_deref(), Some("turn-1"));
    assert_eq!(state.status, Status::Thinking);

    // A terminal completion clears it and reopens the composer.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "turn/completed",
            "params": {"threadId": "thread-1", "turn": turn_json("turn-1", "interrupted")}
        }))),
    );
    assert!(state.active_turn.is_none());
    assert_eq!(state.status, Status::Ready);
}

#[test]
fn turn_interrupt_failure_raises_the_banner() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::TurnInterrupted(Err(codex_gui_bridge::Error::Request {
            code: -32600,
            message: "no active turn".to_string(),
        })),
    );

    let banner = state.status_board.error().expect("banner is up");
    assert!(banner.message.contains("turn interrupt failed"));
}

#[test]
fn review_and_compaction_starts_drive_status_and_banners() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::ReviewStarted(Ok(json!({}))));
    assert_eq!(state.status, Status::Thinking);

    let _ = update(
        &mut state,
        Message::ReviewStarted(Err(codex_gui_bridge::Error::Closed)),
    );
    let banner = state.status_board.error().expect("banner is up");
    assert!(banner.message.contains("review failed to start"));

    let _ = update(&mut state, Message::CompactionStarted(Ok(json!({}))));
    assert_eq!(state.status, Status::Thinking);

    let _ = update(
        &mut state,
        Message::CompactionStarted(Err(codex_gui_bridge::Error::Closed)),
    );
    let banner = state.status_board.error().expect("banner is up");
    assert!(banner.message.contains("compaction failed to start"));
}

#[test]
fn mention_token_opens_the_popup_and_tracks_the_query() {
    let mut state = State::new(Flags::default_app_server());
    state.status_board.apply_cwd("/tmp/proj".to_string());

    let _ = update(&mut state, Message::ComposerChanged("@lib".to_string()));
    assert!(state.mentions.is_open());
    assert_eq!(state.mentions.query(), "lib");

    // Continuing the word re-searches under the same token.
    let _ = update(&mut state, Message::ComposerChanged("@libx".to_string()));
    assert_eq!(state.mentions.query(), "libx");

    // A bare `@` after a word still starts a token.
    let _ = update(
        &mut state,
        Message::ComposerChanged("look at @src".to_string()),
    );
    assert_eq!(state.mentions.query(), "src");
    assert_eq!(state.composer, "look at @src");

    // Committing the token with a space closes the popup.
    let _ = update(
        &mut state,
        Message::ComposerChanged("look at @src ".to_string()),
    );
    assert!(!state.mentions.is_open());

    // Esc closes the popup without clearing the composer.
    let _ = update(
        &mut state,
        Message::ComposerChanged("see @main".to_string()),
    );
    assert!(state.mentions.is_open());
    let _ = update(&mut state, Message::MentionClosed);
    assert!(!state.mentions.is_open());
    assert_eq!(state.composer, "see @main");
}

#[test]
fn mention_page_lands_only_for_the_live_query() {
    let mut state = State::new(Flags::default_app_server());
    state.status_board.apply_cwd("/tmp/proj".to_string());
    let _ = update(&mut state, Message::ComposerChanged("@lib".to_string()));

    let _ = update(
        &mut state,
        Message::MentionSearchLoaded {
            query: "lib".to_string(),
            result: Ok(fuzzy_response(&[
                ("/tmp/proj/src/lib.rs", "lib.rs"),
                ("/tmp/proj/src/lib/parser.rs", "parser.rs"),
            ])),
        },
    );
    assert_eq!(state.mentions.hits().len(), 2);
    assert_eq!(state.mentions.hits()[0].file_name, "lib.rs");

    // A stale page that resolves after the token moved on is dropped.
    let _ = update(
        &mut state,
        Message::MentionSearchLoaded {
            query: "lib-old".to_string(),
            result: Ok(fuzzy_response(&[("/tmp/proj/src/old.rs", "old.rs")])),
        },
    );
    assert_eq!(state.mentions.hits().len(), 2);

    // A failed search keeps the popup and the last good page.
    let _ = update(
        &mut state,
        Message::MentionSearchLoaded {
            query: "lib".to_string(),
            result: Err(codex_gui_bridge::Error::Closed),
        },
    );
    assert!(state.mentions.is_open());
    assert_eq!(state.mentions.hits().len(), 2);
}

#[test]
fn mention_moves_wrap_and_a_pick_rewrites_the_token() {
    let mut state = State::new(Flags::default_app_server());
    state.status_board.apply_cwd("/tmp/proj".to_string());
    let _ = update(&mut state, Message::ComposerChanged("@lib".to_string()));
    let _ = update(
        &mut state,
        Message::MentionSearchLoaded {
            query: "lib".to_string(),
            result: Ok(fuzzy_response(&[
                ("/tmp/proj/a.rs", "a.rs"),
                ("/tmp/proj/b.rs", "b.rs"),
                ("/tmp/proj/c.rs", "c.rs"),
            ])),
        },
    );

    let _ = update(&mut state, Message::MentionMoved(1));
    assert_eq!(state.mentions.selected(), 1);
    let _ = update(&mut state, Message::MentionMoved(-1));
    assert_eq!(state.mentions.selected(), 0);
    // The highlight wraps at the top edge.
    let _ = update(&mut state, Message::MentionMoved(-1));
    assert_eq!(state.mentions.selected(), 2);

    let _ = update(&mut state, Message::MentionPicked(2));
    assert_eq!(state.composer, "@/tmp/proj/c.rs ");
    assert!(!state.mentions.is_open());
}

#[test]
fn submit_inserts_the_picked_mention_instead_of_the_turn() {
    let mut state = State::new(Flags::default_app_server());
    state.status_board.apply_cwd("/tmp/proj".to_string());
    let _ = update(
        &mut state,
        Message::ComposerChanged("look at @lib".to_string()),
    );
    let _ = update(
        &mut state,
        Message::MentionSearchLoaded {
            query: "lib".to_string(),
            result: Ok(fuzzy_response(&[("/tmp/proj/src/lib.rs", "lib.rs")])),
        },
    );

    let _ = update(&mut state, Message::Submit);

    // The popup is open, so Enter completes the token rather than
    // submitting a turn; the server-side mention resolution reads the path.
    assert_eq!(state.composer, "look at @/tmp/proj/src/lib.rs ");
    assert!(!state.mentions.is_open());
}

#[test]
fn slash_submit_is_gated_on_a_live_session() {
    let mut state = State::new(Flags::default_app_server());
    state.thread_id = Some("thread-1".to_string());
    state.status = Status::Ready;
    state.composer = "/review".to_string();

    // Without a client the live-session gate rejects the command and the
    // typed text stays in the composer for a retry.
    let _ = update(&mut state, Message::Submit);
    assert_eq!(state.composer, "/review");
    assert_eq!(state.status, Status::Ready);

    // A picked slash row goes through the same gate.
    let _ = update(&mut state, Message::SlashPicked("/compact".to_string()));
    assert_eq!(state.composer, "/compact");
}

#[test]
fn mcp_inventory_lands_in_the_settings_panel() {
    let mut state = State::new(Flags::default_app_server());
    assert!(!state.mcp_loaded);

    let _ = update(
        &mut state,
        Message::McpServersLoaded(Ok(ListMcpServerStatusResponse {
            data: vec![mcp_server_status(
                "filesystem",
                McpServerConnectionStatus::Connected,
            )],
            next_cursor: None,
        })),
    );
    assert!(state.mcp_loaded);
    assert_eq!(state.mcp_servers.len(), 1);
    assert_eq!(state.mcp_servers[0].name, "filesystem");

    // A failed refresh settles the loading state and keeps the last page.
    let _ = update(
        &mut state,
        Message::McpServersLoaded(Err(codex_gui_bridge::Error::Closed)),
    );
    assert!(state.mcp_loaded);
    assert_eq!(state.mcp_servers.len(), 1);
}

#[test]
fn card_toggles_track_their_expansion_sets() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::ReasoningToggled("reason-1".to_string()),
    );
    assert!(state.collapsed_reasoning.contains("reason-1"));
    let _ = update(
        &mut state,
        Message::ReasoningToggled("reason-1".to_string()),
    );
    assert!(state.collapsed_reasoning.is_empty());

    let _ = update(&mut state, Message::McpCardToggled("call-1".to_string()));
    assert!(state.expanded_mcp.contains("call-1"));
    let _ = update(&mut state, Message::McpCardToggled("call-1".to_string()));
    assert!(state.expanded_mcp.is_empty());
}

/// A `Turn` payload for `turn/started` / `turn/completed` notifications.
fn turn_json(id: &str, status: &str) -> serde_json::Value {
    json!({
        "id": id,
        "items": [],
        "status": status,
        "error": null,
        "startedAt": null,
        "completedAt": null,
        "durationMs": null
    })
}

/// A `fuzzyFileSearch` reply carrying the given `(path, file_name)` pairs.
fn fuzzy_response(files: &[(&str, &str)]) -> FuzzyFileSearchResponse {
    FuzzyFileSearchResponse {
        files: files
            .iter()
            .map(|(path, file_name)| FuzzyFileSearchResult {
                root: "/tmp/proj".to_string(),
                path: (*path).to_string(),
                match_type: FuzzyFileSearchMatchType::File,
                file_name: (*file_name).to_string(),
                score: 1,
                indices: None,
            })
            .collect(),
    }
}

/// One MCP inventory row as `mcpServerStatus/list` would report it.
fn mcp_server_status(name: &str, status: McpServerConnectionStatus) -> McpServerStatus {
    McpServerStatus {
        name: name.to_string(),
        runtime_status: Some(status),
        plugin_id: None,
        http_origin: None,
        server_info: None,
        server_capabilities: None,
        tools: HashMap::new(),
        tools_error: None,
        resources: Vec::new(),
        resource_templates: Vec::new(),
        auth_status: McpAuthStatus::OAuth,
    }
}

/// A typed server request frame, as the bridge hands it to the UI.
fn server_request(payload: serde_json::Value) -> GuiEvent {
    let message: JSONRPCMessage = from_value(payload).expect("request frame decodes");
    match message {
        JSONRPCMessage::Request(request) => GuiEvent::ServerRequest(
            ServerRequest::try_from(request).expect("server request decodes"),
        ),
        other => panic!("expected a request frame, got {other:?}"),
    }
}

#[test]
fn questions_open_a_dialog_and_submit_drains_the_queue() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(server_request(json!({
            "method": "item/tool/requestUserInput",
            "id": "srv-1",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "questions": [{
                    "id": "q1",
                    "header": "Deploy",
                    "question": "Which region?",
                    "isOther": true,
                    "options": [{"label": "us", "description": "US East"}]
                }],
                "isBlocking": true
            }
        }))),
    );
    assert_eq!(state.questions.pending().len(), 1);

    // The picked row and the notes land in the draft.
    let _ = update(
        &mut state,
        Message::QuestionOptionPicked {
            question_id: "q1".to_string(),
            index: 0,
        },
    );
    let _ = update(
        &mut state,
        Message::QuestionNotesChanged {
            question_id: "q1".to_string(),
            notes: "pin to us-east".to_string(),
        },
    );
    assert_eq!(state.question_drafts["q1"].selected, Some(0));
    assert_eq!(state.question_drafts["q1"].notes, "pin to us-east");

    // Submitting answers the head and clears its drafts.
    let _ = update(&mut state, Message::QuestionAnswered);
    assert!(state.questions.pending().is_empty());
    assert!(state.question_drafts.is_empty());
}

#[test]
fn elicitation_form_seeds_defaults_and_declines() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(server_request(json!({
            "method": "mcpServer/elicitation/request",
            "id": "srv-2",
            "params": {
                "threadId": "thread-1",
                "serverName": "deployments",
                "mode": "form",
                "message": "Deploy config",
                "requestedSchema": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "title": "Name", "default": "prod"}
                    },
                    "required": ["name"]
                }
            }
        }))),
    );
    assert_eq!(state.elicitations.pending().len(), 1);
    assert_eq!(state.elicitation_drafts["name"].text, "prod");

    let _ = update(
        &mut state,
        Message::ElicitationFieldTextChanged {
            key: "name".to_string(),
            text: "staging".to_string(),
        },
    );
    assert_eq!(state.elicitation_drafts["name"].text, "staging");

    // Declining answers the head and drops the draft set.
    let _ = update(
        &mut state,
        Message::ElicitationAnswered(McpServerElicitationAction::Decline),
    );
    assert!(state.elicitations.pending().is_empty());
    assert!(state.elicitation_drafts.is_empty());
}

#[test]
fn raw_openai_form_elicitation_never_opens_a_dialog() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::Event(server_request(json!({
            "method": "mcpServer/elicitation/request",
            "id": "srv-3",
            "params": {
                "threadId": "thread-1",
                "serverName": "codex_apps",
                "mode": "openai/form",
                "message": "Allow this request?",
                "requestedSchema": {"type": "object"}
            }
        }))),
    );

    // The raw mode is answered with a decline instead of a dialog.
    assert!(state.elicitations.pending().is_empty());
}

#[test]
fn archived_partition_loads_and_reacts_to_notifications() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(&mut state, Message::ArchivedToggled);
    assert!(state.archived_open);

    let response: ThreadListResponse = from_value(json!({
        "data": [thread_json("archived-1", "old quest", "/tmp/proj")],
        "nextCursor": null,
        "backwardsCursor": null
    }))
    .expect("list response decodes");
    let _ = update(&mut state, Message::ArchivedSessionsLoaded(Ok(response)));
    assert!(state.archived_loaded);
    assert_eq!(state.archived.threads().len(), 1);
    assert_eq!(state.archived.threads()[0].id, "archived-1");

    // Archiving drops the row from the live partition; unarchiving drops
    // it from the archived one.
    seed_sessions(&mut state, vec![thread_json("t-1", "live", "/tmp/proj")]);
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/archived",
            "params": {"threadId": "t-1"}
        }))),
    );
    assert!(state.sessions.threads().is_empty());

    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "thread/unarchived",
            "params": {"threadId": "archived-1"}
        }))),
    );
    assert!(state.archived.threads().is_empty());
}

#[test]
fn mcp_login_banner_tracks_the_completion_notification() {
    let mut state = State::new(Flags::default_app_server());

    let _ = update(
        &mut state,
        Message::McpLoginStarted {
            name: "filesystem".to_string(),
            result: Ok(McpServerOauthLoginResponse {
                authorization_url: "https://mcp.example/auth".to_string(),
            }),
        },
    );
    let login = state.mcp_login.as_ref().expect("banner is up");
    assert_eq!(login.url, "https://mcp.example/auth");
    assert!(state.settings.notice.is_some());

    // The completion for the matching server clears the banner.
    let _ = update(
        &mut state,
        Message::Event(notification(json!({
            "method": "mcpServer/oauthLogin/completed",
            "params": {
                "name": "filesystem",
                "threadId": null,
                "success": true,
                "error": null
            }
        }))),
    );
    assert!(state.mcp_login.is_none());
    assert_eq!(
        state.settings.notice,
        Some("filesystem signed in".to_string())
    );

    // A dismissal clears the banner without any notification.
    let _ = update(
        &mut state,
        Message::McpLoginStarted {
            name: "other".to_string(),
            result: Ok(McpServerOauthLoginResponse {
                authorization_url: "https://mcp.example/other".to_string(),
            }),
        },
    );
    let _ = update(&mut state, Message::McpLoginDismissed);
    assert!(state.mcp_login.is_none());
}
