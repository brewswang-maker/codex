//! The approval dialog overlay (modal pattern from `iced/examples/modal`).
//!
//! Decisions are final on press: the mask is inert on purpose, an approval
//! must be answered explicitly, never dismissed by clicking around it.

use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::ApprovalKind;
use codex_gui_core::Decision;
use codex_gui_core::PendingApproval;
use iced::Color;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::center;
use iced::widget::column;
use iced::widget::container;
use iced::widget::mouse_area;
use iced::widget::opaque;
use iced::widget::row;
use iced::widget::stack;
use iced::widget::text;

/// Wraps the main UI with the approval dialog while one is pending.
///
/// The dialog queue is answered oldest first.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let Some(approval) = state.approvals.pending().first() else {
        return base.into();
    };

    stack![
        base.into(),
        opaque(mouse_area(center(opaque(dialog(approval))).style(
            |_theme| {
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
        )))
    ]
    .into()
}

fn dialog(approval: &PendingApproval) -> Element<'_, Message> {
    let request_id = approval.request_id.to_string();

    let headline = match &approval.kind {
        ApprovalKind::CommandExecution { .. } => "Run command?",
        ApprovalKind::FileChange { .. } => "Apply file change?",
        ApprovalKind::Permissions { .. } => "Grant permissions?",
    };

    let mut body = column![text(headline).size(theme::SIZE_BODY).style(theme::fg)]
        .spacing(10)
        .padding(16);

    body = match &approval.kind {
        ApprovalKind::CommandExecution { command, reason } => {
            body = body.push(detail(command.as_deref().unwrap_or("(stdin)")));
            if let Some(reason) = reason {
                body = body.push(detail(reason));
            }
            body
        }
        ApprovalKind::FileChange { reason } | ApprovalKind::Permissions { reason, .. } => {
            if let Some(reason) = reason {
                body = body.push(detail(reason));
            }
            body
        }
    };

    body = body.push(actions(&request_id, &approval.kind));

    container(body)
        .width(Fill)
        .max_width(480)
        .style(dialog_surface)
        .into()
}

/// The modal card: white surface, hairline border, large radius.
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

fn detail(text_line: &str) -> Element<'static, Message> {
    container(
        text(String::from(text_line))
            .size(theme::SIZE_SM)
            .style(muted),
    )
    .width(Fill)
    .padding([6, 8])
    .style(detail_well)
    .into()
}

/// The quoted detail well behind command/reason text.
fn detail_well(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    }
}

fn muted(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::MUTED),
    }
}

fn actions<'a>(request_id: &str, kind: &ApprovalKind) -> Element<'a, Message> {
    let decide = |decision| Message::ApprovalDecided {
        request_id: request_id.to_string(),
        decision,
    };

    // Permissions requests grant or deny the requested profile; the richer
    // command/file decisions add the session-wide approval option.
    let mut buttons = row![];
    if matches!(
        kind,
        ApprovalKind::CommandExecution { .. } | ApprovalKind::FileChange { .. }
    ) {
        buttons = buttons.push(
            button(
                text("Always (session)")
                    .size(theme::SIZE_SM)
                    .style(theme::dim),
            )
            .padding([6, 12])
            .style(quiet_button)
            .on_press(decide(Decision::AcceptForSession)),
        );
    }
    buttons = buttons
        .push(
            button(text("Deny").size(theme::SIZE_SM).style(deny_label))
                .padding([6, 12])
                .style(quiet_button)
                .on_press(decide(Decision::Decline)),
        )
        .push(
            button(
                container(text("Allow").size(theme::SIZE_SM).style(allow_label))
                    .align_x(alignment::Horizontal::Center)
                    .width(Fill),
            )
            .padding([6, 16])
            .style(theme::primary_button)
            .on_press(decide(Decision::Accept)),
        )
        .spacing(8);

    buttons.into()
}

/// The quiet secondary button: hairline chip, muted label.
fn quiet_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
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

fn deny_label(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::DANGER),
    }
}

fn allow_label(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::BG),
    }
}
