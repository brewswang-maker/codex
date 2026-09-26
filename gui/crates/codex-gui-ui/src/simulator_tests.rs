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
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::ThreadListResponse;
use codex_gui_bridge::Flags;
use codex_gui_core::MarketDetail;
use codex_gui_core::MarketPlugin;
use codex_gui_core::MarketSkill;
use codex_gui_core::SkillAdd;
use codex_gui_core::SkillEdit;
use codex_gui_core::SkillNotice;
use codex_gui_core::SkillRow;
use codex_gui_core::SkillsTab;
use iced_test::simulator;
use serde_json::json;
use std::path::PathBuf;
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

    assert!(
        ui.find("Ask Codex…（@ 提及文件，/ 命令）").is_ok(),
        "composer placeholder shows"
    );
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
        ui.find("Ask Codex…（@ 提及文件，/ 命令）").is_ok(),
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
        ui.find("Ask Codex…（@ 提及文件，/ 命令）").is_ok(),
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

#[test]
fn model_menu_lists_every_vendor_and_picks_a_row() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.vendors.apply_config(
        Some("glm-5.3"),
        Some("glm"),
        &[codex_gui_core::ConfiguredProvider {
            id: "glm".to_string(),
            name: Some("GLM (智谱)".to_string()),
            base_url: "https://open.bigmodel.cn/api/v1".to_string(),
        }],
    );
    state.vendors.attach_catalog(&[
        ("glm-5.3".to_string(), "glm-5.3".to_string(), String::new()),
        (
            "glm-5-turbo".to_string(),
            "glm-5-turbo".to_string(),
            String::new(),
        ),
    ]);
    state.model_menu_open = true;

    let mut ui = simulator(view(&state));
    assert!(ui.find("选择模型").is_ok(), "the menu title shows");
    assert!(
        ui.find("GLM (智谱)").is_ok(),
        "the configured vendor group shows"
    );
    assert!(
        ui.find("Kimi (月之暗面)").is_ok(),
        "unconfigured vendors stay listed"
    );
    assert!(
        ui.find("未配置 · 去设置").is_ok(),
        "unconfigured vendors link to settings"
    );

    ui.click("glm-5-turbo")
        .expect("a row of the configured vendor is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::VendorModelPicked { provider_id, slug }
                if provider_id == "glm" && slug == "glm-5-turbo"
        )),
        "clicking a row emits the vendor pick, got {messages:?}"
    );
}

#[test]
fn model_menu_mask_click_dismisses_the_menu() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.model_menu_open = true;

    // Inert panel space keeps the menu up.
    let mut ui = simulator(view(&state));
    ui.click("选择模型").expect("the caption is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.is_empty(),
        "clicking the panel caption is inert, got {messages:?}"
    );

    // The mask covers the window; a click outside the panel dismisses.
    let mut ui = simulator(view(&state));
    assert!(
        ui.find("选择模型").is_ok(),
        "the menu is up before dismissing"
    );
    ui.point_at(iced::Point::new(40.0, 700.0));
    let _ = ui.simulate(iced_test::simulator::click());

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ModelMenuClosed)),
        "clicking the mask emits ModelMenuClosed, got {messages:?}"
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

/// One market row of the shared mock marketplace.
fn market_plugin(installed: bool) -> MarketPlugin {
    MarketPlugin {
        id: "pdf-tools@local-market".to_string(),
        name: "pdf-tools".to_string(),
        wire_name: "pdf-tools".to_string(),
        marketplace: "local-market".to_string(),
        marketplace_path: Some(PathBuf::from("/tmp/marketplace.json")),
        description: Some("Split and merge PDF files".to_string()),
        source_label: "本地".to_string(),
        installed,
        enabled: installed,
    }
}

#[test]
fn skills_page_lists_rows_with_sources_and_actions() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.rows = vec![
        skill_row("local-skill", SkillScope::User, None),
        skill_row(
            "plugin-skill",
            SkillScope::User,
            Some("pdf-tools@local-market"),
        ),
        skill_row("sys-skill", SkillScope::System, None),
    ];

    let mut ui = simulator(view(&state));
    assert!(ui.find("Skills 管理").is_ok(), "the page title shows");
    assert!(ui.find("共 3 个 skill").is_ok(), "the row count shows");
    assert!(
        ui.find("$local-skill").is_ok(),
        "the local row shows its name"
    );
    assert!(
        ui.find("local-skill description").is_ok(),
        "the row shows its description"
    );
    assert!(ui.find("本地").is_ok(), "the local source badge shows");
    assert!(ui.find("插件").is_ok(), "the plugin source badge shows");
    assert!(ui.find("系统").is_ok(), "the system source badge shows");
    assert!(
        ui.find("Ask Codex…（@ 提及文件，/ 命令）").is_err(),
        "the page takes the whole surface"
    );

    ui.click("编辑").expect("the local row is editable");
    ui.click("删除").expect("the local row is deletable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillEditOpened(name) if name == "local-skill"
        )),
        "editing emits SkillEditOpened, got {messages:?}"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillDeleteArmed(name) if name == "local-skill"
        )),
        "deleting arms the confirmation, got {messages:?}"
    );
}

#[test]
fn skill_picker_localizes_descriptions_and_keeps_the_wire_name() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.picker_open = true;
    state.skills.rows = vec![
        skill_row(
            "superpowers:systematic-debugging",
            SkillScope::User,
            Some("superpowers@openai-api-curated"),
        ),
        skill_row("my-own-skill", SkillScope::User, None),
    ];

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("先系统定位根因，再提出修复").is_ok(),
        "a known skill shows its Chinese one-liner"
    );
    assert!(
        ui.find("my-own-skill description").is_ok(),
        "an unknown skill falls back to its wire description"
    );

    ui.click("$superpowers:systematic-debugging")
        .expect("the localized row is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillPicked(name) if name == "superpowers:systematic-debugging"
        )),
        "picking still inserts the wire name, got {messages:?}"
    );
}

#[test]
fn skills_row_toggle_emits_the_enabled_change() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.rows = vec![skill_row("local-skill", SkillScope::User, None)];

    let mut ui = simulator(view(&state));
    ui.click("$local-skill").expect("the checkbox is clickable");

    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillEnabledChanged { name, enabled }
                if name == "local-skill" && !enabled
        )),
        "toggling emits SkillEnabledChanged, got {messages:?}"
    );
}

#[test]
fn skills_delete_confirmation_renders_and_confirms() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.rows = vec![skill_row("local-skill", SkillScope::User, None)];
    state.skills.confirm_delete = Some("local-skill".to_string());

    let mut ui = simulator(view(&state));
    assert!(ui.find("确认删除？").is_ok(), "the confirmation copy shows");
    ui.click("删除").expect("the confirm button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillDeleteConfirmed(name) if name == "local-skill"
        )),
        "confirming emits SkillDeleteConfirmed, got {messages:?}"
    );

    let mut ui = simulator(view(&state));
    ui.click("取消").expect("the cancel button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillDeleteCancelled)),
        "cancelling emits SkillDeleteCancelled, got {messages:?}"
    );
}

#[test]
fn skills_add_form_renders_and_submits() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.add = Some(SkillAdd::default());

    let mut ui = simulator(view(&state));
    assert!(ui.find("添加 Skill").is_ok(), "the form card shows");
    assert!(ui.find("链接").is_ok(), "the link input is offered");
    assert!(
        ui.find("导入目录…").is_ok(),
        "the directory import is offered"
    );
    assert!(
        ui.find("导入 SKILL.md…").is_ok(),
        "the file import is offered"
    );
    assert!(
        ui.find("将在用户 skills 目录下新建目录并写入 SKILL.md")
            .is_ok(),
        "the create hint shows"
    );

    ui.click("保存").expect("the save button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillAddSubmitted)),
        "saving emits SkillAddSubmitted, got {messages:?}"
    );
}

#[test]
fn skills_add_form_shows_the_link_hint_and_busy_state() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.add = Some(SkillAdd {
        link: "https://github.com/obra/superpowers".to_string(),
        ..SkillAdd::default()
    });

    {
        let mut ui = simulator(view(&state));
        assert!(
            ui.find("从 Git 仓库克隆并导入其 SKILL.md（含多个 skill 时全部导入）；链接优先于名称/描述与本地来源")
                .is_ok(),
            "the link hint shows while the draft is non-empty"
        );
    }

    state.skills.add.as_mut().expect("form").busy = true;
    let mut ui = simulator(view(&state));
    assert!(ui.find("导入中…").is_ok(), "the running clone shows");
    assert!(
        ui.find("保存").is_err(),
        "the save button is locked while busy"
    );
}

#[test]
fn skills_add_form_shows_the_picked_import_source() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.add = Some(SkillAdd {
        source: Some(PathBuf::from("/tmp/imported-skill")),
        ..SkillAdd::default()
    });

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("/tmp/imported-skill").is_ok(),
        "the picked path shows"
    );
    assert!(
        ui.find("导入其 SKILL.md；名称与描述在文件中读取").is_ok(),
        "the import hint replaces the drafts"
    );

    ui.click("×").expect("the clear button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillAddSourcePicked(None))),
        "clearing emits SkillAddSourcePicked(None), got {messages:?}"
    );
}

#[test]
fn skills_edit_form_shows_and_submits() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.editor = Some(SkillEdit {
        name: "local-skill".to_string(),
        path: PathBuf::from("/tmp/skills/local-skill/SKILL.md"),
        name_draft: "local-skill".to_string(),
        description_draft: "local-skill description".to_string(),
    });

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("编辑 `$local-skill`").is_ok(),
        "the edit form shows its target"
    );

    ui.click("保存").expect("the save button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillEditSubmitted)),
        "saving emits SkillEditSubmitted, got {messages:?}"
    );
}

#[test]
fn skills_market_tab_offers_install_and_refresh() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.tab = SkillsTab::Market;
    state.skills.market.loaded = true;
    state.skills.market.plugins = vec![market_plugin(false)];

    let mut ui = simulator(view(&state));
    assert!(ui.find("pdf-tools").is_ok(), "the installable plugin shows");
    assert!(
        ui.find("local-market · 本地").is_ok(),
        "the marketplace and source label show"
    );
    assert!(
        ui.find("Split and merge PDF files").is_ok(),
        "the description shows"
    );
    assert!(ui.find("刷新").is_ok(), "the refresh action shows");

    ui.click("安装").expect("the install button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillMarketInstallRequested(id) if id == "pdf-tools@local-market"
        )),
        "installing emits SkillMarketInstallRequested, got {messages:?}"
    );

    let mut ui = simulator(view(&state));
    ui.click("刷新").expect("the refresh button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillMarketRefreshRequested)),
        "refreshing emits SkillMarketRefreshRequested, got {messages:?}"
    );
}

#[test]
fn skills_market_installed_row_offers_uninstall() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.tab = SkillsTab::Market;
    state.skills.market.loaded = true;
    state.skills.market.plugins = vec![market_plugin(true)];

    let mut ui = simulator(view(&state));
    assert!(ui.find("已安装").is_ok(), "the installed chip shows");

    ui.click("卸载").expect("the uninstall button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillUninstallRequested(id) if id == "pdf-tools@local-market"
        )),
        "uninstalling emits SkillUninstallRequested, got {messages:?}"
    );
}

#[test]
fn skills_market_detail_lists_the_bundled_skills() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.tab = SkillsTab::Market;
    state.skills.market.loaded = true;
    state.skills.market.plugins = vec![market_plugin(false)];
    state.skills.market.detail = Some(MarketDetail {
        plugin_id: "pdf-tools@local-market".to_string(),
        skills: vec![
            MarketSkill {
                name: "pdf-split".to_string(),
                description: "Split a PDF into pages".to_string(),
            },
            MarketSkill {
                name: "pdf-merge".to_string(),
                description: "Merge several PDFs into one".to_string(),
            },
        ],
    });

    let mut ui = simulator(view(&state));
    assert!(ui.find("$pdf-split").is_ok(), "the bundled skill shows");
    assert!(
        ui.find("Split a PDF into pages").is_ok(),
        "the bundled skill description shows"
    );
    assert!(ui.find("$pdf-merge").is_ok(), "the second skill shows");

    ui.click("收起").expect("the collapse button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SkillMarketPluginToggled(id) if id == "pdf-tools@local-market"
        )),
        "collapsing emits SkillMarketPluginToggled, got {messages:?}"
    );
}

#[test]
fn skills_market_loading_and_empty_states() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.tab = SkillsTab::Market;

    {
        let mut ui = simulator(view(&state));
        assert!(
            ui.find("正在加载 Skill 市场…").is_ok(),
            "the loading hint shows before the first list"
        );
    }

    state.skills.market.loaded = true;
    let mut ui = simulator(view(&state));
    assert!(
        ui.find("暂无可安装条目；点击「刷新」重试，或添加一个市场来源")
            .is_ok(),
        "the empty hint shows after an empty list"
    );
}

#[test]
fn skills_page_closes_and_switches_tabs() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;

    let mut ui = simulator(view(&state));
    ui.click("关闭").expect("the close button is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillsPageToggled)),
        "closing emits SkillsPageToggled, got {messages:?}"
    );

    let mut ui = simulator(view(&state));
    ui.click("Skill 市场").expect("the market tab is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillsTabPicked(SkillsTab::Market))),
        "switching tabs emits SkillsTabPicked, got {messages:?}"
    );
}

#[test]
fn skills_page_renders_the_mutation_notice() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;
    state.skills.notice = Some(SkillNotice::ok("已创建 skill `demo`"));

    {
        let mut ui = simulator(view(&state));
        assert!(
            ui.find("已创建 skill `demo`").is_ok(),
            "the success notice shows"
        );
    }

    state.skills.notice = Some(SkillNotice::failed("操作失败：disk full"));
    let mut ui = simulator(view(&state));
    assert!(
        ui.find("操作失败：disk full").is_ok(),
        "the failure notice shows"
    );
}

#[test]
fn skills_empty_inventory_shows_the_add_hint() {
    let mut state = State::new(Flags::default_app_server());
    state.status = Status::Ready;
    state.skills.page_open = true;

    let mut ui = simulator(view(&state));
    assert!(
        ui.find("暂无 skill；点击「+ 添加 Skill」新建或导入")
            .is_ok(),
        "the empty hint shows"
    );

    ui.click("+ 添加 Skill")
        .expect("the add entry is clickable");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SkillAddOpened)),
        "the add entry emits SkillAddOpened, got {messages:?}"
    );
}
