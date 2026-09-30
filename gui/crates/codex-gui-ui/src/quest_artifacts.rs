//! The Quest artifact panel: the task dialog's right-hand column.
//!
//! Mirrors the reference's output pane: the live plan ("Spec") and the
//! files the turn changed sit in one switchable column beside the
//! conversation, so inspecting either no longer needs a raised overlay.

use crate::diff_view;
use crate::message::ArtifactTab;
use crate::message::Message;
use crate::plan_view::plan_text;
use crate::state::State;
use crate::theme;
use codex_gui_core::FileChangeRecord;
use codex_gui_core::PlanStep;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;

/// Panel width, in the reference's output-column proportions.
const PANEL_WIDTH: f32 = 300.0;

/// The panel: the tab strip over the picked tab's body.
pub fn panel(state: &State) -> Element<'_, Message> {
    let body = match state.artifacts.tab {
        ArtifactTab::Spec => spec_tab(state),
        ArtifactTab::Changes => changes_tab(state),
    };

    container(
        column![header(state), scrollable(body).height(Fill)]
            .spacing(8)
            .padding(10)
            .height(Fill),
    )
    .width(PANEL_WIDTH)
    .height(Fill)
    .style(panel_surface)
    .into()
}

/// The tab strip: one button per tab plus the close control.
fn header(state: &State) -> Element<'_, Message> {
    row![
        tab_button(ArtifactTab::Spec, state.artifacts.tab),
        tab_button(ArtifactTab::Changes, state.artifacts.tab),
        Space::new().width(Fill),
        button(text("×").size(theme::SIZE_MD).style(theme::dim))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::QuestArtifactsToggled),
    ]
    .spacing(4)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// One tab chip; the picked tab sits on the rail's active surface.
fn tab_button(tab: ArtifactTab, current: ArtifactTab) -> Element<'static, Message> {
    let selected = tab == current;
    button(text(tab.title()).size(theme::SIZE_SM).style(if selected {
        theme::fg
    } else {
        theme::dim
    }))
    .padding([3, 10])
    .style(theme::rail_button(selected))
    .on_press(Message::QuestArtifactTabSelected(tab))
    .into()
}

/// The Spec tab: the live plan with its copy and overlay actions.
fn spec_tab(state: &State) -> Element<'_, Message> {
    let Some(plan) = state.transcript.plan() else {
        return empty_hint("暂无计划 — 任务开始后会在此实时更新");
    };

    let mut body = column![].spacing(6);
    if let Some(explanation) = &plan.explanation {
        body = body.push(
            text(explanation.clone())
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    }

    let mut steps = column![].spacing(4);
    for step in &plan.steps {
        steps = steps.push(step_row(step));
    }
    body = body.push(steps);
    body = body.push(
        row![
            button(text("复制").size(theme::SIZE_XS).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::CopyMessage {
                    key: String::from("quest-spec"),
                    text: plan_text(plan),
                }),
            button(text("在弹层查看").size(theme::SIZE_XS).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::PlanOverlayToggled),
        ]
        .spacing(6),
    );
    body.into()
}

/// One plan step row: the status glyph plus the step text.
fn step_row(step: &PlanStep) -> Element<'static, Message> {
    let (glyph, color) = match step.status.as_str() {
        "completed" => ("✓", theme::BRAND),
        "in progress" => ("●", theme::INFO),
        _ => ("○", theme::FAINT),
    };
    row![
        text(glyph)
            .size(theme::SIZE_XS)
            .style(move |_| iced::widget::text::Style { color: Some(color) }),
        text(step.step.clone())
            .size(theme::SIZE_XS)
            .style(theme::fg),
    ]
    .spacing(6)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The Changes tab: one row per changed file in arrival order.
fn changes_tab(state: &State) -> Element<'_, Message> {
    let changes = diff_view::file_changes(state);
    if changes.is_empty() {
        return empty_hint("暂无文件变更 — 修改会随任务进行列出");
    }

    let mut list = column![].spacing(2);
    for (index, change) in changes.iter().enumerate() {
        list = list.push(change_row(index, change));
    }
    list.into()
}

/// One changed-file row: the kind tag plus the path; clicking opens the
/// diff overlay on that file.
fn change_row(index: usize, change: &FileChangeRecord) -> Element<'static, Message> {
    let (tag, color) = match change.kind.as_str() {
        "add" => ("A", theme::BRAND),
        "delete" => ("D", theme::DANGER),
        _ => ("U", theme::WARN),
    };
    button(
        row![
            text(tag)
                .size(theme::SIZE_XS)
                .style(move |_| iced::widget::text::Style { color: Some(color) }),
            text(change.path.clone())
                .size(theme::SIZE_XS)
                .style(theme::fg),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([4, 6])
    .style(theme::ghost_button)
    .on_press(Message::QuestArtifactFileOpened(index))
    .into()
}

/// The shared empty-state hint.
fn empty_hint(message: &'static str) -> Element<'static, Message> {
    container(text(message).size(theme::SIZE_XS).style(theme::dim))
        .padding(6)
        .into()
}

/// The panel surface: sidebar tone with the rail's hairline divider.
fn panel_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
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
