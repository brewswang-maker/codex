//! Headless view-interaction tests through the `iced_test` simulator
//! (spec section 10, UI layer row).
#![allow(clippy::expect_used, clippy::panic)]

use crate::message::AppMode;
use crate::message::MenuId;
use crate::message::Message;
use crate::message::QuestScenario;
use crate::state::FileBody;
use crate::state::State;
use crate::state::Status;
use crate::view;
use codex_app_server_protocol::ThreadListResponse;
use codex_gui_bridge::Flags;
use iced_test::simulator;
use serde_json::json;
use std::sync::Arc;

#[test]
fn disconnected_screen_offers_the_one_click_relaunch() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Disconnected("exit code 1".to_string());

    let mut ui = simulator(view(&state));
    assert!(ui.find("Connection lost").is_ok(), "dead-end copy is shown");

    ui.click("Restart app-server")
        .expect("restart button is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::Reconnect)),
        "clicking restart emits Reconnect, got {messages:?}"
    );
}

#[test]
fn error_banner_dismiss_button_emits_error_dismissed() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state
        .status_board
        .raise_banner("rate limit approaching".to_string());

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("rate limit approaching").is_ok(),
        "banner text shows"
    );

    ui.click("dismiss").expect("dismiss button is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ErrorDismissed)),
        "clicking dismiss emits ErrorDismissed, got {messages:?}"
    );
}

#[test]
fn idle_ready_view_shows_the_composer_prompt() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;

    let mut ui = simulator(view(&state));

    assert!(ui.find("Ask Codex…").is_ok(), "composer placeholder shows");
}

#[test]
fn welcome_headline_still_shows_before_any_project_opens() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;

    let mut ui = simulator(view(&state));

    assert!(
        ui.find("What can I help you ship?").is_ok(),
        "the welcome headline shows while no project is open"
    );
}

#[test]
fn open_project_replaces_the_welcome_with_a_bare_chat_pane() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state
        .status_board
        .apply_cwd("/tmp/simulator-project".to_string());

    let mut ui = simulator(view(&state));

    assert!(
        ui.find("What can I help you ship?").is_err(),
        "the welcome headline is gone once a project is open"
    );
    assert!(ui.find("Recent").is_err(), "the recents list is gone");
    assert!(
        ui.find("Ask Codex…").is_ok(),
        "the bare conversation keeps the composer"
    );
}

#[test]
fn editor_menu_bar_offers_all_eight_menus() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;

    let mut ui = simulator(view(&state));
    for title in [
        "文件(F)",
        "编辑(E)",
        "选择(S)",
        "查看(V)",
        "转到(G)",
        "运行(R)",
        "终端(T)",
        "帮助(H)",
    ] {
        assert!(ui.find(title).is_ok(), "{title} is present");
    }

    ui.click("查看(V)").expect("the View title is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::MenuToggled(MenuId::View))),
        "clicking a title emits MenuToggled, got {messages:?}"
    );
}

#[test]
fn open_dropdown_renders_its_items() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.menu = Some(MenuId::File);

    let mut ui = simulator(view(&state));

    assert!(
        ui.find("新建 Quest").is_ok(),
        "File menu lists the new quest item"
    );
    assert!(
        ui.find("打开文件夹…").is_ok(),
        "File menu lists the folder picker"
    );
    assert!(
        ui.find("关闭菜单").is_ok(),
        "File menu offers the close entry"
    );
}

#[test]
fn quest_mode_shows_the_quest_rail_and_task_dialog() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;

    let mut ui = simulator(view(&state));

    assert!(
        ui.find("创建 Quest").is_ok(),
        "quest rail shows the create button"
    );
    assert!(
        ui.find("Quests").is_ok(),
        "quest rail shows its section caption"
    );
    assert!(
        ui.find("打开编辑器").is_ok(),
        "quest header offers the way back to the editor"
    );
    assert!(
        ui.find("Ask Codex…").is_ok(),
        "the task composer is shared with the editor shell"
    );
}

#[test]
fn quest_mode_returns_to_the_editor_via_the_header_button() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;

    let mut ui = simulator(view(&state));
    ui.click("打开编辑器")
        .expect("the editor return button is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ModeToggled)),
        "clicking the header button emits ModeToggled, got {messages:?}"
    );
}

#[test]
fn quest_header_offers_the_board_entry() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;

    let mut ui = simulator(view(&state));
    ui.click("看板").expect("the board entry is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::QuestBoardToggled)),
        "clicking the board entry emits QuestBoardToggled, got {messages:?}"
    );
}

#[test]
fn quest_board_overlay_lists_the_state_columns() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;
    state.quest.board_open = true;
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [idle_thread("t1", "alpha quest")],
        "nextCursor": null,
        "backwardsCursor": null
    }))
    .expect("list response decodes");
    state.sessions.apply_list(&response);

    let mut ui = simulator(view(&state));

    assert!(ui.find("My Quests").is_ok(), "the board title shows");
    assert!(ui.find("执行中 (0)").is_ok());
    assert!(ui.find("等待操作 (0)").is_ok());
    assert!(ui.find("已完成 (1)").is_ok());
    assert!(ui.find("alpha quest").is_ok(), "the card shows the quest");
}

#[test]
fn quest_scenario_picker_offers_all_scenarios() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;
    state.quest.picker_open = true;

    let mut ui = simulator(view(&state));
    for title in ["自动判断", "Spec 驱动", "原型探索", "创建工具"] {
        assert!(ui.find(title).is_ok(), "{title} is offered");
    }

    ui.click("Spec 驱动").expect("a scenario row is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::QuestScenarioPicked(Some(QuestScenario::Spec))
        )),
        "picking a scenario emits QuestScenarioPicked, got {messages:?}"
    );
}

#[test]
fn editor_area_shows_the_empty_state_before_any_file_opens() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;

    let mut ui = simulator(view(&state));

    assert!(ui.find("文件浏览区").is_ok(), "the empty-state title shows");
    assert!(
        ui.find("在左侧文件树中点击任意文件即可在此预览").is_ok(),
        "the file-tree hint shows"
    );
}

#[test]
fn editor_area_renders_open_tabs_and_the_active_body() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    let first = std::env::temp_dir().join(format!("sim-a-{}.rs", std::process::id()));
    let second = std::env::temp_dir().join(format!("sim-b-{}.rs", std::process::id()));
    std::fs::write(&first, b"fn first() {}").expect("fixture writes");
    std::fs::write(&second, b"fn second() {}").expect("fixture writes");
    state.editor.open(first.clone());
    state.editor.open(second.clone());
    state
        .editor
        .bodies
        .insert(second.clone(), FileBody::Text(Arc::from("fn second() {}")));

    let mut ui = simulator(view(&state));

    assert!(
        ui.find("fn second() {}").is_ok(),
        "the active tab body renders"
    );
    let first_name = first
        .file_name()
        .expect("fixture has a name")
        .to_string_lossy()
        .into_owned();
    let second_name = second
        .file_name()
        .expect("fixture has a name")
        .to_string_lossy()
        .into_owned();
    assert!(
        ui.find(first_name.as_str()).is_ok(),
        "the first tab renders"
    );
    assert!(
        ui.find(second_name.as_str()).is_ok(),
        "the second tab renders"
    );

    ui.click("×").expect("a close button is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::FilePreviewClosed(path) if *path == first
        )),
        "the first tab's close emits FilePreviewClosed, got {messages:?}"
    );

    let _ = std::fs::remove_file(&first);
    let _ = std::fs::remove_file(&second);
}

#[test]
fn quest_launch_page_replaces_the_welcome_while_idle() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;

    let mut ui = simulator(view(&state));

    assert!(ui.find("Quest on, hands off").is_ok(), "the headline shows");
    assert!(ui.find("运行于").is_ok(), "the run-on row shows");
    assert!(ui.find("本地模式").is_ok(), "the execution chip shows");
    assert!(ui.find("试试一个场景").is_ok(), "scenario suggestions show");
    assert!(ui.find("自动判断").is_ok(), "the auto scenario is offered");
    assert!(
        ui.find("What can I help you ship?").is_err(),
        "the generic welcome is replaced"
    );
}

#[test]
fn quest_launch_suggestions_resume_recent_threads() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;
    let response: ThreadListResponse = serde_json::from_value(json!({
        "data": [idle_thread("t9", "alpha quest")],
        "nextCursor": null,
        "backwardsCursor": null
    }))
    .expect("list response decodes");
    state.sessions.apply_list(&response);

    let mut ui = simulator(view(&state));
    assert!(ui.find("最近的 Quest").is_ok(), "recent quests are listed");

    ui.click("alpha quest")
        .expect("a suggestion row is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SessionSelected(id) if id == "t9")),
        "clicking a suggestion resumes the thread, got {messages:?}"
    );
}

#[test]
fn quest_workspace_menu_offers_the_folder_picker() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.mode = AppMode::Quest;
    state.quest.workspace_menu = true;

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("打开文件夹").is_ok(),
        "the menu lists the folder picker"
    );

    ui.click("打开文件夹").expect("the menu row is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::FolderPickRequested)),
        "clicking the entry opens the folder picker, got {messages:?}"
    );
}

/// A minimal idle thread payload for the board tests.
fn idle_thread(id: &str, preview: &str) -> serde_json::Value {
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
        "cwd": "/repo/a",
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
