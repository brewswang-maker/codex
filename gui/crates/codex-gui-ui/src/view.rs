//! The main window layout: top bar, activity rail, sidebar, transcript,
//! composer, status bar, approval overlay.

use crate::activity_bar;
use crate::approvals_view;
use crate::chat;
use crate::command_palette;
use crate::diff_view;
use crate::editor_view;
use crate::git_view;
use crate::icons::Icon;
use crate::icons::IconKind;
use crate::menu_bar;
use crate::message::AppMode;
use crate::message::Message;
use crate::model_menu;
use crate::plan_view;
use crate::quest_board;
use crate::quest_launch;
use crate::quest_overlays;
use crate::quest_view;
use crate::requests_view;
use crate::sessions_view;
use crate::settings_view;
use crate::skills_view;
use crate::state::State;
use crate::state::Status;
use crate::theme;
use codex_gui_core::AccountBadge;
use codex_gui_core::ErrorBanner;
use codex_gui_core::localize_skill_description;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::image;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;
use std::path::PathBuf;

#[cfg(test)]
#[path = "simulator_tests.rs"]
mod simulator_tests;

/// Renders the whole window.
pub fn view(state: &State) -> Element<'_, Message> {
    crate::perf::frame_tick();

    if let Status::Disconnected(reason) = &state.status {
        // Full-screen dead-end: the backend is gone and nothing else is
        // interactive until the user relaunches it.
        return disconnected_screen(reason);
    }
    if state.settings.open {
        // The settings panel replaces the whole chat surface while open.
        return settings_view::panel(state);
    }
    if state.skills.page_open {
        // The Skills page likewise owns the whole surface while open.
        return skills_view::page(state);
    }

    let main = conversation(state);
    let window = match state.mode {
        AppMode::Editor => {
            // Qoder's editor shell: rail, pane, file preview, then the
            // chat panel pinned to the right edge.
            let body = row![
                activity_bar::activity_bar(state),
                sessions_view::sidebar(state),
                editor_view::editor_area(state),
                container(main).width(CHAT_PANEL_WIDTH),
            ]
            .height(Fill);
            column![
                menu_bar::menu_bar(state),
                top_bar(state),
                error_strip(state),
                body,
                status_bar(state),
            ]
        }
        AppMode::Quest => {
            // A fresh quest shows the launch page (mark, run-on chips,
            // centered composer) instead of the transcript scaffold; the
            // skills picker keeps the ordinary layout it anchors to.
            let quest_empty = state.transcript.entries().is_empty()
                && state.transcript.plan().is_none()
                && !state.skills.picker_open;
            let body = if quest_empty {
                row![
                    quest_view::quest_sidebar(state),
                    quest_launch::page(state, composer(state)),
                ]
                .height(Fill)
            } else {
                row![quest_view::quest_sidebar(state), main].height(Fill)
            };
            column![
                quest_view::quest_header(state),
                error_strip(state),
                body,
                status_bar(state),
            ]
        }
    };

    // Overlays stack from the palette upward: the Quest board and dialogs,
    // git composer, plan review, diff viewer, request dialogs, and finally
    // the always-on-top approval modal.
    approvals_view::layered(
        state,
        requests_view::layered(
            state,
            diff_view::layered(
                state,
                plan_view::layered(
                    state,
                    git_view::layered(
                        state,
                        quest_board::layered(
                            state,
                            quest_overlays::layered(
                                state,
                                command_palette::layered(
                                    state,
                                    model_menu::layered(state, menu_bar::layered(state, window)),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    )
}

/// The shared conversation column: transcript, slots, composer.
///
/// The optional bars keep a constant slot even while hidden: iced pairs
/// widget state by child position, so a changing child count would shift
/// the composer to a fresh index mid-typing, rebuilding its `text_input`
/// state and silently dropping the keyboard focus.
fn conversation(state: &State) -> Element<'_, Message> {
    column![
        chat::transcript_view(state),
        slot(skill_picker(state)),
        slot(mention_picker(state)),
        slot(slash_picker(state)),
        slot(chat::taskbar(state)),
        composer(state),
    ]
    .height(Fill)
    .into()
}

/// Wraps one optional bar above the composer so the child count never
/// changes; a hidden bar collapses to a zero-height container.
fn slot(bar: Option<Element<'_, Message>>) -> Element<'_, Message> {
    let content = bar.unwrap_or_else(|| iced::widget::Space::new().height(0.0).into());
    container(content).width(Fill).into()
}

/// The full-screen disconnect state with the one-click relaunch.
fn disconnected_screen(reason: &str) -> Element<'_, Message> {
    container(
        column![
            text("Connection lost").size(24).style(theme::fg),
            text(reason).style(theme::dim),
            button(text("Restart app-server")).on_press(Message::Reconnect),
        ]
        .spacing(16)
        .align_x(alignment::Horizontal::Center),
    )
    .width(Fill)
    .height(Fill)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The banner strip below the top bar while an error is active.
fn error_strip(state: &State) -> Element<'_, Message> {
    match state.status_board.error() {
        Some(banner) => error_banner(banner),
        None => column![].into(),
    }
}

/// A dismissible strip for `error` notifications.
fn error_banner(banner: &ErrorBanner) -> Element<'_, Message> {
    let retry_hint = if banner.will_retry {
        " (retrying…)"
    } else {
        ""
    };
    let label: Element<'_, Message> = text(format!("{}{retry_hint}", banner.message))
        .size(theme::SIZE_SM)
        .style(warn)
        .into();

    container(
        row![
            label,
            iced::widget::Space::new().width(Fill),
            button(text("dismiss").size(theme::SIZE_XS))
                .style(theme::ghost_button)
                .on_press(Message::ErrorDismissed),
        ]
        .spacing(8),
    )
    .width(Fill)
    .padding([4, 12])
    .style(theme::surface(theme::CARD))
    .into()
}

/// The 36px top bar: wordmark, working-directory breadcrumb, and the
/// window-level actions.
fn top_bar(state: &State) -> Element<'_, Message> {
    let cwd = state.status_board.cwd().unwrap_or("no project");

    // The green way into Quest mode, mirroring the reference title bar.
    let quest = button(text("打开 Quest").size(theme::SIZE_SM))
        .padding([4, 12])
        .style(theme::primary_button)
        .on_press(Message::ModeToggled);

    container(
        row![
            text("Codex")
                .size(theme::SIZE_MD)
                .style(|_| iced::widget::text::Style {
                    color: Some(theme::GOLD),
                }),
            text(cwd).size(theme::SIZE_SM).style(theme::dim),
            iced::widget::Space::new().width(Fill),
            quest,
            icon_button(IconKind::Folder, Message::FolderPickRequested),
            icon_button(IconKind::Gear, Message::SettingsToggled),
        ]
        .spacing(12)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .height(36.0)
    .padding([0, 12])
    .style(|_theme| iced::widget::container::Style {
        background: Some(theme::TOPBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    })
    .into()
}

/// A borderless icon button for the top bar.
fn icon_button(kind: IconKind, on_press: Message) -> Element<'static, Message> {
    button(
        container(Icon::new(kind, theme::MUTED, 18.0).widget())
            .align_x(alignment::Horizontal::Center)
            .width(Fill),
    )
    .padding(4)
    .width(28.0)
    .style(theme::ghost_button)
    .on_press(on_press)
    .into()
}

/// The 24px bottom status bar: lifecycle dot and context on the left,
/// model/account/thread on the right.
fn status_bar(state: &State) -> Element<'_, Message> {
    let (dot, status): (iced::Color, &str) = match &state.status {
        Status::Bootstrapping => (theme::BADGE, "connecting…"),
        Status::Ready => (theme::GLOW, "ready"),
        Status::Thinking => (theme::GOLD, "thinking…"),
        Status::Disconnected(reason) => (theme::DANGER, reason),
    };

    let thread = state.thread_id.as_deref().unwrap_or("no thread");
    let model = state.status_board.current_model().unwrap_or("no model");
    let account = account_label(state.status_board.account());
    let cwd = state.status_board.cwd().unwrap_or("no cwd");

    container(
        row![
            status_dot(dot),
            text(status).size(theme::SIZE_SM).style(theme::dim),
            git_branch(state),
            text(cwd).size(theme::SIZE_SM).style(theme::dim),
            iced::widget::Space::new().width(Fill),
            text(model).size(theme::SIZE_SM).style(theme::dim),
            text(account).size(theme::SIZE_SM).style(theme::dim),
            text(thread).size(theme::SIZE_SM).style(theme::dim),
        ]
        .spacing(10)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .height(24.0)
    .padding([0, 12])
    .style(theme::surface(theme::PANEL))
    .into()
}

/// The branch chip shown on the status bar while a repo is detected.
fn git_branch(state: &State) -> Element<'_, Message> {
    let Some(branch) = &state.git.branch else {
        return column![].into();
    };
    let changes = state.git.change_count();
    let label = if changes > 0 {
        format!("{branch} ±{changes}")
    } else {
        branch.clone()
    };
    row![
        Icon::new(IconKind::GitBranch, theme::MUTED, 13.0).widget(),
        text(label).size(theme::SIZE_SM).style(theme::dim),
    ]
    .spacing(4)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// An 8px lifecycle dot.
fn status_dot(color: iced::Color) -> Element<'static, Message> {
    container(iced::widget::Space::new().width(0.0).height(0.0))
        .width(8.0)
        .height(8.0)
        .style(move |_| iced::widget::container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// One-line login badge for the status bar.
fn account_label(badge: &AccountBadge) -> String {
    match badge {
        AccountBadge::Unknown => "…".to_string(),
        AccountBadge::SignedOut => "signed out".to_string(),
        AccountBadge::ApiKey => "api key".to_string(),
        AccountBadge::Chatgpt { email, plan } => match email {
            Some(email) => format!("{email} ({plan})"),
            None => format!("chatgpt ({plan})"),
        },
    }
}

/// The inline picker between transcript and composer while open: one row
/// per enabled skill with its Chinese one-liner, inserting its `$name`
/// mention.
fn skill_picker(state: &State) -> Option<Element<'_, Message>> {
    if !state.skills.picker_open {
        return None;
    }

    let enabled: Vec<_> = state.skills.rows.iter().filter(|row| row.enabled).collect();
    if enabled.is_empty() {
        return Some(
            container(text("没有已启用的 Skill").style(theme::dim))
                .padding(8)
                .width(Fill)
                .into(),
        );
    }

    let mut options = column![].spacing(4);
    for skill in enabled {
        let description = localize_skill_description(&skill.name, &skill.description);
        let mut label = column![
            text(format!("${}", skill.name))
                .size(theme::SIZE_XS)
                .style(theme::fg)
        ]
        .spacing(2)
        .width(Fill);
        if !description.is_empty() {
            label = label.push(
                text(description)
                    .size(theme::SIZE_XS)
                    .style(theme::dim)
                    .width(Fill),
            );
        }
        options = options.push(
            button(label)
                .width(Fill)
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::SkillPicked(skill.name.clone())),
        );
    }
    Some(scrollable(options).height(240).width(Fill).into())
}

/// The `@` mention completion list between transcript and composer: one
/// row per fuzzy match, the highlighted row on a panel surface.
fn mention_picker(state: &State) -> Option<Element<'_, Message>> {
    if !state.mentions.is_open() {
        return None;
    }
    if state.mentions.hits().is_empty() {
        return Some(
            container(text("没有匹配的文件").style(theme::dim))
                .padding(8)
                .width(Fill)
                .into(),
        );
    }

    let mut options = column![].spacing(4);
    for (index, hit) in state.mentions.hits().iter().enumerate() {
        let selected = index == state.mentions.selected();
        let label = row![
            text(hit.file_name.clone())
                .size(theme::SIZE_XS)
                .style(theme::fg),
            text(hit.path.clone())
                .size(theme::SIZE_XS)
                .style(theme::dim),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .width(Fill);
        options = options.push(
            button(label)
                .width(Fill)
                .padding([4, 10])
                .style(move |theme, status| {
                    if selected {
                        button::Style {
                            background: Some(theme::PANEL.into()),
                            text_color: theme::TEXT,
                            ..theme::ghost_button(theme, status)
                        }
                    } else {
                        theme::ghost_button(theme, status)
                    }
                })
                .on_press(Message::MentionPicked(index)),
        );
    }
    Some(scrollable(options).height(240).width(Fill).into())
}

/// The `/` command palette between transcript and composer while the
/// composer holds a bare `/word`.
fn slash_picker(state: &State) -> Option<Element<'_, Message>> {
    let options = crate::commands::slash_options(&state.composer);
    if options.is_empty() {
        return None;
    }

    let mut list = column![].spacing(4);
    for (command, hint) in options {
        let label = row![
            text(command).size(theme::SIZE_XS).style(theme::fg),
            text(hint).size(theme::SIZE_XS).style(theme::dim),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .width(Fill);
        list = list.push(
            button(label)
                .width(Fill)
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::SlashPicked(String::from(command))),
        );
    }
    Some(container(list).width(Fill).into())
}

fn composer(state: &State) -> Element<'_, Message> {
    let can_submit = state.can_submit();

    let input = text_input("Ask Codex…（@ 提及文件，/ 命令）", &state.composer)
        .id(COMPOSER_INPUT_ID)
        .on_input(Message::ComposerChanged)
        .on_submit_maybe(can_submit.then_some(Message::Submit))
        .size(theme::SIZE_BODY)
        .padding(4)
        .style(theme::bare_input)
        .width(Fill);

    let attach = button(
        row![
            Icon::new(IconKind::Paperclip, theme::MUTED, 13.0).widget(),
            text("attach").size(theme::SIZE_XS).style(theme::fg),
        ]
        .spacing(4)
        .align_y(alignment::Vertical::Center),
    )
    .padding([4, 8])
    .style(theme::ghost_button)
    .on_press(Message::AttachImages);
    let skills = button(text("$ skills").size(theme::SIZE_XS).style(theme::fg))
        .padding([4, 8])
        .style(theme::ghost_button)
        .on_press(Message::SkillsPickerToggled);

    // While a turn streams the primary action becomes a stop: it asks
    // the server to interrupt the live turn.
    let interrupting = matches!(state.status, Status::Thinking) && state.active_turn.is_some();
    let send = |enabled: bool| {
        if interrupting {
            button(
                container(Icon::new(IconKind::Stop, theme::BG, 16.0).widget())
                    .align_x(alignment::Horizontal::Center),
            )
            .padding([6, 10])
            .style(stop_button)
            .on_press(Message::TurnInterruptRequested)
        } else {
            button(
                container(Icon::new(IconKind::SendUp, theme::BG, 16.0).widget())
                    .align_x(alignment::Horizontal::Center),
            )
            .padding([6, 10])
            .style(theme::primary_button)
            .on_press_maybe(enabled.then_some(Message::Submit))
        }
    };

    let mut card = column![input];
    if state.attachments.hovered {
        card = card.push(drop_hint());
    }
    if !state.attachments.images.is_empty() {
        card = card.push(pending_chips(&state.attachments.images));
    }
    card = card.push(
        row![
            skills,
            attach,
            iced::widget::Space::new().width(Fill),
            model_menu_button(state),
            send(can_submit),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    );

    container(card.spacing(6).padding(10))
        .width(Fill)
        .max_width(CONTENT_WIDTH)
        .style(theme::card)
        .into()
}

/// Reading-width cap shared with the transcript column.
const CONTENT_WIDTH: f32 = 760.0;

/// Widget id of the composer text input. Pickers hand the keyboard back
/// through it: iced blurs a text input whenever a press lands outside its
/// bounds, so a row click would otherwise swallow everything typed next.
pub const COMPOSER_INPUT_ID: &str = "composer-input";

/// The stop button while a turn streams: the danger-tinted primary action.
fn stop_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(theme::DANGER.into()),
        text_color: theme::BG,
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}

/// The chat panel's fixed width; the file preview takes the rest.
const CHAT_PANEL_WIDTH: f32 = 420.0;

/// The composer's model chip: shows the effective model and opens the
/// multi-vendor menu.
fn model_menu_button(state: &State) -> Element<'_, Message> {
    let name = state
        .status_board
        .current_model()
        .unwrap_or("model")
        .to_string();
    button(
        text(format!("{name} ▾"))
            .size(theme::SIZE_SM)
            .style(theme::dim),
    )
    .padding([4, 8])
    .style(theme::ghost_button)
    .on_press(Message::ModelMenuToggled)
    .into()
}

/// The hint shown while a file drag hovers over the window.
fn drop_hint() -> Element<'static, Message> {
    container(
        text("Drop images to attach")
            .size(theme::SIZE_XS)
            .style(theme::dim),
    )
    .width(Fill)
    .padding([4, 12])
    .into()
}

/// One dismissible thumbnail chip per pending image attachment.
fn pending_chips(images: &[PathBuf]) -> Element<'_, Message> {
    let mut chips = row![].spacing(8).padding([4, 12]);
    for (index, path) in images.iter().enumerate() {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        chips = chips.push(
            row![
                image(image::Handle::from_path(path)).height(28),
                text(name).size(theme::SIZE_XS).style(theme::fg),
                button(text("×").size(theme::SIZE_XS).style(theme::fg))
                    .style(theme::ghost_button)
                    .on_press(Message::AttachmentRemoved(index)),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center)
            .padding([2, 4]),
        );
    }
    chips.into()
}

fn warn(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::DANGER),
    }
}
