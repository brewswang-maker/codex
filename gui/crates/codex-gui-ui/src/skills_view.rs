//! The full-screen Skills page: the managed inventory (add / edit / delete
//! / enable) on one tab, the installable Skill market on the other.
//!
//! The page replaces the whole chat surface while open, mirroring the
//! settings panel; every mutation round-trips through `commands` and the
//! settled results land in `state.skills.notice`.

use crate::message::Message;
use crate::message::SkillImportTarget;
use crate::state::State;
use crate::theme;
use codex_app_server_protocol::SkillScope;
use codex_gui_core::MarketPlugin;
use codex_gui_core::SkillAdd;
use codex_gui_core::SkillEdit;
use codex_gui_core::SkillNotice;
use codex_gui_core::SkillRow;
use codex_gui_core::SkillSource;
use codex_gui_core::SkillsTab;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the full-screen Skills page.
pub fn page(state: &State) -> Element<'_, Message> {
    let mut panel = column![
        row![
            text("Skills 管理").size(20),
            Space::new().width(Fill),
            button(text("关闭")).on_press(Message::SkillsPageToggled),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
        tab_row(state),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if let Some(notice) = &state.skills.notice {
        panel = panel.push(notice_row(notice));
    }

    let body = match state.skills.tab {
        SkillsTab::Manage => manage_tab(state),
        SkillsTab::Market => market_tab(state),
    };
    panel.push(scrollable(body).height(Fill)).into()
}

/// The two halves of the page, with the active one in brand green.
fn tab_row(state: &State) -> Element<'_, Message> {
    let manage_active = state.skills.tab == SkillsTab::Manage;
    row![
        tab_button("Skill 管理", SkillsTab::Manage, manage_active),
        tab_button("Skill 市场", SkillsTab::Market, !manage_active),
    ]
    .spacing(8)
    .into()
}

/// One tab button: brand-filled while active, ghost otherwise.
fn tab_button(label: &'static str, tab: SkillsTab, active: bool) -> Element<'static, Message> {
    let style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style =
        if active {
            theme::primary_button
        } else {
            theme::ghost_button
        };
    button(text(label).size(theme::SIZE_SM))
        .padding([6, 16])
        .style(style)
        .on_press(Message::SkillsTabPicked(tab))
        .into()
}

/// The last mutation's outcome: green on success, red on failure.
fn notice_row(notice: &SkillNotice) -> Element<'_, Message> {
    let style: fn(&iced::Theme) -> iced::widget::text::Style = if notice.error {
        theme::danger_text
    } else {
        theme::brand
    };
    container(text(notice.text.clone()).size(theme::SIZE_SM).style(style))
        .width(Fill)
        .padding([4, 8])
        .style(theme::surface(theme::CARD))
        .into()
}

/// The managed inventory: the row count, the add entry, and one card per
/// discovered skill.
fn manage_tab(state: &State) -> Element<'_, Message> {
    let mut tab = column![
        row![
            text(format!("共 {} 个 skill", state.skills.rows.len()))
                .size(theme::SIZE_SM)
                .style(theme::dim),
            Space::new().width(Fill),
            button(text("+ 添加 Skill").size(theme::SIZE_SM))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::SkillAddOpened),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(10)
    .width(Fill);

    if let Some(form) = &state.skills.add {
        tab = tab.push(add_form(form));
    }
    if let Some(form) = &state.skills.editor {
        tab = tab.push(edit_form(form));
    }

    if state.skills.rows.is_empty() {
        tab = tab.push(
            text("暂无 skill；点击「+ 添加 Skill」新建或导入")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    }
    for skill in &state.skills.rows {
        tab = tab.push(skill_card(state, skill));
    }
    tab.into()
}

/// One inventory row: the enable toggle, name, source badge, scope,
/// description, and the per-row edit/delete actions.
fn skill_card<'a>(state: &'a State, skill: &'a SkillRow) -> Element<'a, Message> {
    let description = if skill.description.is_empty() {
        "（无描述）".to_string()
    } else {
        skill.description.clone()
    };
    let info = column![
        row![
            checkbox(skill.enabled)
                .label(format!("${}", skill.name))
                .on_toggle({
                    let name = skill.name.clone();
                    move |enabled| Message::SkillEnabledChanged {
                        name: name.clone(),
                        enabled,
                    }
                }),
            source_badge(skill.source()),
            text(scope_label(skill.scope))
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        text(description).size(theme::SIZE_SM).style(theme::dim),
        text(skill.path.display().to_string())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(4)
    .width(Fill);

    let mut actions = row![].spacing(6).align_y(iced::Alignment::Center);
    if skill.editable() {
        actions = actions.push(
            button(text("编辑").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillEditOpened(skill.name.clone())),
        );
    }
    if skill.deletable() {
        if state.skills.confirm_delete.as_deref() == Some(skill.name.as_str()) {
            actions = actions.push(
                text("确认删除？")
                    .size(theme::SIZE_XS)
                    .style(theme::danger_text),
            );
            actions = actions.push(
                button(text("删除").size(theme::SIZE_XS))
                    .padding([4, 12])
                    .style(theme::danger_button)
                    .on_press(Message::SkillDeleteConfirmed(skill.name.clone())),
            );
            actions = actions.push(
                button(text("取消").size(theme::SIZE_XS))
                    .style(theme::ghost_button)
                    .on_press(Message::SkillDeleteCancelled),
            );
        } else {
            actions = actions.push(
                button(text("删除").size(theme::SIZE_XS))
                    .style(theme::ghost_button)
                    .on_press(Message::SkillDeleteArmed(skill.name.clone())),
            );
        }
    }

    container(
        row![info, actions]
            .spacing(12)
            .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .padding(10)
    .style(theme::card)
    .into()
}

/// The source as a small badge.
fn source_badge(source: SkillSource) -> Element<'static, Message> {
    container(text(source.label()).size(theme::SIZE_XS))
        .padding([1, 6])
        .style(theme::surface(theme::CARD))
        .into()
}

/// The add form: create from the drafts, import a picked directory or
/// `SKILL.md` file, or clone a Git link.
fn add_form(form: &SkillAdd) -> Element<'_, Message> {
    let mut card = column![text("添加 Skill").size(theme::SIZE_BODY)].spacing(8);
    if let Some(source) = &form.source {
        card = card.push(
            row![
                text("来源").width(80),
                text(source.display().to_string())
                    .size(theme::SIZE_SM)
                    .style(theme::dim),
                button(text("×").size(theme::SIZE_XS))
                    .style(theme::ghost_button)
                    .on_press(Message::SkillAddSourcePicked(None)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
        card = card.push(
            text("导入其 SKILL.md；名称与描述在文件中读取")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    } else {
        card = card.push(
            row![
                text("名称").width(80),
                text_input("skill 名称", &form.name)
                    .on_input(Message::SkillAddNameChanged)
                    .width(Fill),
            ]
            .spacing(12),
        );
        card = card.push(
            row![
                text("描述").width(80),
                text_input("一句话描述", &form.description)
                    .on_input(Message::SkillAddDescriptionChanged)
                    .width(Fill),
            ]
            .spacing(12),
        );
        card = card.push(
            text("将在用户 skills 目录下新建目录并写入 SKILL.md")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    }

    card = card.push(
        row![
            text("链接").width(80),
            text_input("https://github.com/<owner>/<repo>", &form.link)
                .on_input(Message::SkillAddLinkChanged)
                .width(Fill),
        ]
        .spacing(12),
    );
    if !form.link.trim().is_empty() {
        card = card.push(
            text("从 Git 仓库克隆并导入其 SKILL.md（含多个 skill 时全部导入）；链接优先于名称/描述与本地来源")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    }

    let save = if form.busy {
        button(text("导入中…").size(theme::SIZE_XS))
            .padding([6, 16])
            .style(theme::primary_button)
    } else {
        button(text("保存").size(theme::SIZE_XS))
            .padding([6, 16])
            .style(theme::primary_button)
            .on_press(Message::SkillAddSubmitted)
    };
    card = card.push(
        row![
            button(text("导入目录…").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillAddImportRequested(
                    SkillImportTarget::Directory
                )),
            button(text("导入 SKILL.md…").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillAddImportRequested(SkillImportTarget::File)),
            Space::new().width(Fill),
            button(text("取消").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillAddCancelled),
            save,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    container(card)
        .width(Fill)
        .padding(10)
        .style(theme::card)
        .into()
}

/// The edit form of one local skill: name and description drafts.
fn edit_form(form: &SkillEdit) -> Element<'_, Message> {
    let card = column![
        text(format!("编辑 `${}`", form.name)).size(theme::SIZE_BODY),
        row![
            text("名称").width(80),
            text_input("skill 名称", &form.name_draft)
                .on_input(Message::SkillEditNameChanged)
                .width(Fill),
        ]
        .spacing(12),
        row![
            text("描述").width(80),
            text_input("一句话描述", &form.description_draft)
                .on_input(Message::SkillEditDescriptionChanged)
                .width(Fill),
        ]
        .spacing(12),
        row![
            Space::new().width(Fill),
            button(text("取消").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillEditCancelled),
            button(text("保存").size(theme::SIZE_XS))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::SkillEditSubmitted),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(8);

    container(card)
        .width(Fill)
        .padding(10)
        .style(theme::card)
        .into()
}

/// The market tab: the marketplace-source entry, a refresh action, and one
/// card per installable plugin.
fn market_tab(state: &State) -> Element<'_, Message> {
    let market = &state.skills.market;
    let mut tab = column![
        row![
            text_input(
                "添加市场：owner/repo、git URL 或本地路径",
                &market.source_draft
            )
            .on_input(Message::SkillMarketSourceChanged)
            .width(Fill),
            button(text("添加市场").size(theme::SIZE_SM))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::SkillMarketSourceSubmitted),
            button(text("刷新").size(theme::SIZE_SM))
                .style(theme::ghost_button)
                .on_press(Message::SkillMarketRefreshRequested),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(10)
    .width(Fill);

    if !market.loaded {
        tab = tab.push(
            text("正在加载 Skill 市场…")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    } else if market.plugins.is_empty() {
        tab = tab.push(
            text("暂无可安装条目；点击「刷新」重试，或添加一个市场来源")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    }
    for plugin in &market.plugins {
        tab = tab.push(market_card(state, plugin));
    }
    tab.into()
}

/// One market row: the plugin identity, its install state, and the
/// install/uninstall plus expand actions.
fn market_card<'a>(state: &'a State, plugin: &'a MarketPlugin) -> Element<'a, Message> {
    let market = &state.skills.market;
    let busy = market.busy_plugin.as_deref() == Some(plugin.id.as_str());
    let expanded = market
        .detail
        .as_ref()
        .is_some_and(|detail| detail.plugin_id == plugin.id)
        || market.detail_loading.as_deref() == Some(plugin.id.as_str());

    let mut header = row![
        text(plugin.name.clone())
            .size(theme::SIZE_BODY)
            .style(theme::fg),
        text(format!("{} · {}", plugin.marketplace, plugin.source_label))
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if plugin.installed {
        header = header.push(
            container(text("已安装").size(theme::SIZE_XS))
                .padding([1, 6])
                .style(theme::surface(theme::CARD)),
        );
    }

    let mut actions = row![].spacing(6).align_y(iced::Alignment::Center);
    actions = actions.push(
        button(text(if expanded { "收起" } else { "详情" }).size(theme::SIZE_XS))
            .style(theme::ghost_button)
            .on_press(Message::SkillMarketPluginToggled(plugin.id.clone())),
    );
    if busy {
        actions = actions.push(text("处理中…").size(theme::SIZE_XS).style(theme::dim));
    } else if plugin.installed {
        actions = actions.push(
            button(text("卸载").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::SkillUninstallRequested(plugin.id.clone())),
        );
    } else {
        actions = actions.push(
            button(text("安装").size(theme::SIZE_XS))
                .padding([4, 12])
                .style(theme::primary_button)
                .on_press(Message::SkillMarketInstallRequested(plugin.id.clone())),
        );
    }

    let mut card =
        column![row![header, Space::new().width(Fill), actions].align_y(iced::Alignment::Center)]
            .spacing(6);
    if let Some(description) = &plugin.description {
        card = card.push(
            text(description.clone())
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    }
    if expanded {
        card = card.push(market_detail(state, &plugin.id));
    }

    container(card)
        .width(Fill)
        .padding(10)
        .style(theme::card)
        .into()
}

/// The expanded plugin's bundled skills, or its loading / empty hint.
fn market_detail<'a>(state: &'a State, plugin_id: &str) -> Element<'a, Message> {
    let market = &state.skills.market;
    if market.detail_loading.as_deref() == Some(plugin_id) {
        return text("正在加载插件详情…")
            .size(theme::SIZE_XS)
            .style(theme::dim)
            .into();
    }
    let Some(detail) = market
        .detail
        .as_ref()
        .filter(|detail| detail.plugin_id == plugin_id)
    else {
        return column![].into();
    };
    if detail.skills.is_empty() {
        return text("该插件未包含 skill")
            .size(theme::SIZE_XS)
            .style(theme::dim)
            .into();
    }
    let mut skills = column![].spacing(6);
    for skill in &detail.skills {
        skills = skills.push(
            column![
                text(format!("${}", skill.name))
                    .size(theme::SIZE_SM)
                    .style(theme::fg),
                text(skill.description.clone())
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            ]
            .spacing(2),
        );
    }
    skills.into()
}

/// The wire scope as a display label.
fn scope_label(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::User => "user",
        SkillScope::Repo => "repo",
        SkillScope::System => "system",
        SkillScope::Admin => "admin",
    }
}
