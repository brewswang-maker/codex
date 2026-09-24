//! The Quest launch page shown while the transcript is empty: a ring
//! mark, the "Quest on, hands off" headline, the run-on chip row (with
//! the workspace selector), the centered composer, and quest suggestions.
//!
//! Mirrors Qoder's new-quest surface, which replaces the generic welcome
//! with a launch pad instead.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::message::QuestScenario;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;

use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::stack;
use iced::widget::text;

/// Reading-width cap of the launch column, so the composer card and the
/// suggestions read as one centered unit.
const LAUNCH_WIDTH: f32 = 720.0;

/// The launch page: mark, headline, run-on chips, composer, suggestions.
pub fn page<'a>(state: &'a State, composer: Element<'a, Message>) -> Element<'a, Message> {
    let mut body = column![
        ring_mark(),
        text("Quest on, hands off")
            .size(theme::SIZE_TITLE)
            .style(theme::fg),
        run_on_row(state),
        composer,
    ]
    .spacing(16)
    .align_x(alignment::Horizontal::Center);

    if let Some(suggestions) = suggestion_list(state) {
        body = body.push(suggestions);
    }

    let surface: Element<'_, Message> =
        container(container(body).max_width(LAUNCH_WIDTH).width(Fill))
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .into();

    if state.quest.workspace_menu {
        stack![surface, workspace_menu(state)].into()
    } else {
        surface
    }
}

/// The translucent brand ring standing in for the launch artwork.
fn ring_mark() -> Element<'static, Message> {
    container(iced::widget::Space::new().width(44.0).height(44.0))
        .width(52.0)
        .height(52.0)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(|_| iced::widget::container::Style {
            border: iced::Border {
                color: iced::Color {
                    a: 0.30,
                    ..theme::BRAND
                },
                width: 6.0,
                radius: 26.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// The run-on row: the workspace chip (opens the selector), plus the
/// honest static chips for the execution target and the git branch.
fn run_on_row(state: &State) -> Element<'_, Message> {
    let workspace = state.tree.root_path().to_string_lossy().into_owned();
    let workspace_name = std::path::Path::new(&workspace).file_name().map_or_else(
        || workspace.clone(),
        |name| name.to_string_lossy().into_owned(),
    );

    let mut chips = row![text("运行于").size(theme::SIZE_SM).style(theme::dim)]
        .spacing(8)
        .align_y(alignment::Vertical::Center);

    chips = chips.push(
        button(
            row![
                Icon::new(IconKind::Folder, theme::MUTED, 13.0).widget(),
                text(workspace_name).size(theme::SIZE_SM).style(theme::fg),
                text("▾").size(theme::SIZE_XS).style(theme::faint_text),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
        )
        .padding([4, 10])
        .style(theme::ghost_button)
        .on_press(Message::QuestWorkspaceToggled),
    );

    chips = chips.push(
        container(
            row![
                Icon::new(IconKind::Remote, theme::MUTED, 13.0).widget(),
                text("本地模式").size(theme::SIZE_SM).style(theme::dim),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
        )
        .padding([4, 10])
        .style(theme::card),
    );

    if let Some(branch) = &state.git.branch {
        chips = chips.push(
            container(
                row![
                    Icon::new(IconKind::GitBranch, theme::MUTED, 13.0).widget(),
                    text(branch.clone()).size(theme::SIZE_SM).style(theme::dim),
                ]
                .spacing(6)
                .align_y(alignment::Vertical::Center),
            )
            .padding([4, 10])
            .style(theme::card),
        );
    }

    chips.into()
}

/// The suggestion rows under the composer: resumable recent quests when
/// any exist, otherwise one launcher per quest scenario.
fn suggestion_list(state: &State) -> Option<Element<'static, Message>> {
    let threads = state.sessions.threads();
    let (caption, rows) = if threads.is_empty() {
        let rows: Vec<Element<'static, Message>> = QuestScenario::ALL
            .iter()
            .map(|scenario| {
                suggestion_row(
                    String::from(scenario.title()),
                    String::from(scenario.hint()),
                    Message::QuestScenarioPicked(Some(*scenario)),
                )
            })
            .collect();
        (String::from("试试一个场景"), rows)
    } else {
        let rows: Vec<Element<'static, Message>> = threads
            .iter()
            .take(3)
            .map(|thread| {
                suggestion_row(
                    String::from(thread.label()),
                    String::from("点击继续这个 Quest"),
                    Message::SessionSelected(thread.id.clone()),
                )
            })
            .collect();
        (String::from("最近的 Quest"), rows)
    };

    if rows.is_empty() {
        return None;
    }

    let mut list = column![text(caption).size(theme::SIZE_XS).style(theme::dim)].spacing(2);
    for element in rows {
        list = list.push(element);
    }

    Some(
        container(list)
            .width(LAUNCH_WIDTH)
            .padding(iced::Padding::default().top(18))
            .into(),
    )
}

/// One suggestion row: a hollow ring, the title, and the quiet hint.
fn suggestion_row(title: String, hint: String, action: Message) -> Element<'static, Message> {
    button(
        row![
            hollow_ring(),
            column![
                text(title).size(theme::SIZE_SM).style(theme::fg),
                text(hint).size(theme::SIZE_XS).style(theme::faint_text),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([6, 10])
    .style(theme::ghost_button)
    .on_press(action)
    .into()
}

/// The 8px hollow circle that fronts a suggestion row.
fn hollow_ring() -> Element<'static, Message> {
    container(iced::widget::Space::new().width(8.0).height(8.0))
        .width(10.0)
        .height(10.0)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(|_| iced::widget::container::Style {
            border: iced::Border {
                color: theme::MUTED,
                width: 1.5,
                radius: 5.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// The workspace selector: recent projects over the open-folder entry.
/// A full-screen invisible mask button sits under the panel so a click
/// anywhere outside it closes the menu (Esc works too).
fn workspace_menu(state: &State) -> Element<'_, Message> {
    let mask: Element<'_, Message> = button(iced::widget::Space::new().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(invisible_button)
        .on_press(Message::QuestWorkspaceToggled)
        .into();

    let mut list = column![].spacing(2);
    for path in state.recents.entries().iter().take(5) {
        list = list.push(
            button(
                row![
                    Icon::new(IconKind::Folder, theme::MUTED, 13.0).widget(),
                    text(path.clone()).size(theme::SIZE_XS).style(theme::fg),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            )
            .width(Fill)
            .padding([6, 10])
            .style(theme::card_button)
            .on_press(Message::FolderPicked(Some(std::path::PathBuf::from(path)))),
        );
    }
    list = list.push(
        button(
            row![
                Icon::new(IconKind::Folder, theme::BRAND, 13.0).widget(),
                text("打开文件夹").size(theme::SIZE_XS).style(theme::fg),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center),
        )
        .width(Fill)
        .padding([6, 10])
        .style(theme::card_button)
        .on_press(Message::FolderPickRequested),
    );

    let panel = container(column![list].padding(6).width(Fill))
        .width(280.0)
        .style(menu_surface);

    // Anchored just below the chip row: the launch column is centered,
    // so the menu drops in under the run-on chips.
    let placed = container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Top)
        .padding(iced::Padding::default().top(335.0));

    stack![mask, placed].into()
}

/// A click-through-looking button: no fill, no border, no visual.
fn invisible_button(
    _theme: &iced::Theme,
    _status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: None,
        border: iced::Border::default(),
        ..iced::widget::button::Style::default()
    }
}

/// The dropdown card: white fill, hairline border, soft radius.
fn menu_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
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
