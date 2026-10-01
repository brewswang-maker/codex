//! The full-screen Better Harness panel: managed lifecycles for
//! external coding-agent runs — start/stop, the plan → implement →
//! review → repair stage loop, the captured log, the recorded review,
//! and the derived repair checklist.

use crate::boards_shell;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::HarnessSession;
use codex_gui_core::HarnessStatus;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the full-screen Better Harness panel.
pub fn panel(state: &State) -> Element<'_, Message> {
    let mut page = column![
        boards_shell::header("Better Harness", Message::HarnessPanelToggled),
        new_session_form(state),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if let Some(notice) = &state.harness_panel.notice {
        page = page.push(boards_shell::notice_row(notice));
    }

    let mut list = column![].spacing(10).width(Fill);
    if state.harness.sessions.is_empty() {
        list = list.push(
            text("还没有会话；把外部 agent（如 codex exec \"…\"）托管为一个 Harness 会话")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    } else {
        list = list.push(
            text(format!("共 {} 个会话", state.harness.sessions.len()))
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
        for session in &state.harness.sessions {
            list = list.push(session_card(state, session));
        }
    }
    page.push(scrollable(list).height(Fill)).into()
}

/// The new-session form: name, the agent command, and an optional cwd.
fn new_session_form(state: &State) -> Element<'_, Message> {
    let panel = &state.harness_panel;
    let body = column![
        text("托管外部 Agent").size(theme::SIZE_BODY),
        boards_shell::form_row(
            "名称",
            text_input("会话名称", &panel.draft_name)
                .on_input(Message::HarnessDraftNameChanged)
                .width(Fill),
        ),
        boards_shell::form_row(
            "命令",
            text_input("如 codex exec \"修复 lint 告警\"", &panel.draft_command,)
                .on_input(Message::HarnessDraftCommandChanged)
                .width(Fill),
        ),
        boards_shell::form_row(
            "目录",
            text_input("工作目录（留空使用当前工作区）", &panel.draft_cwd)
                .on_input(Message::HarnessDraftCwdChanged)
                .width(Fill),
        ),
        row![
            Space::new().width(Fill),
            button(text("添加会话").size(theme::SIZE_XS))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::HarnessSessionAdded),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Fill);
    boards_shell::card(body)
}

/// The process state in Chinese, for the session headline.
fn status_label(status: &HarnessStatus) -> String {
    match status {
        HarnessStatus::Pending => String::from("未启动"),
        HarnessStatus::Running => String::from("运行中"),
        HarnessStatus::Succeeded => String::from("已成功"),
        HarnessStatus::Failed { reason } => format!("失败：{reason}"),
        HarnessStatus::Stopped => String::from("已停止"),
    }
}

/// One session: the collapsible headline (name, stage, status, actions)
/// and, while selected, the log tail plus the review / repair forms.
fn session_card<'a>(state: &'a State, session: &'a HarnessSession) -> Element<'a, Message> {
    let selected = state.harness_panel.selected.as_deref() == Some(session.id.as_str());
    let name_button = button(
        text(session.name.clone())
            .size(theme::SIZE_MD)
            .style(theme::brand),
    )
    .style(theme::ghost_button)
    .on_press(Message::HarnessSelected(if selected {
        None
    } else {
        Some(session.id.clone())
    }));

    let mut headline = row![
        name_button,
        text(format!("阶段：{}", session.stage.label()))
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
        text(status_label(&session.status))
            .size(theme::SIZE_XS)
            .style(if session.status == HarnessStatus::Running {
                theme::brand
            } else {
                theme::dim
            }),
        Space::new().width(Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if session.status == HarnessStatus::Running {
        headline = headline.push(boards_shell::small_button(
            "停止",
            Message::HarnessSessionStopped(session.id.clone()),
        ));
    } else {
        headline = headline.push(boards_shell::small_button(
            "启动",
            Message::HarnessSessionStarted(session.id.clone()),
        ));
    }
    headline = headline.push(boards_shell::small_button(
        "下一阶段",
        Message::HarnessStageAdvanced(session.id.clone()),
    ));
    headline = headline.push(boards_shell::small_button(
        "删除",
        Message::HarnessSessionRemoved(session.id.clone()),
    ));

    let mut body = column![headline].spacing(4).width(Fill);
    body = body.push(
        text(session.command.clone())
            .size(theme::SIZE_SM)
            .style(theme::dim),
    );
    if let Some(cwd) = &session.cwd {
        body = body.push(
            text(cwd.display().to_string())
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    }
    if selected {
        body = body.push(log_section(state, session));
        body = body.push(review_section(state, session));
    }
    boards_shell::card(body)
}

/// The captured output's last lines, dim.
fn log_section<'a>(state: &'a State, session: &'a HarnessSession) -> Element<'a, Message> {
    let mut section = column![
        text("运行日志（末尾）")
            .size(theme::SIZE_XS)
            .style(theme::faint_text)
    ]
    .spacing(2)
    .width(Fill);
    let tail = state.harness.log_tail(&session.id, 12);
    if tail.is_empty() {
        section = section.push(
            text("（暂无输出）")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
    }
    for line in tail {
        section = section.push(text(line).size(theme::SIZE_XS).style(theme::faint_text));
    }
    section.into()
}

/// The review form and the derived repair checklist.
fn review_section<'a>(state: &'a State, session: &'a HarnessSession) -> Element<'a, Message> {
    let panel = &state.harness_panel;
    let mut section = column![
        text("结果审查")
            .size(theme::SIZE_XS)
            .style(theme::faint_text)
    ]
    .spacing(6)
    .width(Fill);
    if let Some(review) = &session.review {
        section = section.push(
            text(format!(
                "已记录结论：{}（{} 条发现）",
                review.verdict,
                review.findings.len()
            ))
            .size(theme::SIZE_XS)
            .style(theme::brand),
        );
    }
    section = section.push(boards_shell::form_row(
        "结论",
        text_input("如 通过 / 存在问题", &panel.review_verdict)
            .on_input(Message::HarnessReviewVerdictChanged)
            .width(Fill),
    ));
    section = section.push(boards_shell::form_row(
        "发现",
        text_input("用「；」分隔多项发现", &panel.review_findings)
            .on_input(Message::HarnessReviewFindingsChanged)
            .width(Fill),
    ));
    section = section.push(
        row![
            boards_shell::small_button(
                "记录审查",
                Message::HarnessReviewRecorded(session.id.clone()),
            ),
            boards_shell::small_button(
                "生成修复清单",
                Message::HarnessRepairPlanned(session.id.clone()),
            ),
        ]
        .spacing(6),
    );
    if !session.repair_steps.is_empty() {
        section = section.push(
            text("修复清单")
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        );
        for (index, step) in session.repair_steps.iter().enumerate() {
            section = section.push(checkbox(step.done).label(step.text.clone()).on_toggle({
                let id = session.id.clone();
                move |_| Message::HarnessRepairToggled {
                    id: id.clone(),
                    index,
                }
            }));
        }
    }
    section.into()
}
