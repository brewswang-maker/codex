//! The source-control pane: working-tree changes with a commit composer
//! and the recent-commit graph, mirroring the VS Code SCM layout.

use crate::icons::IconKind;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::GitStatus;
use iced::Element;
use iced::Fill;
use iced::Font;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the source-control pane for the active project.
pub fn panel(state: &State) -> Element<'_, Message> {
    if state.git.branch.is_none() {
        return crate::sessions_view::plain_pane(
            IconKind::SourceControl,
            "源代码管理",
            "当前目录不是 Git 仓库，没有可显示的更改。",
        );
    }

    let repository = repository_row(state);
    let composer = composer_row(state);
    let changes = changes_section(state);
    let graph = graph_section(state);

    scrollable(
        column![repository, composer, changes, graph]
            .width(Fill)
            .spacing(10)
            .padding([4, 2]),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

/// The repo line: project name, branch badge, and the refresh action.
fn repository_row(state: &State) -> Element<'_, Message> {
    let project = state.tree.root_path().file_name().map_or_else(
        || String::from("workspace"),
        |name| name.to_string_lossy().into_owned(),
    );
    let branch = state.git.branch.clone().unwrap_or_default();

    row![
        text(project).size(theme::SIZE_SM).style(theme::fg),
        text("⊢").size(theme::SIZE_XS).style(theme::faint_text),
        text(branch).size(theme::SIZE_XS).style(theme::dim),
        text(format!("{}⊙", state.git.change_count()))
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
        iced::widget::Space::new().width(Fill),
        action_button("⟳", Message::GitRefreshRequested),
    ]
    .spacing(6)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The commit composer: message well, submit, and the selection sync row.
fn composer_row(state: &State) -> Element<'_, Message> {
    let selected = state.git_overlay.selected.len();
    let branch = state.git.branch.clone().unwrap_or_default();
    let can_commit = !state.git_overlay.message.trim().is_empty() && selected > 0;
    let placeholder = format!("消息(Ctrl+Enter 在\"{branch}\"提交)");

    column![
        section_header(format!("更改 {}", state.git.change_count())),
        row![
            select_button(state, selected),
            text_input(&placeholder, &state.git_overlay.message)
                .on_input(Message::GitCommitDraftChanged)
                .on_submit_maybe(can_commit.then_some(Message::GitCommitRequested))
                .size(theme::SIZE_XS)
                .padding([5, 8])
                .style(theme::bare_input)
                .width(Fill),
            button(text("✓ 提交").size(theme::SIZE_XS).style(theme::fg))
                .padding([4, 10])
                .style(theme::primary_button)
                .on_press_maybe(can_commit.then_some(Message::GitCommitRequested)),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
        row![
            text(format!("同步更改 {}↑", selected))
                .size(theme::SIZE_XS)
                .style(theme::dim),
            iced::widget::Space::new().width(Fill),
            button(text("全选").size(theme::SIZE_XS).style(theme::dim))
                .padding([2, 6])
                .style(theme::ghost_button)
                .on_press(Message::GitFilesSelectAll(true)),
            button(text("清空").size(theme::SIZE_XS).style(theme::dim))
                .padding([2, 6])
                .style(theme::ghost_button)
                .on_press(Message::GitFilesSelectAll(false)),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    ]
    .spacing(6)
    .into()
}

/// The working-tree file rows; clicking a row toggles its commit mark.
fn changes_section(state: &State) -> Element<'_, Message> {
    let mut list = column![].spacing(1);
    for (index, (path, status)) in state.git.changed_files().into_iter().enumerate() {
        list = list.push(change_row(
            index,
            &path,
            status,
            state.git_overlay.selected.contains(&index),
        ));
    }
    list.into()
}

/// One selectable change row: commit mark, status letter, path.
fn change_row<'a>(
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
                .style(if checked {
                    theme::fg
                } else {
                    theme::faint_text
                }),
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

/// The recent-commit graph: a dotted rail of subjects with their hashes.
fn graph_section(state: &State) -> Element<'_, Message> {
    if state.git.commits.is_empty() {
        return column![].into();
    }

    let mut entries = column![].spacing(2);
    for commit in state.git.commits.iter().take(12) {
        entries = entries.push(
            row![
                text("●").size(theme::SIZE_XS).style(theme::brand),
                text(commit.subject.clone())
                    .size(theme::SIZE_XS)
                    .style(theme::fg),
                iced::widget::Space::new().width(Fill),
                text(commit.hash.clone())
                    .size(theme::SIZE_XS)
                    .font(Font::MONOSPACE)
                    .style(theme::faint_text),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center),
        );
    }

    column![section_header("图表".to_string()), entries]
        .spacing(6)
        .into()
}

/// The selection shortcut: one toggle that checks or clears everything.
fn select_button(state: &State, selected: usize) -> Element<'static, Message> {
    let all_checked = selected > 0 && selected == state.git.change_count();
    let label = if all_checked { "[x]" } else { "[ ]" };
    let message = if all_checked {
        Message::GitFilesSelectAll(false)
    } else {
        Message::GitFilesSelectAll(true)
    };
    button(
        text(label)
            .size(theme::SIZE_XS)
            .font(Font::MONOSPACE)
            .style(theme::dim),
    )
    .padding([4, 4])
    .style(theme::ghost_button)
    .on_press(message)
    .into()
}

/// A small quiet icon-style action button.
fn action_button(label: &'static str, message: Message) -> Element<'static, Message> {
    button(text(label).size(theme::SIZE_XS).style(theme::dim))
        .padding([2, 6])
        .style(theme::ghost_button)
        .on_press(message)
        .into()
}

/// A VS Code-style uppercase section caption.
fn section_header(label: String) -> Element<'static, Message> {
    container(
        text(label.to_uppercase())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    )
    .width(Fill)
    .into()
}

/// Status-letter colors: modified gold, added/untracked green, deleted red.
fn marker_color(status: GitStatus) -> impl Fn(&iced::Theme) -> iced::widget::text::Style {
    move |_theme| iced::widget::text::Style {
        color: Some(match status {
            GitStatus::Modified => theme::GOLD,
            GitStatus::Added | GitStatus::Untracked => theme::BRAND,
            GitStatus::Deleted => theme::DANGER,
        }),
    }
}
