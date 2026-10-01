//! The full-screen scheduled-task panel: one-shot and recurring tasks
//! whose prompts are submitted through the composer when due.

use crate::boards_shell;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::ScheduledTask;
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

/// Renders the full-screen scheduled-task panel.
pub fn panel(state: &State) -> Element<'_, Message> {
    let mut page = column![
        boards_shell::header("定时任务", Message::SchedulerPanelToggled),
        new_task_form(state),
    ]
    .padding(16)
    .spacing(12)
    .width(Fill);

    if let Some(notice) = &state.scheduler_panel.notice {
        page = page.push(boards_shell::notice_row(notice));
    }

    let mut list = column![].spacing(10).width(Fill);
    if state.scheduler.tasks.is_empty() {
        list = list.push(
            text("暂无任务；到期后会把提示词自动发送到当前会话")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
    } else {
        list = list.push(
            text(format!("共 {} 个任务", state.scheduler.tasks.len()))
                .size(theme::SIZE_SM)
                .style(theme::dim),
        );
        for task in &state.scheduler.tasks {
            list = list.push(task_card(task));
        }
    }
    page.push(scrollable(list).height(Fill)).into()
}

/// The new-task form: kind switch, timing draft, name, and prompt.
fn new_task_form(state: &State) -> Element<'_, Message> {
    let panel = &state.scheduler_panel;
    let kind_switch = row![
        checkbox(panel.draft_once)
            .label("一次性")
            .on_toggle(|once| Message::SchedulerDraftOncePicked(once)),
        checkbox(!panel.draft_once)
            .label("周期性")
            .on_toggle(|recurring| Message::SchedulerDraftOncePicked(!recurring)),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);
    let timing = if panel.draft_once {
        boards_shell::form_row(
            "延迟",
            text_input("如 30m / 2h（裸数字按分钟）", &panel.draft_delay)
                .on_input(Message::SchedulerDraftDelayChanged)
                .width(Fill),
        )
    } else {
        boards_shell::form_row(
            "间隔",
            text_input("如 90s / 30m / 2h / 1d", &panel.draft_interval)
                .on_input(Message::SchedulerDraftIntervalChanged)
                .width(Fill),
        )
    };
    let body = column![
        boards_shell::form_row(
            "名称",
            text_input("任务名称", &panel.draft_name)
                .on_input(Message::SchedulerDraftNameChanged)
                .width(Fill),
        ),
        boards_shell::form_row("类型", kind_switch),
        timing,
        boards_shell::form_row(
            "提示词",
            text_input("到期后发送的内容", &panel.draft_prompt)
                .on_input(Message::SchedulerDraftPromptChanged)
                .width(Fill),
        ),
        row![
            Space::new().width(Fill),
            button(text("创建任务").size(theme::SIZE_XS))
                .padding([6, 16])
                .style(theme::primary_button)
                .on_press(Message::SchedulerDraftSubmitted),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .width(Fill);
    boards_shell::card(column![text("新建任务").size(theme::SIZE_BODY), body].spacing(8))
}

/// One task row: the enable toggle, the kind, the prompt, and delete.
fn task_card(task: &ScheduledTask) -> Element<'_, Message> {
    let mut headline = row![
        checkbox(task.enabled).label(task.name.clone()).on_toggle({
            let id = task.id.clone();
            move |_| Message::SchedulerTaskToggled(id.clone())
        }),
        text(task.kind.label())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if !task.enabled {
        headline = headline.push(
            text("已停用")
                .size(theme::SIZE_XS)
                .style(theme::danger_text),
        );
    }
    let info = column![
        headline,
        text(task.prompt.clone())
            .size(theme::SIZE_SM)
            .style(theme::dim),
    ]
    .spacing(4)
    .width(Fill);
    let actions = row![boards_shell::small_button(
        "删除",
        Message::SchedulerTaskRemoved(task.id.clone()),
    )]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    boards_shell::card(
        row![info, actions]
            .spacing(12)
            .align_y(iced::Alignment::Center),
    )
}
