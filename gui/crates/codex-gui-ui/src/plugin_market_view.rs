//! The full-screen plugin-market panel: import built cdylib plugins
//! from a local path or a git URL, then enable, roll back, or remove
//! them. Invocation goes through `codex_gui_core::PluginMarket`.

use crate::boards_shell;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::PluginRecord;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the full-screen plugin-market panel.
pub fn panel(state: &State) -> Element<'_, Message> {
    let mut page = column![
        boards_shell::header("插件市场", Message::PluginMarketPanelToggled),
        import_form(state),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if let Some(notice) = &state.plugin_market_panel.notice {
        page = page.push(boards_shell::notice_row(notice));
    }

    let mut list = column![].spacing(10).width(Fill);
    if state.plugin_market.plugins.is_empty() {
        list = list.push(
            text("还没有安装插件；用本地路径或 git 地址导入已构建的插件")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
        list = list.push(
            text("插件用 codex-plugin-sdk 编写：实现 Plugin trait，crate-type 设为 [\"cdylib\"]，register_plugin! 导出入口；升级时旧二进制保留为 .bak 可一键回滚")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    } else {
        list = list.push(
            text(format!("共 {} 个插件", state.plugin_market.plugins.len()))
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
        for record in &state.plugin_market.plugins {
            list = list.push(plugin_card(record));
        }
    }
    page.push(scrollable(list).height(Fill)).into()
}

/// The import form: one source draft (path or git URL) plus the button.
fn import_form(state: &State) -> Element<'_, Message> {
    let market = &state.plugin_market;
    let body = column![
        text("导入 / 升级插件").size(theme::SIZE_BODY),
        boards_shell::form_row(
            "来源",
            text_input(
                "本地 plugin.so 路径，或 https://git 仓库地址",
                &market.source_draft
            )
            .on_input(Message::PluginMarketSourceChanged)
            .width(Fill),
        ),
        row![
            Space::new().width(Fill),
            button(text("导入").size(theme::SIZE_XS))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::PluginMarketImportSubmitted),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Fill);
    boards_shell::card(body)
}

/// One installed plugin: the enable toggle, version, source, cached
/// capabilities, and the rollback / uninstall actions.
fn plugin_card(record: &PluginRecord) -> Element<'_, Message> {
    let mut headline = row![
        checkbox(record.enabled)
            .label(record.name.clone())
            .on_toggle({
                let id = record.id.clone();
                move |_| Message::PluginMarketToggled(id.clone())
            }),
        text(format!("v{}", record.version))
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if !record.enabled {
        headline = headline.push(
            text("已禁用")
                .size(theme::SIZE_XS)
                .style(theme::danger_text),
        );
    }
    let info = column![
        headline,
        text(record.source.clone())
            .size(theme::SIZE_SM)
            .style(theme::dim),
        text(format!(
            "技能 {} · 命令 {}",
            record.skills.len(),
            record.commands.len()
        ))
        .size(theme::SIZE_XS)
        .style(theme::faint_text),
    ]
    .spacing(4)
    .width(Fill);
    let mut actions = row![
        boards_shell::small_button("回滚", Message::PluginMarketRolledBack(record.id.clone())),
        boards_shell::small_button("卸载", Message::PluginMarketUninstalled(record.id.clone())),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    let mut body = column![info].spacing(4);
    if let Some(error) = &record.last_error {
        actions = actions.push(
            text(error.clone())
                .size(theme::SIZE_XS)
                .style(theme::danger_text),
        );
    }
    body = body.push(actions);
    boards_shell::card(body)
}
