//! The remote-explorer pane: SSH targets from `~/.ssh/config` plus the
//! Dev Containers section, which reports an absent Docker honestly.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;

/// Renders the remote-explorer pane for the sidebar.
pub fn panel(state: &State) -> Element<'_, Message> {
    scrollable(
        column![ssh_section(state), dev_containers_section(state)]
            .width(Fill)
            .spacing(14)
            .padding([4, 2]),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

/// The `REMOTE` group listing the ssh-config host aliases.
fn ssh_section(state: &State) -> Element<'_, Message> {
    let mut entries = column![].spacing(2);
    if state.remote.targets.is_empty() {
        entries = entries.push(
            text("~/.ssh/config 中没有可显示的 SSH 目标。")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
    }
    for target in &state.remote.targets {
        entries = entries.push(
            row![
                Icon::new(IconKind::Remote, theme::MUTED, 14.0).widget(),
                text(target.clone()).size(theme::SIZE_XS).style(theme::fg),
            ]
            .spacing(8)
            .padding([3, 2]),
        );
    }

    column![section_header("Remote"), entries].spacing(6).into()
}

/// The Dev Containers group: requires a working Docker daemon, and says
/// so plainly when there is none (the VS Code wording).
fn dev_containers_section(state: &State) -> Element<'_, Message> {
    let unavailable = "Docker is not available on this host.";
    let docker_ready = state.remote.docker_available == Some(true);

    let mut entries = column![].spacing(4);
    if docker_ready {
        entries = entries.push(
            text("本机 Docker 已就绪，容器视图随后续版本提供。")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
    } else {
        for label in ["Running Containers", "Exited Containers"] {
            entries = entries.push(
                row![
                    text(label).size(theme::SIZE_XS).style(theme::dim),
                    text(format!("ⓘ {unavailable}"))
                        .size(theme::SIZE_XS)
                        .style(theme::faint_text),
                ]
                .spacing(6)
                .padding([2, 2]),
            );
        }
    }

    column![section_header("Dev Containers"), entries]
        .spacing(6)
        .into()
}

/// An uppercase group caption with the hairline under-spacing.
fn section_header(label: &'static str) -> Element<'static, Message> {
    container(
        text(label.to_uppercase())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    )
    .width(Fill)
    .padding([0, 2])
    .align_x(alignment::Horizontal::Left)
    .into()
}
