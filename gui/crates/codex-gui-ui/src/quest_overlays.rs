//! Quest-mode dialogs: the new-quest scenario picker and the rename dialog.
//!
//! Both sit above the board and below nothing else; the rename dialog
//! stacks on top of the picker so Esc resolves one layer at a time.

use crate::message::Message;
use crate::message::QuestScenario;
use crate::state::QuestRename;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::stack;
use iced::widget::text;
use iced::widget::text_input;

/// Focus id of the rename input, so opening the dialog can hand it the
/// keyboard without a click.
pub const RENAME_INPUT_ID: &str = "quest-rename-input";

/// The quest dialogs layered over `base`: the rename dialog on top, then
/// the scenario picker.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let with_picker: Element<'a, Message> = if state.quest.picker_open {
        stack![base.into(), picker_placement()].into()
    } else {
        base.into()
    };

    if let Some(rename) = &state.quest.rename {
        stack![with_picker, rename_placement(rename)].into()
    } else {
        with_picker
    }
}

/// The scenario picker card, centered over a dim mask.
fn picker_placement<'a>() -> Element<'a, Message> {
    let panel = container(picker_panel())
        .width(Fill)
        .max_width(460)
        .style(dialog_surface);
    container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(dim_mask)
        .into()
}

/// The scenario choices; picking one starts the Quest.
fn picker_panel() -> Element<'static, Message> {
    let mut list = column![].spacing(4);
    for scenario in QuestScenario::ALL {
        list = list.push(
            button(
                column![
                    text(scenario.title()).size(theme::SIZE_SM).style(theme::fg),
                    text(scenario.hint()).size(theme::SIZE_XS).style(theme::dim),
                ]
                .spacing(2),
            )
            .width(Fill)
            .padding([8, 10])
            .style(theme::card_button)
            .on_press(Message::QuestScenarioPicked(Some(scenario))),
        );
    }

    column![
        text("选择 Quest 场景")
            .size(theme::SIZE_MD)
            .style(theme::fg)
            .font(bold()),
        text("场景决定首轮对话的推进方式")
            .size(theme::SIZE_XS)
            .style(theme::dim),
        list,
        text("Esc 取消")
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(8)
    .padding(12)
    .into()
}

/// The rename dialog: one input plus the confirm pair.
fn rename_placement<'a>(rename: &'a QuestRename) -> Element<'a, Message> {
    let input = text_input("Quest 名称", &rename.draft)
        .id(RENAME_INPUT_ID)
        .on_input(Message::QuestRenameDraftChanged)
        .on_submit(Message::QuestRenameConfirmed)
        .size(theme::SIZE_SM)
        .padding([6, 8])
        .style(dialog_input);

    let panel = container(
        column![
            text("重命名 Quest")
                .size(theme::SIZE_MD)
                .style(theme::fg)
                .font(bold()),
            container(input).width(Fill),
            row![
                Space::new().width(Fill),
                button(text("取消").size(theme::SIZE_SM).style(theme::dim))
                    .padding([4, 12])
                    .style(theme::ghost_button)
                    .on_press(Message::QuestRenameCancelled),
                button(text("保存").size(theme::SIZE_SM))
                    .padding([4, 12])
                    .style(theme::primary_button)
                    .on_press(Message::QuestRenameConfirmed),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center),
        ]
        .spacing(10)
        .padding(12),
    )
    .width(Fill)
    .max_width(420)
    .style(dialog_surface);

    container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(dim_mask)
        .into()
}

/// The modal card behind both dialogs.
fn dialog_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_MD.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

fn dialog_input(
    _theme: &iced::Theme,
    _status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: theme::PANEL.into(),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        icon: theme::MUTED,
        placeholder: theme::FAINT,
        value: theme::TEXT,
        selection: iced::Color::from_rgb8(0xB4, 0xD5, 0xFE),
    }
}

/// The click-away mask: translucent black; Esc dismisses the dialogs.
fn dim_mask(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(
            iced::Color {
                a: 0.25,
                ..iced::Color::BLACK
            }
            .into(),
        ),
        ..iced::widget::container::Style::default()
    }
}

fn bold() -> font::Font {
    font::Font {
        weight: font::Weight::Bold,
        ..font::Font::default()
    }
}
