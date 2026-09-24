//! My Quests: the board overlay with running / waiting / finished columns,
//! a search box, and per-project tabs.
//!
//! Mirrors Qoder's quest board: the same three state columns over the whole
//! inventory, independent from the rail's grouping. Error quests land in
//! the finished column carrying a red tag.

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
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;
use iced::widget::text_input;
use std::collections::BTreeSet;

/// The board overlay layered over `base` while open.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.quest.board_open {
        return base.into();
    }

    let panel = container(board_panel(state))
        .width(Fill)
        .height(Fill)
        .max_width(960)
        .style(board_surface);
    let placement = container(panel)
        .width(Fill)
        .height(Fill)
        .padding(
            iced::Padding::default()
                .top(36)
                .bottom(36)
                .left(48)
                .right(48),
        )
        .align_x(alignment::Horizontal::Center)
        .style(dim_mask);

    stack![base.into(), placement].into()
}

/// The board body: title, search box, project tabs, three state columns.
fn board_panel(state: &State) -> Element<'_, Message> {
    let now = now_secs();
    let header = row![
        text("My Quests")
            .size(theme::SIZE_MD)
            .style(theme::fg)
            .font(bold()),
        Space::new().width(Fill),
        button(text("×").size(theme::SIZE_MD).style(theme::dim))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::QuestBoardToggled),
    ]
    .align_y(alignment::Vertical::Center);

    let search = text_input("搜索 Quest…", &state.quest.board_filter)
        .size(theme::SIZE_XS)
        .on_input(Message::QuestBoardFilterChanged)
        .padding([4, 8])
        .width(Fill);

    let mut tabs = row![].spacing(6);
    tabs = tabs.push(tab_entry(
        String::from("全部"),
        state.quest.board_project.is_none(),
        None,
    ));
    for cwd in projects(state) {
        let name = std::path::Path::new(&cwd)
            .file_name()
            .map_or_else(|| cwd.clone(), |name| name.to_string_lossy().into_owned());
        let selected = state.quest.board_project.as_deref() == Some(cwd.as_str());
        tabs = tabs.push(tab_entry(name, selected, Some(cwd)));
    }

    let visible = visible_threads(state);
    let running: Vec<_> = visible
        .iter()
        .copied()
        .filter(|thread| thread.status == QuestStatus::Running)
        .collect();
    let waiting: Vec<_> = visible
        .iter()
        .copied()
        .filter(|thread| thread.status == QuestStatus::Waiting)
        .collect();
    let finished: Vec<_> = visible
        .iter()
        .copied()
        .filter(|thread| !thread.status.is_active())
        .collect();

    let columns = if visible.is_empty() {
        container(
            text("没有匹配的 Quest")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        )
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
    } else {
        container(
            row![
                board_column("执行中", theme::INFO, &running, now),
                board_column("等待操作", theme::WARN, &waiting, now),
                board_column("已完成", theme::BRAND, &finished, now),
            ]
            .spacing(10)
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
    };

    column![header, container(search).style(filter_well), tabs, columns]
        .spacing(10)
        .into()
}

/// The threads the board shows: project tab plus search filter applied.
fn visible_threads(state: &State) -> Vec<&ThreadSummary> {
    let needle = state.quest.board_filter.trim().to_lowercase();
    state
        .sessions
        .threads()
        .iter()
        .filter(|thread| {
            let project_ok = state
                .quest
                .board_project
                .as_deref()
                .is_none_or(|cwd| thread.cwd == cwd);
            project_ok
                && (needle.is_empty()
                    || thread.label().to_lowercase().contains(&needle)
                    || thread.cwd.to_lowercase().contains(&needle))
        })
        .collect()
}

/// The distinct project directories, sorted for stable tabs.
fn projects(state: &State) -> Vec<String> {
    let set: BTreeSet<String> = state
        .sessions
        .threads()
        .iter()
        .map(|thread| thread.cwd.clone())
        .collect();
    set.into_iter().collect()
}

/// One project tab; `cwd` is `None` for the "all" tab.
fn tab_entry(label: String, selected: bool, cwd: Option<String>) -> Element<'static, Message> {
    button(
        text(label)
            .size(theme::SIZE_XS)
            .style(if selected { theme::fg } else { theme::dim }),
    )
    .padding([3, 10])
    .style(theme::rail_button(selected))
    .on_press(Message::QuestBoardProjectPicked(cwd))
    .into()
}

/// One state column: a colored dot, the counted title, and its cards.
fn board_column<'a>(
    title: &'static str,
    accent: iced::Color,
    threads: &[&'a ThreadSummary],
    now: i64,
) -> Element<'a, Message> {
    let mut list = column![].spacing(6);
    if threads.is_empty() {
        list = list.push(text("—").size(theme::SIZE_XS).style(theme::faint_text));
    }
    for thread in threads {
        list = list.push(card(thread, now));
    }

    container(
        column![
            row![
                status_dot(accent),
                text(format!("{title} ({})", threads.len()))
                    .size(theme::SIZE_XS)
                    .style(theme::fg)
                    .font(bold()),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
            scrollable(list).height(Fill),
        ]
        .spacing(8),
    )
    .width(Fill)
    .height(Fill)
    .padding(8)
    .style(column_surface)
    .into()
}

/// One quest card: name, project, age, and the error tag when failed.
fn card<'a>(thread: &'a ThreadSummary, now: i64) -> Element<'a, Message> {
    let project = std::path::Path::new(&thread.cwd).file_name().map_or_else(
        || String::from(thread.cwd.as_str()),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut title = row![
        text(String::from(thread.label()))
            .size(theme::SIZE_SM)
            .style(theme::fg)
    ];
    if thread.status == QuestStatus::Error {
        title = title.push(
            text(QuestStatus::Error.label())
                .size(theme::SIZE_XS)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(theme::DANGER),
                }),
        );
    }

    let body = column![
        title,
        row![
            text(project).size(theme::SIZE_XS).style(theme::faint_text),
            Space::new().width(Fill),
            text(chat::relative_time(thread.updated_at, now))
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        ]
        .spacing(4)
        .align_y(alignment::Vertical::Center),
    ]
    .spacing(4);

    button(body)
        .width(Fill)
        .padding([8, 10])
        .style(theme::card_button)
        .on_press(Message::SessionSelected(thread.id.clone()))
        .into()
}

/// An 8px state dot for the column headers.
fn status_dot(color: iced::Color) -> Element<'static, Message> {
    container(Space::new().width(8.0).height(8.0))
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// The board card surface: white, hairline border.
fn board_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_LG.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// The board column surface: pale fill behind the cards.
fn column_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::SIDEBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// The click-away mask behind the board; Esc closes it.
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

fn filter_well(_theme: &iced::Theme) -> iced::widget::container::Style {
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

fn bold() -> font::Font {
    font::Font {
        weight: font::Weight::Bold,
        ..font::Font::default()
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_secs() as i64)
}
