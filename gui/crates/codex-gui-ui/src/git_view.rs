//! The commit-composer overlay: pick working-tree files, draft a message
//! (by hand or via a throwaway AI thread), stage and commit them
//! (borrowed from AgentCodeGUI's commit composer).

use crate::message::Message;
use crate::state::GitOverlayStatus;
use crate::state::State;
use crate::theme;
use codex_gui_core::GitStatus;
use iced::Color;
use iced::Element;
use iced::Fill;
use iced::Font;
use iced::alignment;
use iced::widget::button;
use iced::widget::center;
use iced::widget::column;
use iced::widget::container;
use iced::widget::mouse_area;
use iced::widget::opaque;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;
use iced::widget::text_input;

/// Overlay dimensions, in the approval-dialog size class.
const OVERLAY_WIDTH: f32 = 560.0;
const OVERLAY_HEIGHT: f32 = 480.0;

/// Wraps the main UI with the commit composer while it is open; the
/// mask dismisses (a running draft is cancelled by that close).
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.git_overlay.open {
        return base.into();
    }

    stack![
        base.into(),
        mouse_area(opaque(
            container(iced::widget::Space::new().width(Fill).height(Fill)).style(mask),
        ))
        .on_press(Message::GitOverlayToggled),
        center(opaque(overlay(state))),
    ]
    .into()
}

fn overlay(state: &State) -> Element<'_, Message> {
    let files = state.git.changed_files();
    let selected = &state.git_overlay.selected;

    let mut list = column![].spacing(2);
    for (index, (path, status)) in files.iter().enumerate() {
        list = list.push(file_row(index, path, *status, selected.contains(&index)));
    }

    let can_commit = !state.git_overlay.message.trim().is_empty() && !selected.is_empty();
    let drafting = state.git_overlay.status == GitOverlayStatus::Drafting;

    let status_line: Element<'_, Message> = match &state.git_overlay.status {
        GitOverlayStatus::Idle => text("").size(theme::SIZE_XS).into(),
        GitOverlayStatus::Drafting => text("Drafting a commit message on a throwaway thread…")
            .size(theme::SIZE_XS)
            .style(theme::dim)
            .into(),
        GitOverlayStatus::Failed(reason) => text(format!("Failed: {reason}"))
            .size(theme::SIZE_XS)
            .style(danger)
            .into(),
    };

    container(
        column![
            row![
                text("Compose commit")
                    .size(theme::SIZE_BODY)
                    .style(theme::fg),
                iced::widget::Space::new().width(Fill),
                button(text("Close").size(theme::SIZE_SM).style(theme::dim))
                    .padding([4, 10])
                    .style(quiet_action)
                    .on_press(Message::GitOverlayToggled),
            ]
            .align_y(alignment::Vertical::Center),
            row![
                button(text("all").size(theme::SIZE_XS).style(theme::fg))
                    .padding([2, 8])
                    .style(quiet_action)
                    .on_press(Message::GitFilesSelectAll(true)),
                button(text("none").size(theme::SIZE_XS).style(theme::fg))
                    .padding([2, 8])
                    .style(quiet_action)
                    .on_press(Message::GitFilesSelectAll(false)),
                iced::widget::Space::new().width(Fill),
                text(format!("{} / {} selected", selected.len(), files.len()))
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
            scrollable(list).height(Fill),
            status_line,
            text_input("Commit message", &state.git_overlay.message)
                .on_input(Message::GitCommitDraftChanged)
                .on_submit_maybe(can_commit.then_some(Message::GitCommitRequested))
                .size(theme::SIZE_SM)
                .padding(6)
                .style(theme::bare_input)
                .width(Fill),
            row![
                button(text("AI draft").size(theme::SIZE_SM).style(theme::fg),)
                    .padding([4, 10])
                    .style(quiet_action)
                    .on_press_maybe((!drafting).then_some(Message::GitAiDraftRequested)),
                iced::widget::Space::new().width(Fill),
                button(
                    container(text("Commit").size(theme::SIZE_SM))
                        .align_x(alignment::Horizontal::Center)
                        .width(Fill),
                )
                .padding([5, 16])
                .width(96)
                .style(theme::primary_button)
                .on_press_maybe(can_commit.then_some(Message::GitCommitRequested)),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center),
        ]
        .spacing(10),
    )
    .width(Fill)
    .max_width(OVERLAY_WIDTH)
    .height(Fill)
    .max_height(OVERLAY_HEIGHT)
    .padding(16)
    .style(dialog_surface)
    .into()
}

/// One selectable working-tree row: checkbox, status marker, path.
fn file_row<'a>(
    index: usize,
    path: &str,
    status: GitStatus,
    checked: bool,
) -> Element<'a, Message> {
    button(
        row![
            text(if checked { "[x]" } else { "[ ]" })
                .size(theme::SIZE_XS)
                .font(Font::MONOSPACE)
                .style(theme::dim),
            text(String::from(status.marker()))
                .size(theme::SIZE_XS)
                .font(Font::MONOSPACE)
                .style(marker_color(status)),
            text(String::from(path))
                .size(theme::SIZE_XS)
                .style(theme::fg),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .padding([3, 4])
    .width(Fill)
    .style(theme::ghost_button)
    .on_press(Message::GitFileToggled(index))
    .into()
}

/// Status-marker colors: modified gold, added/untracked green, deleted red.
fn marker_color(status: GitStatus) -> impl Fn(&iced::Theme) -> iced::widget::text::Style {
    move |_theme| iced::widget::text::Style {
        color: Some(match status {
            GitStatus::Modified => theme::GOLD,
            GitStatus::Added | GitStatus::Untracked => theme::BRAND,
            GitStatus::Deleted => theme::DANGER,
        }),
    }
}

fn danger(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::DANGER),
    }
}

fn mask(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(
            Color {
                a: 0.45,
                ..Color::BLACK
            }
            .into(),
        ),
        ..container::Style::default()
    }
}

fn dialog_surface(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_LG.into(),
        },
        ..container::Style::default()
    }
}

fn quiet_action(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(theme::BG.into()),
        text_color: theme::MUTED,
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..button::Style::default()
    }
}
