//! Quest rows: one line per quest with its lifecycle badge and the inline
//! action menu (pin, fork, rename, delete).
//!
//! The row mirrors Qoder's task cards: a status dot, the quest name, the
//! live state badge, the age, and the "⋯" entry that expands the action
//! panel underneath.

use crate::chat;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::QuestStatus;
use codex_gui_core::ThreadSummary;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::text;

/// The status dot color; parked quests stay neutral.
fn status_color(status: QuestStatus) -> iced::Color {
    match status {
        QuestStatus::Idle => theme::BADGE,
        QuestStatus::Running => theme::INFO,
        QuestStatus::Waiting => theme::WARN,
        QuestStatus::Error => theme::DANGER,
    }
}

/// One quest row plus, while its menu is open, the inline action panel.
pub fn quest_row<'a>(
    state: &'a State,
    thread: &'a ThreadSummary,
    now: i64,
) -> Element<'a, Message> {
    let mut block = column![row_button(state, thread, now)].spacing(2);
    if state.quest.menu.thread_id.as_deref() == Some(thread.id.as_str()) {
        block = block.push(action_panel(state, thread));
    }
    block.into()
}

/// The row itself: click resumes the quest, "⋯" opens its menu.
fn row_button<'a>(state: &'a State, thread: &'a ThreadSummary, now: i64) -> Element<'a, Message> {
    let active = state.thread_id.as_deref() == Some(thread.id.as_str());
    let label = text(String::from(thread.label()))
        .size(theme::SIZE_MD)
        .style(theme::fg)
        .font(bold_if(active));

    let actions = button(text("⋯").size(theme::SIZE_SM).style(theme::dim))
        .padding([2, 6])
        .style(theme::ghost_button)
        .on_press(Message::QuestMenuToggled(thread.id.clone()));

    button(
        row![
            activity_dot(status_color(thread.status)),
            container(label).width(Fill),
            status_badge(thread.status),
            text(chat::relative_time(thread.updated_at, now))
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
            actions,
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([6, 8])
    .style(theme::rail_button(active))
    .on_press(Message::SessionSelected(thread.id.clone()))
    .into()
}

/// The live-state pill; parked quests show no badge.
fn status_badge(status: QuestStatus) -> Element<'static, Message> {
    if status == QuestStatus::Idle {
        return Space::new().width(0.0).height(0.0).into();
    }
    let color = status_color(status);
    container(
        text(status.label())
            .size(theme::SIZE_XS)
            .style(move |_theme| iced::widget::text::Style { color: Some(color) }),
    )
    .padding([1, 6])
    .style(badge_surface)
    .into()
}

/// The pale pill behind a state badge.
fn badge_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: 6.0.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The 6px activity dot at a row's left edge.
fn activity_dot(color: iced::Color) -> Element<'static, Message> {
    let dot = Space::new().width(6.0).height(6.0);
    let style = move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(color.into()),
        border: iced::Border {
            radius: 3.0.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    };
    container(dot).width(6.0).height(6.0).style(style).into()
}

/// The inline action panel under an open menu row. The delete entry arms
/// on first press and turns into its confirmation pair.
fn action_panel<'a>(state: &'a State, thread: &'a ThreadSummary) -> Element<'a, Message> {
    let mut actions = row![].spacing(2).align_y(alignment::Vertical::Center);

    if state.quest.menu.confirm_delete {
        actions = actions
            .push(menu_entry(
                "确认删除",
                true,
                Message::QuestDeleteRequested(thread.id.clone()),
            ))
            .push(menu_entry("取消", false, Message::QuestMenuClosed));
    } else {
        let pinned = state.pins.is_pinned(&thread.id);
        actions = actions
            .push(menu_entry(
                if pinned { "取消固定" } else { "固定" },
                false,
                Message::SessionPinToggled(thread.id.clone()),
            ))
            .push(menu_entry(
                "Fork",
                false,
                Message::QuestForkRequested(thread.id.clone()),
            ))
            .push(menu_entry(
                "重命名",
                false,
                Message::QuestRenameRequested(thread.id.clone()),
            ))
            .push(menu_entry("删除", true, Message::QuestDeleteArmed));
    }

    container(actions)
        .width(Fill)
        .padding([4, 6])
        .style(panel_surface)
        .into()
}

/// One action entry; danger entries render in red.
fn menu_entry(label: &'static str, danger: bool, message: Message) -> Element<'static, Message> {
    button(
        text(label)
            .size(theme::SIZE_XS)
            .style(move |_theme| iced::widget::text::Style {
                color: Some(if danger { theme::DANGER } else { theme::MUTED }),
            }),
    )
    .padding([3, 8])
    .style(theme::ghost_button)
    .on_press(message)
    .into()
}

/// The action panel's card surface.
fn panel_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

fn bold() -> font::Font {
    font::Font {
        weight: font::Weight::Bold,
        ..font::Font::default()
    }
}

/// A default font with bold weight applied or not.
fn bold_if(active: bool) -> font::Font {
    if active {
        bold()
    } else {
        font::Font::default()
    }
}
