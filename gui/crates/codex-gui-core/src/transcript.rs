//! The conversation transcript: the single source of truth for what the chat
//! pane renders.
//!
//! The transcript is *notification-driven*: entries appear when the app-server
//! reports `item/started`, grow through `item/agentMessage/delta`, and get
//! finalized by `item/completed`. This keeps local optimism out of the model,
//! which in turn makes `thread/resume` replay a pure re-run of the same
//! reducer.

use codex_app_server_protocol::FileUpdateChange;
use codex_app_server_protocol::PatchChangeKind;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnPlanStepStatus;
use codex_app_server_protocol::TurnPlanUpdatedNotification;
use codex_app_server_protocol::UserInput;
use std::path::PathBuf;

/// One rendered row of the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A user turn message; `images` are the local image attachments the
    /// server echoed back, rendered inline by the chat pane.
    UserMessage {
        id: String,
        text: String,
        images: Vec<PathBuf>,
    },
    /// An assistant message; `text` grows while deltas stream in.
    AgentMessage { id: String, text: String },
    /// A command execution item; `output` grows while output deltas
    /// stream in and is finalized by the completed snapshot, which also
    /// carries the exit code and wall duration.
    CommandExecution {
        id: String,
        command: String,
        output: String,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    },
    /// One patch-apply item; rendered as a single summary row, with the
    /// per-file diffs kept for the diff overlay.
    FileChange {
        id: String,
        changes: Vec<FileChangeRecord>,
    },
}

/// Flat display-ready record of one changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeRecord {
    pub path: String,
    /// `add`, `delete`, or `update` (moves surface as update).
    pub kind: String,
    pub diff: String,
}

/// The turn plan as the plan pane shows it; replaced by every
/// `turn/plan/updated` notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnPlan {
    /// Optional one-line summary of the plan.
    pub explanation: Option<String>,
    /// Ordered steps with display-ready statuses.
    pub steps: Vec<PlanStep>,
}

/// One plan row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub step: String,
    pub status: String,
}

/// Ordered conversation entries plus the live turn plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    entries: Vec<Entry>,
    plan: Option<TurnPlan>,
}

impl Transcript {
    /// Applies one server notification to the transcript.
    pub fn apply(&mut self, notification: &ServerNotification) {
        match notification {
            ServerNotification::ItemStarted(started) => self.apply_started(&started.item),
            ServerNotification::ItemCompleted(completed) => self.apply_completed(&completed.item),
            ServerNotification::AgentMessageDelta(delta) => {
                self.append_agent_delta(&delta.item_id, &delta.delta);
            }
            // The stable command-output stream; `process/outputDelta` is the
            // experimental spawn-session equivalent and stays unhandled for
            // now (spec section 9 marks it experimental, MVP display only).
            ServerNotification::CommandExecutionOutputDelta(delta) => {
                self.append_command_delta(&delta.item_id, &delta.delta);
            }
            ServerNotification::TurnPlanUpdated(plan) => self.apply_plan(plan),
            _ignored => {}
        }
    }

    /// Read-only view for the renderer.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The live turn plan, if one is running.
    pub fn plan(&self) -> Option<&TurnPlan> {
        self.plan.as_ref()
    }

    /// Aggregated `(added, removed)` line counts over every file-change
    /// entry. Updates use unified-diff `+`/`-` prefixes; add/delete
    /// snapshots carry raw file contents, so each content line counts as
    /// one added/removed line. Drives the sidebar badge on the active
    /// thread row.
    pub fn diff_totals(&self) -> (usize, usize) {
        let mut totals = (0usize, 0usize);
        for entry in &self.entries {
            let Entry::FileChange { changes, .. } = entry else {
                continue;
            };
            for record in changes {
                match record.kind.as_str() {
                    "add" => {
                        totals.0 += record.diff.lines().filter(|line| !line.is_empty()).count();
                    }
                    "delete" => {
                        totals.1 += record.diff.lines().filter(|line| !line.is_empty()).count();
                    }
                    _ => {
                        for line in record.diff.lines() {
                            if line.starts_with("+++") || line.starts_with("---") {
                                continue;
                            }
                            if line.starts_with('+') {
                                totals.0 += 1;
                            } else if line.starts_with('-') {
                                totals.1 += 1;
                            }
                        }
                    }
                }
            }
        }
        totals
    }

    /// Rebuilds the transcript from resumed-turn history, replacing any
    /// in-memory content. Persisted items are complete snapshots, so each
    /// item runs through the started/completed pair to land finalized.
    pub fn replay(&mut self, turns: &[Turn]) {
        self.entries.clear();
        self.plan = None;
        for turn in turns {
            for item in &turn.items {
                self.apply_started(item);
                self.apply_completed(item);
            }
        }
    }

    fn apply_plan(&mut self, notification: &TurnPlanUpdatedNotification) {
        self.plan = Some(TurnPlan {
            explanation: notification.explanation.clone(),
            steps: notification
                .plan
                .iter()
                .map(|step| PlanStep {
                    step: step.step.clone(),
                    status: plan_status_label(step.status),
                })
                .collect(),
        });
    }

    fn apply_started(&mut self, item: &ThreadItem) {
        match item {
            ThreadItem::UserMessage { id, content, .. } => {
                let text = user_text(content);
                let images = user_images(content);
                self.entries.push(Entry::UserMessage {
                    id: id.clone(),
                    text,
                    images,
                });
            }
            ThreadItem::AgentMessage { id, text, .. } => {
                self.entries.push(Entry::AgentMessage {
                    id: id.clone(),
                    text: text.clone(),
                });
            }
            ThreadItem::CommandExecution { id, command, .. } => {
                self.entries.push(Entry::CommandExecution {
                    id: id.clone(),
                    command: command.clone(),
                    output: String::new(),
                    exit_code: None,
                    duration_ms: None,
                });
            }
            ThreadItem::FileChange { id, changes, .. } => {
                self.entries.push(Entry::FileChange {
                    id: id.clone(),
                    changes: file_change_records(changes),
                });
            }
            // MCP calls, ... get their own entry kinds in
            // later stages; skip them for now.
            _other => {}
        }
    }

    fn apply_completed(&mut self, item: &ThreadItem) {
        match item {
            ThreadItem::AgentMessage { id, text, .. } => {
                // Trust the completed snapshot over the streamed deltas; this
                // also covers items whose start notification was missed.
                match self
                    .entries
                    .iter_mut()
                    .find(|entry| matches!(entry, Entry::AgentMessage { id: entry_id, .. } if entry_id == id))
                {
                    Some(Entry::AgentMessage { text: entry_text, .. }) => {
                        *entry_text = text.clone();
                    }
                    _missing => self.entries.push(Entry::AgentMessage {
                        id: id.clone(),
                        text: text.clone(),
                    }),
                }
            }
            ThreadItem::UserMessage { .. } => {
                // Already inserted on item/started; nothing to finalize.
            }
            ThreadItem::CommandExecution {
                id,
                aggregated_output,
                exit_code,
                duration_ms,
                ..
            } => {
                // Trust the completed snapshot over the streamed deltas.
                let output = aggregated_output.clone().unwrap_or_default();
                match self.entries.iter_mut().find(|entry| {
                    matches!(entry, Entry::CommandExecution { id: entry_id, .. } if entry_id == id)
                }) {
                    Some(Entry::CommandExecution {
                        output: entry_output,
                        exit_code: entry_exit,
                        duration_ms: entry_duration,
                        ..
                    }) => {
                        *entry_output = output;
                        *entry_exit = *exit_code;
                        *entry_duration = *duration_ms;
                    }
                    _missing => self.entries.push(Entry::CommandExecution {
                        id: id.clone(),
                        command: String::new(),
                        output,
                        exit_code: *exit_code,
                        duration_ms: *duration_ms,
                    }),
                }
            }
            ThreadItem::FileChange { id, changes, .. } => {
                // Trust the completed snapshot over the started list.
                let records = file_change_records(changes);
                match self.entries.iter_mut().find(|entry| {
                    matches!(entry, Entry::FileChange { id: entry_id, .. } if entry_id == id)
                }) {
                    Some(Entry::FileChange { changes: entry_changes, .. }) => {
                        *entry_changes = records;
                    }
                    _missing => self.entries.push(Entry::FileChange {
                        id: id.clone(),
                        changes: records,
                    }),
                }
            }
            _other => {}
        }
    }

    fn append_agent_delta(&mut self, item_id: &str, delta: &str) {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::AgentMessage { id, .. } if id == item_id))
        else {
            // Delta without a started entry: materialize the entry lazily so
            // out-of-order delivery cannot drop text.
            self.entries.push(Entry::AgentMessage {
                id: item_id.to_string(),
                text: delta.to_string(),
            });
            return;
        };

        if let Entry::AgentMessage { text, .. } = entry {
            text.push_str(delta);
        }
    }

    fn append_command_delta(&mut self, item_id: &str, delta: &str) {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::CommandExecution { id, .. } if id == item_id))
        else {
            // Unlike agent messages, orphan output would materialize an
            // entry without its command line; skip it instead.
            return;
        };

        if let Entry::CommandExecution { output, .. } = entry {
            output.push_str(delta);
        }
    }
}

/// Flattens protocol patch changes into display records.
fn file_change_records(changes: &[FileUpdateChange]) -> Vec<FileChangeRecord> {
    changes
        .iter()
        .map(|change| FileChangeRecord {
            path: change.path.clone(),
            kind: change_kind_label(&change.kind),
            diff: change.diff.clone(),
        })
        .collect()
}

/// Short label for a patch change kind.
fn change_kind_label(kind: &PatchChangeKind) -> String {
    match kind {
        PatchChangeKind::Add => String::from("add"),
        PatchChangeKind::Delete => String::from("delete"),
        PatchChangeKind::Update { .. } => String::from("update"),
    }
}

/// Display-ready label for a plan step status.
fn plan_status_label(status: TurnPlanStepStatus) -> String {
    match status {
        TurnPlanStepStatus::Pending => "pending".to_string(),
        TurnPlanStepStatus::InProgress => "in progress".to_string(),
        TurnPlanStepStatus::Completed => "completed".to_string(),
    }
}

/// Concatenates the text fragments of a user message content list.
fn user_text(content: &[UserInput]) -> String {
    content
        .iter()
        .filter_map(|input| match input {
            UserInput::Text { text, .. } => Some(text.as_str()),
            UserInput::Image { .. }
            | UserInput::LocalImage { .. }
            | UserInput::Audio { .. }
            | UserInput::LocalAudio { .. }
            | UserInput::Skill { .. }
            | UserInput::Mention { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Collects the on-disk image paths of a user message content list; only
/// `LocalImage` carries a renderable path (remote `Image` needs fetching).
fn user_images(content: &[UserInput]) -> Vec<PathBuf> {
    content
        .iter()
        .filter_map(|input| match input {
            UserInput::LocalImage { path, .. } => Some(path.clone()),
            UserInput::Text { .. }
            | UserInput::Image { .. }
            | UserInput::Audio { .. }
            | UserInput::LocalAudio { .. }
            | UserInput::Skill { .. }
            | UserInput::Mention { .. } => None,
        })
        .collect()
}
