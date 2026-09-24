//! The sidebar: resumable threads grouped by project, plus a directory-tree
//! view of the working directory (Files pane). Pane switching lives in the
//! activity bar.

use crate::chat;
use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::message::SidebarTab;
use crate::state::State;
use crate::theme;
use codex_gui_core::ThreadSummary;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Fixed sidebar width; the transcript takes the rest.
const SIDEBAR_WIDTH: f32 = 270.0;

/// Renders the sidebar dispatch on the active activity-bar tab.
pub fn sidebar(state: &State) -> Element<'_, Message> {
    let (title, body): (&str, Element<'_, Message>) = match state.sidebar_tab {
        SidebarTab::Threads => ("Threads", thread_list(state)),
        SidebarTab::Files => ("Files", files_body(state)),
        SidebarTab::Search => ("Search", search_pane()),
        SidebarTab::SourceControl => ("Source Control", source_control_body(state)),
        SidebarTab::Wiki => (
            "Repo Wiki",
            plain_pane(
                IconKind::Wiki,
                "Repo Wiki",
                "Wiki pages generated from the repository appear here.",
            ),
        ),
        SidebarTab::Debug => (
            "Run and Debug",
            plain_pane(
                IconKind::Debug,
                "Run and Debug",
                "No launch configurations. Debugging arrives with the embedded terminal.",
            ),
        ),
        SidebarTab::Remote => (
            "Remote Explorer",
            plain_pane(
                IconKind::Remote,
                "Remote Explorer",
                "No remote targets configured.",
            ),
        ),
        SidebarTab::Extensions => ("Extensions", extensions_body()),
    };

    let header = text(title).size(theme::SIZE_MD).style(theme::fg);

    container(
        column![header, body]
            .width(Fill)
            .height(Fill)
            .padding(8)
            .spacing(8),
    )
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .style(sidebar_surface)
    .into()
}

/// The working-directory tree pane: root caption, git block, tree.
fn files_body(state: &State) -> Element<'_, Message> {
    let root = text(state.tree.root_path().display().to_string())
        .size(theme::SIZE_XS)
        .style(theme::dim);
    // The tree widget ships its own scrollable; pin the row height so the
    // sidebar clips instead of growing with the expansion. The git block
    // above it mirrors `git status` for the active project.
    container(column![root, git_block(state), state.tree.view(Message::Tree)].spacing(4))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The search pane: an inert query well until file search lands.
fn search_pane() -> Element<'static, Message> {
    let input = text_input("Search files…", "")
        .size(theme::SIZE_XS)
        .padding([4, 8])
        .width(Fill);
    column![
        container(input).style(filter_well),
        text("Full-text file search lands with the embedded terminal milestone.")
            .size(theme::SIZE_XS)
            .style(theme::dim),
    ]
    .spacing(8)
    .padding([4, 2])
    .into()
}

/// The source-control pane: the git block, or a clean-tree hint.
fn source_control_body(state: &State) -> Element<'_, Message> {
    if state.git.is_empty() {
        return plain_pane(
            IconKind::SourceControl,
            "Source Control",
            "No source control changes detected.",
        );
    }
    git_block(state)
}

/// The extensions pane: the built-in inventory, statically listed.
fn extensions_body() -> Element<'static, Message> {
    let mut list = column![].spacing(4);
    for name in ["codex-gui", "codex-cli", "codex-mcp"] {
        list = list.push(
            row![
                Icon::new(IconKind::Extensions, theme::MUTED, 16.0).widget(),
                text(name).size(theme::SIZE_MD).style(theme::fg),
                iced::widget::Space::new().width(Fill),
                text("built-in").size(theme::SIZE_XS).style(faint),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([4, 2]),
        );
    }
    list.into()
}

/// A shared empty-pane hint: brand glyph, title, and one-line caption.
fn plain_pane(
    icon: IconKind,
    title: &'static str,
    hint: &'static str,
) -> Element<'static, Message> {
    column![
        Icon::new(icon, theme::BRAND, 28.0).widget(),
        text(title).size(theme::SIZE_MD).style(theme::fg),
        text(hint).size(theme::SIZE_XS).style(theme::dim),
    ]
    .spacing(8)
    .padding([12, 2])
    .into()
}

/// Threads grouped under their project directory, pinned ones on top,
/// both filtered by the sidebar search box (ZCode TaskList borrow).
fn thread_list(state: &State) -> Element<'_, Message> {
    let now = now_secs();
    let mut list = column![].spacing(6);

    let filter_row = text_input("Filter threads…", &state.sidebar_filter)
        .size(theme::SIZE_XS)
        .on_input(Message::SidebarFilterChanged)
        .padding([4, 8])
        .width(Fill);
    list = list.push(container(filter_row).style(filter_well));

    if state.sessions.threads().is_empty() {
        return container(scrollable(
            column![text("no threads yet").style(theme::dim)].padding([4, 2]),
        ))
        .width(Fill)
        .height(Fill)
        .into();
    }

    let (pinned, groups) = codex_gui_core::organized_sidebar(
        state.sessions.threads(),
        state.pins.id_set(),
        &state.sidebar_filter,
    );

    if pinned.is_empty() && groups.is_empty() {
        return container(scrollable(
            column![text("no matching threads").style(theme::dim)].padding([4, 2]),
        ))
        .width(Fill)
        .height(Fill)
        .into();
    }

    if !pinned.is_empty() {
        list = list.push(group_label("Pinned"));
        for thread in &pinned {
            list = list.push(thread_row(state, thread, now));
        }
    }
    for (cwd, threads) in groups {
        if threads.is_empty() {
            continue;
        }
        list = list.push(group_header(cwd));
        for thread in threads {
            list = list.push(thread_row(state, thread, now));
        }
    }

    container(scrollable(list.padding([2, 0])).height(Fill))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The pinned partition header.
fn group_label(label: &str) -> Element<'static, Message> {
    text(String::from(label))
        .size(theme::SIZE_XS)
        .style(theme::fg)
        .font(bold())
        .into()
}

/// The filter box well: hairline-bordered input surface (shared with the
/// Quest rail's filter).
pub(crate) fn filter_well(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// One project header: the directory's display name in small caps color.
fn group_header(cwd: &str) -> Element<'static, Message> {
    let name = std::path::Path::new(cwd).file_name().map_or_else(
        || String::from(cwd),
        |name| name.to_string_lossy().into_owned(),
    );
    text(name)
        .size(theme::SIZE_XS)
        .style(theme::fg)
        .font(bold())
        .into()
}

/// One thread row: white card while active, preview plus relative age,
/// a pin toggle, and (on the active row) the live diff totals badge.
fn thread_row<'a>(state: &'a State, thread: &'a ThreadSummary, now: i64) -> Element<'a, Message> {
    let id = thread.id.clone();
    let active = state.thread_id.as_deref() == Some(thread.id.as_str());

    let preview = if thread.preview.is_empty() {
        String::from("(untitled)")
    } else {
        String::from(thread.preview.as_str())
    };
    let label = text(preview)
        .size(theme::SIZE_MD)
        .style(theme::fg)
        .font(bold_if(active));

    let age = chat::relative_time(thread.updated_at, now);
    let pinned = state.pins.is_pinned(&thread.id);
    let pin = button(
        text(if pinned { "unpin" } else { "pin" })
            .size(theme::SIZE_XS)
            .style(faint),
    )
    .padding([2, 6])
    .style(theme::ghost_button)
    .on_press(Message::SessionPinToggled(id));
    let archive = button(text("archive").size(theme::SIZE_XS).style(faint))
        .padding([2, 6])
        .style(theme::ghost_button)
        .on_press(Message::SessionArchived(thread.id.clone()));

    let mut trailing = row![].spacing(6).align_y(alignment::Vertical::Center);
    if active {
        let (added, removed) = state.transcript.diff_totals();
        if added > 0 || removed > 0 {
            trailing = trailing
                .push(
                    text(format!("+{added}"))
                        .size(theme::SIZE_XS)
                        .style(added_style),
                )
                .push(
                    text(format!("-{removed}"))
                        .size(theme::SIZE_XS)
                        .style(removed_style),
                );
        }
    }
    trailing = trailing
        .push(text(age).size(theme::SIZE_XS).style(faint))
        .push(pin)
        .push(archive);

    container(
        button(row![container(label).width(Fill), trailing,])
            .padding([8, 10])
            .style(theme::rail_button(active))
            .on_press(Message::SessionSelected(thread.id.clone()))
            .width(Fill),
    )
    .width(Fill)
    .style(row_card(active))
    .into()
}

/// Added-line badge: brand green.
fn added_style(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::BRAND),
    }
}

/// Removed-line badge: danger red.
fn removed_style(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::DANGER),
    }
}

/// The row card: hairline-bordered white chip only while active.
fn row_card(active: bool) -> impl Fn(&iced::Theme) -> iced::widget::container::Style {
    move |_theme| {
        if active {
            iced::widget::container::Style {
                background: Some(theme::BG.into()),
                border: iced::Border {
                    color: theme::BORDER,
                    width: 1.0,
                    radius: theme::RADIUS_SM.into(),
                },
                ..iced::widget::container::Style::default()
            }
        } else {
            iced::widget::container::Style::default()
        }
    }
}

/// The git block: branch line plus the changed paths, mirroring
/// `git status` output for the active project.
fn git_block(state: &State) -> Element<'_, Message> {
    if state.git.is_empty() {
        return column![].into();
    }

    let mut block = column![].spacing(2);
    if let Some(branch) = &state.git.branch {
        block = block.push(
            row![
                text(branch).size(theme::SIZE_XS).style(theme::fg),
                iced::widget::Space::new().width(Fill),
                text(format!("{} changes", state.git.change_count()))
                    .size(theme::SIZE_XS)
                    .style(theme::faint_text),
            ]
            .align_y(iced::alignment::Vertical::Center),
        );
    }
    for (path, status) in state.git.changes.iter().take(8) {
        let name = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        block = block.push(
            row![
                text(String::from(status.marker()))
                    .size(theme::SIZE_XS)
                    .style(git_marker(*status)),
                text(name).size(theme::SIZE_XS).style(theme::dim),
            ]
            .spacing(6),
        );
    }
    // The commit composer entry (AgentCodeGUI borrow).
    block = block.push(
        button(text("Compose commit").size(theme::SIZE_XS).style(theme::fg))
            .padding([2, 6])
            .style(theme::ghost_button)
            .on_press(Message::GitOverlayToggled),
    );

    container(block)
        .padding([4, 2])
        .width(Fill)
        .style(git_well)
        .into()
}

/// Status-marker colors: modified gold, added/untracked green, deleted red.
fn git_marker(
    status: codex_gui_core::GitStatus,
) -> impl Fn(&iced::Theme) -> iced::widget::text::Style {
    move |_theme| iced::widget::text::Style {
        color: Some(match status {
            codex_gui_core::GitStatus::Modified => theme::GOLD,
            codex_gui_core::GitStatus::Added | codex_gui_core::GitStatus::Untracked => theme::BRAND,
            codex_gui_core::GitStatus::Deleted => theme::DANGER,
        }),
    }
}

/// The filled well behind the git block.
fn git_well(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The sidebar surface with its hairline divider from the main pane.
fn sidebar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::SIDEBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

fn faint(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::FAINT),
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

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_secs() as i64)
}

#[cfg(test)]
#[path = "sessions_view_tests.rs"]
mod tests;
