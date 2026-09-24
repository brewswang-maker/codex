//! The plan review overlay: the live plan in one place with copy/quote
//! actions.
//!
//! Honest adaptation of AgentCodeGUI's plan-review card: the Codex
//! app-server has no plan-approval request, so this is a viewer that
//! feeds the composer, not an approver.

use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::PlanStep;
use codex_gui_core::TurnPlan;
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
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;

/// Overlay dimensions, in the approval-dialog size class.
const OVERLAY_WIDTH: f32 = 560.0;
const OVERLAY_HEIGHT: f32 = 480.0;

/// Wraps the main UI with the plan overlay while it is open; the mask
/// dismisses, the actions are the point.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.plan_overlay_open {
        return base.into();
    }

    stack![
        base.into(),
        mouse_area(opaque(
            container(iced::widget::Space::new().width(Fill).height(Fill)).style(mask),
        ))
        .on_press(Message::PlanOverlayToggled),
        center(opaque(overlay(state))),
    ]
    .into()
}

fn overlay(state: &State) -> Element<'_, Message> {
    let Some(plan) = state.transcript.plan() else {
        // The plan finished while the overlay was up.
        return container(
            column![
                text("Plan").size(theme::SIZE_BODY).style(theme::fg),
                text("No plan is running right now.")
                    .size(theme::SIZE_SM)
                    .style(theme::dim),
            ]
            .spacing(8),
        )
        .width(Fill)
        .max_width(OVERLAY_WIDTH)
        .padding(16)
        .style(dialog_surface)
        .into();
    };

    let mut steps = column![].spacing(4);
    for step in &plan.steps {
        steps = steps.push(step_row(step));
    }

    let mut body = column![text("Plan").size(theme::SIZE_BODY).style(theme::fg)];
    if let Some(explanation) = &plan.explanation {
        body = body.push(
            text(explanation.clone())
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    }
    body = body.push(steps);
    body = body.push(actions(plan));

    container(scrollable(body.spacing(10).padding([0, 4])).height(Fill))
        .width(Fill)
        .max_width(OVERLAY_WIDTH)
        .height(Fill)
        .max_height(OVERLAY_HEIGHT)
        .padding(16)
        .style(dialog_surface)
        .into()
}

fn actions(plan: &TurnPlan) -> Element<'static, Message> {
    let payload = plan_text(plan);
    row![
        button(text("Copy").size(theme::SIZE_SM).style(theme::fg))
            .padding([4, 10])
            .style(quiet_action)
            .on_press(Message::CopyMessage {
                id: String::from("plan"),
                text: payload,
            }),
        button(
            text("Quote into composer")
                .size(theme::SIZE_SM)
                .style(theme::fg),
        )
        .padding([4, 10])
        .style(quiet_action)
        .on_press(Message::PlanQuoted),
        iced::widget::Space::new().width(Fill),
        button(text("Close").size(theme::SIZE_SM).style(theme::dim))
            .padding([4, 10])
            .style(quiet_action)
            .on_press(Message::PlanOverlayToggled),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center)
    .into()
}

fn step_row(step: &PlanStep) -> Element<'static, Message> {
    let color = match step.status.as_str() {
        "in progress" => theme::GOLD,
        "completed" => theme::BRAND,
        _ => theme::FAINT,
    };
    row![
        text("*")
            .size(theme::SIZE_SM)
            .style(move |_| iced::widget::text::Style { color: Some(color) }),
        text(step.step.clone())
            .size(theme::SIZE_SM)
            .style(theme::fg),
        iced::widget::Space::new().width(Fill),
        text(step.status.clone())
            .size(theme::SIZE_XS)
            .style(move |_| iced::widget::text::Style { color: Some(color) }),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The plan as one copyable/quotable text block; shared with the
/// update loop's quote action.
pub fn plan_text(plan: &TurnPlan) -> String {
    let mut lines = Vec::new();
    if let Some(explanation) = &plan.explanation {
        lines.push(explanation.clone());
    }
    for step in &plan.steps {
        lines.push(format!("- {} [{}]", step.step, step.status));
    }
    lines.join("\n")
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

#[cfg(test)]
mod tests {
    use super::plan_text;
    use codex_gui_core::PlanStep;
    use codex_gui_core::TurnPlan;

    #[test]
    fn plan_text_joins_explanation_and_steps() {
        let plan = TurnPlan {
            explanation: Some("shipping the fix".to_string()),
            steps: vec![
                PlanStep {
                    step: "reproduce".to_string(),
                    status: "completed".to_string(),
                },
                PlanStep {
                    step: "patch".to_string(),
                    status: "in progress".to_string(),
                },
            ],
        };

        assert_eq!(
            plan_text(&plan),
            "shipping the fix\n- reproduce [completed]\n- patch [in progress]"
        );
    }
}
