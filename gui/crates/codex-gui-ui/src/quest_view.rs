//! Quest mode: the quest list rail on the left and the task dialog on the
//! right.
//!
//! Mirrors the reference layout: a create button over per-project quest
//! groups and a utility footer on the rail, a task header over the
//! conversation on the right.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::quest_rows;
use crate::state::State;
use crate::theme;
use codex_gui_core::AccountBadge;
use codex_gui_core::QuestStatus;
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
use iced::widget::text;
use iced::widget::text_input;

/// Quest rail width, mirroring the reference proportions.
const QUEST_RAIL_WIDTH: f32 = 264.0;
/// How many quest rows the list shows before the show-more hint.
const QUEST_ROWS_SHOWN: usize = 8;

/// The quest rail: create button, quest filter, quest groups, utility
/// footer.
pub fn quest_sidebar(state: &State) -> Element<'_, Message> {
    let body = column![
        create_button(),
        filter_box(state),
        quest_list(state),
        footer(state),
    ]
    .width(Fill)
    .height(Fill)
    .padding(10)
    .spacing(8);

    container(body)
        .width(QUEST_RAIL_WIDTH)
        .height(Fill)
        .style(rail_surface)
        .into()
}

/// The rail's quest filter well (shares the sidebar filter state with the
/// editor's Threads pane).
fn filter_box(state: &State) -> Element<'_, Message> {
    let input = text_input("Filter quests…", &state.sidebar_filter)
        .size(theme::SIZE_XS)
        .on_input(Message::SidebarFilterChanged)
        .padding([4, 8])
        .width(Fill);
    container(input)
        .style(crate::sessions_view::filter_well)
        .into()
}

/// The task header strip: the live state, the active quest name, its
/// scenario and project chips, the board entry, and the way back to the
/// editor shell.
pub fn quest_header(state: &State) -> Element<'_, Message> {
    let active = state
        .thread_id
        .as_deref()
        .and_then(|id| state.sessions.threads().iter().find(|t| t.id == id));
    let title = active.map_or_else(
        || String::from("新会话"),
        |thread| String::from(thread.label()),
    );

    let mut header = row![text(title).size(theme::SIZE_MD).style(theme::fg),]
        .spacing(8)
        .align_y(alignment::Vertical::Center);

    if let Some(thread) = active
        && thread.status != QuestStatus::Idle
    {
        header = header.push(status_chip(thread.status));
    }
    if let Some(scenario) = state.quest.scenario {
        header = header.push(
            container(
                text(format!("场景 · {}", scenario.title()))
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            )
            .padding([2, 8])
            .style(chip_surface),
        );
    }
    if let Some(cwd) = state.status_board.cwd() {
        let name = std::path::Path::new(cwd).file_name().map_or_else(
            || String::from(cwd),
            |name| name.to_string_lossy().into_owned(),
        );
        header = header.push(
            container(text(name).size(theme::SIZE_XS).style(theme::dim))
                .padding([2, 8])
                .style(chip_surface),
        );
    }

    header = header
        .push(Space::new().width(Fill))
        .push(
            button(text("看板").size(theme::SIZE_SM).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::QuestBoardToggled),
        )
        .push(
            button(text("打开编辑器").size(theme::SIZE_SM).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::ModeToggled),
        );

    container(header)
        .width(Fill)
        .height(36.0)
        .padding([0, 12])
        .style(header_surface)
        .into()
}

/// The live-state chip in the header: a colored dot plus the state label.
fn status_chip(status: QuestStatus) -> Element<'static, Message> {
    let color = match status {
        QuestStatus::Idle => theme::BADGE,
        QuestStatus::Running => theme::INFO,
        QuestStatus::Waiting => theme::WARN,
        QuestStatus::Error => theme::DANGER,
    };
    container(
        row![
            container(Space::new().width(6.0).height(6.0))
                .width(6.0)
                .height(6.0)
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(color.into()),
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::container::Style::default()
                }),
            text(status.label()).size(theme::SIZE_XS).style(theme::dim),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding([2, 8])
    .style(chip_surface)
    .into()
}

/// The bordered create button at the rail's top; raises the scenario
/// picker instead of starting blindly.
fn create_button() -> Element<'static, Message> {
    button(
        row![
            Icon::new(IconKind::Plus, theme::TEXT, 14.0).widget(),
            text("创建 Quest").size(theme::SIZE_MD).style(theme::fg),
            Space::new().width(Fill),
            text("Ctrl N").size(theme::SIZE_XS).style(theme::faint_text),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([8, 10])
    .style(create_surface)
    .on_press(Message::QuestCreatePressed)
    .into()
}

/// The grouped quest rows, capped with a show-more hint.
fn quest_list(state: &State) -> Element<'_, Message> {
    let now = now_secs();
    let mut list = column![].spacing(2);
    list = list.push(section_label("Quests"));

    if state.sessions.threads().is_empty() {
        list = list.push(
            text("还没有 Quest — Ctrl+N 开始第一个任务")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
        return list.into();
    }

    let (pinned, groups) = codex_gui_core::organized_sidebar(
        state.sessions.threads(),
        state.pins.id_set(),
        &state.sidebar_filter,
    );

    let total = pinned.len() + groups.values().map(Vec::len).sum::<usize>();
    // Expanded shows everything; otherwise the rail caps at one screenful.
    let cap = if state.quest.list_expanded {
        usize::MAX
    } else {
        QUEST_ROWS_SHOWN
    };
    let mut shown = 0usize;

    if !pinned.is_empty() {
        list = list.push(group_header("已固定"));
        for thread in &pinned {
            if shown >= cap {
                break;
            }
            list = list.push(quest_rows::quest_row(state, thread, now));
            shown += 1;
        }
    }
    for (cwd, threads) in &groups {
        if threads.is_empty() || shown >= cap {
            break;
        }
        list = list.push(group_header(cwd));
        for thread in threads {
            if shown >= cap {
                break;
            }
            list = list.push(quest_rows::quest_row(state, thread, now));
            shown += 1;
        }
    }

    if total == 0 && !state.sidebar_filter.trim().is_empty() {
        list = list.push(
            text("没有匹配的 Quest")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
    }

    if total > QUEST_ROWS_SHOWN {
        let label = if state.quest.list_expanded {
            "收起"
        } else {
            "…  显示更多"
        };
        list = list.push(
            button(text(label).size(theme::SIZE_XS).style(theme::dim))
                .padding([4, 2])
                .style(theme::ghost_button)
                .on_press(Message::QuestListExpanded),
        );
    }

    scrollable(list).height(Fill).into()
}

/// A project group caption with the folder glyph.
fn group_header(cwd: &str) -> Element<'static, Message> {
    let name = std::path::Path::new(cwd).file_name().map_or_else(
        || String::from(cwd),
        |name| name.to_string_lossy().into_owned(),
    );
    row![
        Icon::new(IconKind::Folder, theme::MUTED, 14.0).widget(),
        text(name)
            .size(theme::SIZE_SM)
            .style(theme::fg)
            .font(bold()),
    ]
    .spacing(6)
    .align_y(alignment::Vertical::Center)
    .padding(iced::Padding::default().top(6).bottom(2))
    .into()
}

/// A small section caption.
fn section_label(label: &'static str) -> Element<'static, Message> {
    text(label).size(theme::SIZE_XS).style(theme::dim).into()
}

/// The rail footer: utility entries over the account block.
fn footer(state: &State) -> Element<'_, Message> {
    let utilities = column![
        utility_row(IconKind::Clock, "定时任务", None),
        utility_row(IconKind::Gear, "Better Harness", Some("Beta")),
        utility_row(IconKind::Wiki, "知识中心", None),
        utility_row(IconKind::Extensions, "插件市场", None),
    ]
    .spacing(2);

    column![utilities, divider(), account_row(state)]
        .spacing(6)
        .into()
}

/// One footer utility entry; inert until the corresponding surface lands.
fn utility_row(
    icon: IconKind,
    label: &'static str,
    tag: Option<&'static str>,
) -> Element<'static, Message> {
    let mut content = row![
        Icon::new(icon, theme::MUTED, 15.0).widget(),
        text(label).size(theme::SIZE_SM).style(theme::dim),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center);
    if let Some(tag) = tag {
        content = content.push(text(tag).size(theme::SIZE_XS).style(|_theme| {
            iced::widget::text::Style {
                color: Some(theme::BRAND),
            }
        }));
    }
    button(content)
        .width(Fill)
        .padding([4, 2])
        .style(theme::ghost_button)
        .on_press(Message::Noop)
        .into()
}

/// The account block: avatar, address, plan tag, settings gear.
fn account_row(state: &State) -> Element<'_, Message> {
    let (email, plan) = match state.status_board.account() {
        AccountBadge::Chatgpt { email, plan } => (
            email.clone().unwrap_or_else(|| String::from("chatgpt")),
            plan.clone(),
        ),
        AccountBadge::ApiKey => (String::from("API key"), String::from("api")),
        AccountBadge::SignedOut => (String::from("未登录"), String::from("免费版")),
        AccountBadge::Unknown => (String::from("…"), String::from("")),
    };
    let initial = email
        .chars()
        .next()
        .map_or_else(|| String::from("?"), |c| c.to_uppercase().to_string());

    row![
        container(text(initial).size(theme::SIZE_SM).style(|_theme| {
            iced::widget::text::Style {
                color: Some(theme::BG),
            }
        }))
        .width(24.0)
        .height(24.0)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(avatar_surface),
        column![
            text(email).size(theme::SIZE_SM).style(theme::fg),
            text(plan).size(theme::SIZE_XS).style(theme::faint_text),
        ]
        .spacing(0),
        Space::new().width(Fill),
        button(Icon::new(IconKind::Gear, theme::MUTED, 15.0).widget())
            .padding(2)
            .style(theme::ghost_button)
            .on_press(Message::SettingsToggled),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The rounded avatar chip behind the account initial.
fn avatar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BRAND.into()),
        border: iced::Border {
            radius: 12.0.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The create button surface: white fill with a hairline border.
fn create_surface(
    _theme: &iced::Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let background = match status {
        iced::widget::button::Status::Hovered => theme::CARD,
        _ => theme::BG,
    };
    iced::widget::button::Style {
        background: Some(background.into()),
        text_color: theme::TEXT,
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::button::Style::default()
    }
}

/// The project chip behind the header's directory name.
fn chip_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The header strip surface with its hairline base.
fn header_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::TOPBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The rail surface with its hairline divider from the task dialog.
fn rail_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
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

/// A one-pixel rule above the account block.
fn divider() -> Element<'static, Message> {
    container(Space::new().height(1.0))
        .width(Fill)
        .style(|_theme| iced::widget::container::Style {
            background: Some(theme::BORDER.into()),
            ..iced::widget::container::Style::default()
        })
        .into()
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
