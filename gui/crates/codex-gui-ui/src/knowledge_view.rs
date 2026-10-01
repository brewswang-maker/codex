//! The full-screen knowledge panel: capture workspace knowledge as
//! entries, search them, and quote one into the composer.

use crate::boards_shell;
use crate::message::Message;
use crate::state::State;
use crate::text_fit;
use crate::theme;
use codex_gui_core::KnowledgeEntry;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the full-screen knowledge panel.
pub fn panel(state: &State) -> Element<'_, Message> {
    let mut page = column![
        boards_shell::header("知识中心", Message::KnowledgePanelToggled),
        entry_form(state),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if let Some(notice) = &state.knowledge_panel.notice {
        page = page.push(boards_shell::notice_row(notice));
    }
    page = page.push(
        text_input(
            "搜索知识（标题 / 标签 / 正文）",
            &state.knowledge_panel.query,
        )
        .on_input(Message::KnowledgeQueryChanged)
        .width(Fill),
    );

    let query = state.knowledge_panel.query.trim();
    let entries: Vec<&KnowledgeEntry> = if query.is_empty() {
        state.knowledge.entries.iter().collect()
    } else {
        state.knowledge.search(query)
    };

    let mut list = column![].spacing(10).width(Fill);
    if entries.is_empty() {
        let empty = if query.is_empty() {
            "还没有知识条目；在上方沉淀第一条（命令、流程、约定…）"
        } else {
            "没有匹配的知识条目"
        };
        list = list.push(text(empty).size(theme::SIZE_SM).style(theme::dim));
    } else {
        list = list.push(
            text(format!(
                "共 {} 条{}",
                entries.len(),
                if query.is_empty() {
                    String::new()
                } else {
                    format!("（匹配「{query}」）")
                }
            ))
            .size(theme::SIZE_SM)
            .style(theme::dim),
        );
        for entry in entries {
            list = list.push(entry_card(entry));
        }
    }
    page.push(scrollable(list).height(Fill)).into()
}

/// The capture form; while editing it seeds the drafts and offers a
/// cancel button. The body stays a single-line input for now.
fn entry_form(state: &State) -> Element<'_, Message> {
    let panel = &state.knowledge_panel;
    let mut headline = row![
        text(if panel.editing.is_some() {
            "编辑条目"
        } else {
            "沉淀新知识"
        })
        .size(theme::SIZE_BODY)
    ]
    .spacing(8);
    if panel.editing.is_some() {
        headline = headline.push(boards_shell::small_button(
            "取消编辑",
            Message::KnowledgeEditCancelled,
        ));
    }
    let body = column![
        headline,
        boards_shell::form_row(
            "标题",
            text_input("如 构建命令", &panel.draft_title)
                .on_input(Message::KnowledgeDraftTitleChanged)
                .width(Fill),
        ),
        boards_shell::form_row(
            "标签",
            text_input("逗号分隔，如 构建, 发布", &panel.draft_tags)
                .on_input(Message::KnowledgeDraftTagsChanged)
                .width(Fill),
        ),
        boards_shell::form_row(
            "正文",
            text_input("知识内容", &panel.draft_body)
                .on_input(Message::KnowledgeDraftBodyChanged)
                .width(Fill),
        ),
        row![
            Space::new().width(Fill),
            button(
                text(if panel.editing.is_some() {
                    "保存修改"
                } else {
                    "沉淀"
                })
                .size(theme::SIZE_XS)
            )
            .padding([6, 16])
            .style(theme::primary_button)
            .on_press(Message::KnowledgeDraftSubmitted),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Fill);
    boards_shell::card(body)
}

/// One knowledge entry: title, tags, scope, a body excerpt, and the
/// quote/edit/delete actions.
fn entry_card(entry: &KnowledgeEntry) -> Element<'_, Message> {
    let tags = if entry.tags.is_empty() {
        String::new()
    } else {
        format!("#{}", entry.tags.join(" #"))
    };
    let info = column![
        row![
            text(entry.title.clone()).size(theme::SIZE_MD),
            text(tags).size(theme::SIZE_XS).style(theme::faint_text),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
        text(text_fit::ellipsize(&entry.body, 160))
            .size(theme::SIZE_SM)
            .style(theme::dim),
        text(format!("范围：{}", entry.scope))
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(4)
    .width(Fill);
    let actions = row![
        boards_shell::small_button("引用", Message::KnowledgeInjected(entry.id.clone())),
        boards_shell::small_button("编辑", Message::KnowledgeEditStarted(entry.id.clone())),
        boards_shell::small_button("删除", Message::KnowledgeEntryRemoved(entry.id.clone())),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    boards_shell::card(
        row![info, actions]
            .spacing(12)
            .align_y(iced::Alignment::Center),
    )
}
