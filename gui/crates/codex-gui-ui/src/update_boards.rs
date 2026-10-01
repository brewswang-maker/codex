//! Panel-side updates for the scheduler, knowledge, plugin-market, and
//! Better Harness surfaces, split out of `update.rs` to keep that
//! dispatch table focused on the app-server conversation.
//!
//! Every message here is purely local: the four engines persist
//! themselves, and no app-server round-trips are involved. The forward
//! in `update.rs` routes exactly the variants matched below, so the
//! compiler flags any variant added to one side but not the other.

use crate::message::Message;
use crate::state::PanelNotice;
use crate::state::State;
use codex_gui_core::ScheduleKind;
use codex_gui_core::ScheduledTask;
use codex_gui_core::now_secs;
use codex_gui_core::parse_delay_secs;
use iced::Task;
use std::path::Path;
use std::path::PathBuf;

/// The board-panel subset of [`Message`], projected at the forward site
/// in `update.rs`. `boards_message` matches it exhaustively, so the
/// compiler flags any variant added to one side but not the other.
pub(crate) enum BoardsMessage {
    SchedulerPanelToggled,
    SchedulerDraftNameChanged(String),
    SchedulerDraftOncePicked(bool),
    SchedulerDraftDelayChanged(String),
    SchedulerDraftIntervalChanged(String),
    SchedulerDraftPromptChanged(String),
    SchedulerDraftSubmitted,
    SchedulerTaskToggled(String),
    SchedulerTaskRemoved(String),
    KnowledgePanelToggled,
    KnowledgeQueryChanged(String),
    KnowledgeDraftTitleChanged(String),
    KnowledgeDraftBodyChanged(String),
    KnowledgeDraftTagsChanged(String),
    KnowledgeEditStarted(String),
    KnowledgeDraftSubmitted,
    KnowledgeEditCancelled,
    KnowledgeEntryRemoved(String),
    KnowledgeInjected(String),
    PluginMarketPanelToggled,
    PluginMarketSourceChanged(String),
    PluginMarketImportSubmitted,
    PluginMarketToggled(String),
    PluginMarketUninstalled(String),
    PluginMarketRolledBack(String),
    HarnessPanelToggled,
    HarnessSelected(Option<String>),
    HarnessDraftNameChanged(String),
    HarnessDraftCommandChanged(String),
    HarnessDraftCwdChanged(String),
    HarnessSessionAdded,
    HarnessSessionRemoved(String),
    HarnessSessionStarted(String),
    HarnessSessionStopped(String),
    HarnessStageAdvanced(String),
    HarnessReviewVerdictChanged(String),
    HarnessReviewFindingsChanged(String),
    HarnessReviewRecorded(String),
    HarnessRepairPlanned(String),
    HarnessRepairToggled { id: String, index: usize },
}

impl BoardsMessage {
    /// Projects a raw `Message` onto the board subset; `None` leaves it
    /// for the main dispatch.
    pub(crate) fn project(message: Message) -> Option<Self> {
        Some(match message {
            Message::SchedulerPanelToggled => Self::SchedulerPanelToggled,
            Message::SchedulerDraftNameChanged(text) => Self::SchedulerDraftNameChanged(text),
            Message::SchedulerDraftOncePicked(once) => Self::SchedulerDraftOncePicked(once),
            Message::SchedulerDraftDelayChanged(text) => Self::SchedulerDraftDelayChanged(text),
            Message::SchedulerDraftIntervalChanged(text) => {
                Self::SchedulerDraftIntervalChanged(text)
            }
            Message::SchedulerDraftPromptChanged(text) => Self::SchedulerDraftPromptChanged(text),
            Message::SchedulerDraftSubmitted => Self::SchedulerDraftSubmitted,
            Message::SchedulerTaskToggled(id) => Self::SchedulerTaskToggled(id),
            Message::SchedulerTaskRemoved(id) => Self::SchedulerTaskRemoved(id),
            Message::KnowledgePanelToggled => Self::KnowledgePanelToggled,
            Message::KnowledgeQueryChanged(text) => Self::KnowledgeQueryChanged(text),
            Message::KnowledgeDraftTitleChanged(text) => Self::KnowledgeDraftTitleChanged(text),
            Message::KnowledgeDraftBodyChanged(text) => Self::KnowledgeDraftBodyChanged(text),
            Message::KnowledgeDraftTagsChanged(text) => Self::KnowledgeDraftTagsChanged(text),
            Message::KnowledgeEditStarted(id) => Self::KnowledgeEditStarted(id),
            Message::KnowledgeDraftSubmitted => Self::KnowledgeDraftSubmitted,
            Message::KnowledgeEditCancelled => Self::KnowledgeEditCancelled,
            Message::KnowledgeEntryRemoved(id) => Self::KnowledgeEntryRemoved(id),
            Message::KnowledgeInjected(id) => Self::KnowledgeInjected(id),
            Message::PluginMarketPanelToggled => Self::PluginMarketPanelToggled,
            Message::PluginMarketSourceChanged(text) => Self::PluginMarketSourceChanged(text),
            Message::PluginMarketImportSubmitted => Self::PluginMarketImportSubmitted,
            Message::PluginMarketToggled(id) => Self::PluginMarketToggled(id),
            Message::PluginMarketUninstalled(id) => Self::PluginMarketUninstalled(id),
            Message::PluginMarketRolledBack(id) => Self::PluginMarketRolledBack(id),
            Message::HarnessPanelToggled => Self::HarnessPanelToggled,
            Message::HarnessSelected(id) => Self::HarnessSelected(id),
            Message::HarnessDraftNameChanged(text) => Self::HarnessDraftNameChanged(text),
            Message::HarnessDraftCommandChanged(text) => Self::HarnessDraftCommandChanged(text),
            Message::HarnessDraftCwdChanged(text) => Self::HarnessDraftCwdChanged(text),
            Message::HarnessSessionAdded => Self::HarnessSessionAdded,
            Message::HarnessSessionRemoved(id) => Self::HarnessSessionRemoved(id),
            Message::HarnessSessionStarted(id) => Self::HarnessSessionStarted(id),
            Message::HarnessSessionStopped(id) => Self::HarnessSessionStopped(id),
            Message::HarnessStageAdvanced(id) => Self::HarnessStageAdvanced(id),
            Message::HarnessReviewVerdictChanged(text) => Self::HarnessReviewVerdictChanged(text),
            Message::HarnessReviewFindingsChanged(text) => Self::HarnessReviewFindingsChanged(text),
            Message::HarnessReviewRecorded(id) => Self::HarnessReviewRecorded(id),
            Message::HarnessRepairPlanned(id) => Self::HarnessRepairPlanned(id),
            Message::HarnessRepairToggled { id, index } => Self::HarnessRepairToggled { id, index },
            _ => return None,
        })
    }
}

/// Which full-screen board panel a toggle message addresses.
enum BoardsPanel {
    Scheduler,
    Knowledge,
    PluginMarket,
    Harness,
}

/// Handles every scheduler/knowledge/plugin-market/harness message.
pub(crate) fn boards_message(state: &mut State, message: BoardsMessage) -> Task<Message> {
    match message {
        // --- Scheduler -------------------------------------------------
        BoardsMessage::SchedulerPanelToggled => {
            toggle_panel(state, BoardsPanel::Scheduler);
            Task::none()
        }
        BoardsMessage::SchedulerDraftNameChanged(text) => {
            state.scheduler_panel.draft_name = text;
            Task::none()
        }
        BoardsMessage::SchedulerDraftOncePicked(once) => {
            state.scheduler_panel.draft_once = once;
            Task::none()
        }
        BoardsMessage::SchedulerDraftDelayChanged(text) => {
            state.scheduler_panel.draft_delay = text;
            Task::none()
        }
        BoardsMessage::SchedulerDraftIntervalChanged(text) => {
            state.scheduler_panel.draft_interval = text;
            Task::none()
        }
        BoardsMessage::SchedulerDraftPromptChanged(text) => {
            state.scheduler_panel.draft_prompt = text;
            Task::none()
        }
        BoardsMessage::SchedulerDraftSubmitted => {
            let panel = &mut state.scheduler_panel;
            let now = now_secs();
            let name = panel.draft_name.trim().to_string();
            let prompt = panel.draft_prompt.trim().to_string();
            let kind = if panel.draft_once {
                parse_delay_secs(&panel.draft_delay).map(|secs| ScheduleKind::Once {
                    run_at: now + secs as i64,
                })
            } else {
                parse_delay_secs(&panel.draft_interval).map(|secs| ScheduleKind::Every {
                    interval_secs: secs,
                })
            };
            let Some(kind) = kind else {
                panel.notice = Some(PanelNotice::err(if panel.draft_once {
                    "延迟无效：支持 90s / 30m / 2h / 1d"
                } else {
                    "间隔无效：支持 90s / 30m / 2h / 1d"
                }));
                return Task::none();
            };
            if name.is_empty() || prompt.is_empty() {
                panel.notice = Some(PanelNotice::err("名称与提示词不能为空"));
                return Task::none();
            }
            let label = kind.label();
            state.scheduler.add(&name, kind, &prompt, now);
            panel.notice = Some(PanelNotice::ok(format!("已创建任务 {name}（{label}）")));
            panel.draft_name.clear();
            panel.draft_delay.clear();
            panel.draft_interval.clear();
            panel.draft_prompt.clear();
            Task::none()
        }
        BoardsMessage::SchedulerTaskToggled(id) => {
            let enabled = state
                .scheduler
                .tasks
                .iter()
                .find(|task| task.id == id)
                .map(|task| !task.enabled);
            if let Some(enabled) = enabled {
                state.scheduler.set_enabled(&id, enabled);
            }
            Task::none()
        }
        BoardsMessage::SchedulerTaskRemoved(id) => {
            state.scheduler.remove(&id);
            Task::none()
        }

        // --- Knowledge -------------------------------------------------
        BoardsMessage::KnowledgePanelToggled => {
            toggle_panel(state, BoardsPanel::Knowledge);
            Task::none()
        }
        BoardsMessage::KnowledgeQueryChanged(text) => {
            state.knowledge_panel.query = text;
            Task::none()
        }
        BoardsMessage::KnowledgeDraftTitleChanged(text) => {
            state.knowledge_panel.draft_title = text;
            Task::none()
        }
        BoardsMessage::KnowledgeDraftBodyChanged(text) => {
            state.knowledge_panel.draft_body = text;
            Task::none()
        }
        BoardsMessage::KnowledgeDraftTagsChanged(text) => {
            state.knowledge_panel.draft_tags = text;
            Task::none()
        }
        BoardsMessage::KnowledgeEditStarted(id) => {
            if let Some(entry) = state.knowledge.entries.iter().find(|entry| entry.id == id) {
                let panel = &mut state.knowledge_panel;
                panel.editing = Some(id);
                panel.draft_title = entry.title.clone();
                panel.draft_body = entry.body.clone();
                panel.draft_tags = entry.tags.join(", ");
            }
            Task::none()
        }
        BoardsMessage::KnowledgeDraftSubmitted => {
            let panel = &mut state.knowledge_panel;
            let title = panel.draft_title.trim().to_string();
            let body = panel.draft_body.trim().to_string();
            if title.is_empty() || body.is_empty() {
                panel.notice = Some(PanelNotice::err("标题与正文不能为空"));
                return Task::none();
            }
            let tags: Vec<String> = panel
                .draft_tags
                .split([',', '，'])
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(String::from)
                .collect();
            let now = now_secs();
            match panel.editing.clone() {
                Some(id) => {
                    if state.knowledge.update(&id, &title, &body, &tags, now) {
                        panel.notice = Some(PanelNotice::ok(format!("已更新「{title}」")));
                        panel.editing = None;
                        panel.draft_title.clear();
                        panel.draft_body.clear();
                        panel.draft_tags.clear();
                    } else {
                        panel.notice = Some(PanelNotice::err("条目不存在，可能已被删除"));
                    }
                }
                None => {
                    state.knowledge.add(&title, &body, &tags, "workspace", now);
                    panel.notice = Some(PanelNotice::ok(format!("已沉淀「{title}」")));
                    panel.draft_title.clear();
                    panel.draft_body.clear();
                    panel.draft_tags.clear();
                }
            }
            Task::none()
        }
        BoardsMessage::KnowledgeEditCancelled => {
            let panel = &mut state.knowledge_panel;
            panel.editing = None;
            panel.draft_title.clear();
            panel.draft_body.clear();
            panel.draft_tags.clear();
            Task::none()
        }
        BoardsMessage::KnowledgeEntryRemoved(id) => {
            state.knowledge.remove(&id);
            if state.knowledge_panel.editing.as_deref() == Some(id.as_str()) {
                state.knowledge_panel.editing = None;
            }
            Task::none()
        }
        BoardsMessage::KnowledgeInjected(id) => {
            let quoted = state
                .knowledge
                .entries
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| {
                    format!(
                        "请参考以下知识库条目：\n## {}\n{}\n",
                        entry.title, entry.body
                    )
                });
            let Some(injected) = quoted else {
                return Task::none();
            };
            let next = if state.composer.trim().is_empty() {
                injected
            } else {
                format!("{}\n\n{}", state.composer, injected)
            };
            state.set_composer(next);
            Task::none()
        }

        // --- Plugin market ---------------------------------------------
        BoardsMessage::PluginMarketPanelToggled => {
            toggle_panel(state, BoardsPanel::PluginMarket);
            Task::none()
        }
        BoardsMessage::PluginMarketSourceChanged(text) => {
            state.plugin_market.source_draft = text;
            Task::none()
        }
        BoardsMessage::PluginMarketImportSubmitted => {
            let source = state.plugin_market.source_draft.trim().to_string();
            if source.is_empty() {
                state.plugin_market_panel.notice =
                    Some(PanelNotice::err("请填写本地路径或 git 地址"));
                return Task::none();
            }
            let outcome = if source.starts_with("http://")
                || source.starts_with("https://")
                || source.starts_with("git@")
            {
                state.plugin_market.install_from_git(&source, now_secs())
            } else {
                state
                    .plugin_market
                    .install_from_path(Path::new(&source), now_secs())
            };
            state.plugin_market_panel.notice = Some(match outcome {
                Ok(name) => PanelNotice::ok(format!("插件 {name} 就绪")),
                Err(error) => PanelNotice::err(error),
            });
            Task::none()
        }
        BoardsMessage::PluginMarketToggled(id) => {
            let enabled = state
                .plugin_market
                .plugins
                .iter()
                .find(|record| record.id == id)
                .map(|record| !record.enabled);
            if let Some(enabled) = enabled {
                state.plugin_market.set_enabled(&id, enabled);
            }
            Task::none()
        }
        BoardsMessage::PluginMarketUninstalled(id) => {
            if let Err(error) = state.plugin_market.uninstall(&id) {
                state.plugin_market_panel.notice = Some(PanelNotice::err(error));
            }
            Task::none()
        }
        BoardsMessage::PluginMarketRolledBack(id) => {
            if let Err(error) = state.plugin_market.rollback(&id) {
                state.plugin_market_panel.notice = Some(PanelNotice::err(error));
            }
            Task::none()
        }

        // --- Better Harness --------------------------------------------
        BoardsMessage::HarnessPanelToggled => {
            toggle_panel(state, BoardsPanel::Harness);
            Task::none()
        }
        BoardsMessage::HarnessSelected(id) => {
            state.harness_panel.selected = id;
            Task::none()
        }
        BoardsMessage::HarnessDraftNameChanged(text) => {
            state.harness_panel.draft_name = text;
            Task::none()
        }
        BoardsMessage::HarnessDraftCommandChanged(text) => {
            state.harness_panel.draft_command = text;
            Task::none()
        }
        BoardsMessage::HarnessDraftCwdChanged(text) => {
            state.harness_panel.draft_cwd = text;
            Task::none()
        }
        BoardsMessage::HarnessSessionAdded => {
            let name = state.harness_panel.draft_name.trim().to_string();
            let command = state.harness_panel.draft_command.trim().to_string();
            if name.is_empty() || command.is_empty() {
                state.harness_panel.notice = Some(PanelNotice::err("名称与命令不能为空"));
                return Task::none();
            }
            let cwd = state.harness_panel.draft_cwd.trim().to_string();
            let cwd = (!cwd.is_empty()).then(|| PathBuf::from(cwd));
            let id = state.harness.add(&name, &command, cwd, now_secs());
            let panel = &mut state.harness_panel;
            panel.selected = Some(id);
            panel.notice = Some(PanelNotice::ok(format!("已创建会话 {name}")));
            panel.draft_name.clear();
            panel.draft_command.clear();
            panel.draft_cwd.clear();
            Task::none()
        }
        BoardsMessage::HarnessSessionRemoved(id) => {
            state.harness.remove(&id);
            if state.harness_panel.selected.as_deref() == Some(id.as_str()) {
                state.harness_panel.selected = None;
            }
            Task::none()
        }
        BoardsMessage::HarnessSessionStarted(id) => {
            if let Err(error) = state.harness.start(&id, now_secs()) {
                state.harness_panel.notice = Some(PanelNotice::err(error));
            }
            Task::none()
        }
        BoardsMessage::HarnessSessionStopped(id) => {
            if let Err(error) = state.harness.stop(&id) {
                state.harness_panel.notice = Some(PanelNotice::err(error));
            }
            Task::none()
        }
        BoardsMessage::HarnessStageAdvanced(id) => {
            state.harness.advance_stage(&id);
            Task::none()
        }
        BoardsMessage::HarnessReviewVerdictChanged(text) => {
            state.harness_panel.review_verdict = text;
            Task::none()
        }
        BoardsMessage::HarnessReviewFindingsChanged(text) => {
            state.harness_panel.review_findings = text;
            Task::none()
        }
        BoardsMessage::HarnessReviewRecorded(id) => {
            let panel = &mut state.harness_panel;
            let verdict = panel.review_verdict.trim().to_string();
            if verdict.is_empty() {
                panel.notice = Some(PanelNotice::err("结论不能为空"));
                return Task::none();
            }
            // The single-line draft separates findings with full-width
            // semicolons; the engine expects one finding per line.
            let findings = panel.review_findings.replace('；', "\n");
            if state
                .harness
                .record_review(&id, &verdict, &findings, now_secs())
            {
                panel.notice = Some(PanelNotice::ok("审查已记录"));
            } else {
                panel.notice = Some(PanelNotice::err("会话不存在"));
            }
            Task::none()
        }
        BoardsMessage::HarnessRepairPlanned(id) => match state.harness.plan_repairs(&id) {
            Ok(count) => {
                state.harness_panel.notice =
                    Some(PanelNotice::ok(format!("已生成 {count} 项修复步骤")));
                Task::none()
            }
            Err(error) => {
                state.harness_panel.notice = Some(PanelNotice::err(error));
                Task::none()
            }
        },
        BoardsMessage::HarnessRepairToggled { id, index } => {
            state.harness.toggle_repair_step(&id, index);
            Task::none()
        }
    }
}

/// The per-second board tick: reaps finished harness children and
/// collects due scheduled tasks. The caller turns fired tasks into
/// composer submissions.
pub fn tick_boards(state: &mut State) -> Vec<ScheduledTask> {
    let now = now_secs();
    let settled = state.harness.poll(now);
    if !settled.is_empty() {
        tracing::info!(count = settled.len(), "harness sessions settled");
    }
    state.scheduler.due(now)
}

/// Flips one full-screen panel; opening it takes the surface away from
/// every other full-screen surface (settings, skills, sibling panels).
fn toggle_panel(state: &mut State, panel: BoardsPanel) {
    let opening = match panel {
        BoardsPanel::Scheduler => !state.scheduler_panel.open,
        BoardsPanel::Knowledge => !state.knowledge_panel.open,
        BoardsPanel::PluginMarket => !state.plugin_market_panel.open,
        BoardsPanel::Harness => !state.harness_panel.open,
    };
    if opening {
        if state.settings.open {
            state.settings.toggle();
        }
        if state.skills.page_open {
            let _ = state.skills.toggle_page();
        }
        state.scheduler_panel.open = false;
        state.knowledge_panel.open = false;
        state.plugin_market_panel.open = false;
        state.harness_panel.open = false;
    }
    match panel {
        BoardsPanel::Scheduler => state.scheduler_panel.open = opening,
        BoardsPanel::Knowledge => state.knowledge_panel.open = opening,
        BoardsPanel::PluginMarket => state.plugin_market_panel.open = opening,
        BoardsPanel::Harness => state.harness_panel.open = opening,
    }
}
