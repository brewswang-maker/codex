//! The conversation transcript: the single source of truth for what the chat
//! pane renders.
//!
//! The transcript is *notification-driven*: entries appear when the app-server
//! reports `item/started`, grow through `item/agentMessage/delta`, and get
//! finalized by `item/completed`. This keeps local optimism out of the model,
//! which in turn makes `thread/resume` replay a pure re-run of the same
//! reducer.

use codex_app_server_protocol::FileUpdateChange;
use codex_app_server_protocol::McpToolCallStatus;
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
    /// A reasoning item: the model's visible thinking. `summary` parts are
    /// what the model chose to expose; `content` holds raw reasoning when no
    /// summary was produced. Both grow part-wise through their own delta
    /// notifications and are finalized by the completed snapshot.
    Reasoning {
        id: String,
        summary: Vec<String>,
        content: Vec<String>,
    },
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
    /// One MCP tool call; `arguments` and `result` are pretty-printed JSON
    /// for the expandable detail well.
    McpToolCall {
        id: String,
        server: String,
        tool: String,
        status: String,
        arguments: String,
        result: Option<String>,
        error: Option<String>,
        duration_ms: Option<i64>,
    },
    /// One patch-apply item; rendered as a single summary row, with the
    /// per-file diffs kept for the diff overlay.
    FileChange {
        id: String,
        changes: Vec<FileChangeRecord>,
    },
    /// A quiet divider row for background lifecycle events the transcript
    /// should acknowledge: context compaction and review-mode boundaries.
    SystemNote { id: String, text: String },
}

impl Entry {
    /// The reasoning body to display: the joined summary parts, falling
    /// back to the joined raw content parts when no summary was streamed.
    pub fn reasoning_text(&self) -> String {
        let Entry::Reasoning {
            summary, content, ..
        } = self
        else {
            return String::new();
        };
        let parts = if summary.iter().any(|part| !part.is_empty()) {
            summary
        } else {
            content
        };
        parts
            .iter()
            .filter(|part| !part.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    }
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
            ServerNotification::ReasoningSummaryTextDelta(delta) => {
                self.append_reasoning_delta(
                    &delta.item_id,
                    delta.summary_index,
                    &delta.delta,
                    true,
                );
            }
            ServerNotification::ReasoningSummaryPartAdded(added) => {
                self.ensure_reasoning_part(&added.item_id, added.summary_index, true);
            }
            ServerNotification::ReasoningTextDelta(delta) => {
                self.append_reasoning_delta(
                    &delta.item_id,
                    delta.content_index,
                    &delta.delta,
                    false,
                );
            }
            ServerNotification::TurnPlanUpdated(plan) => self.apply_plan(plan),
            // `thread/compacted` is deprecated in favor of the
            // `contextCompaction` item handled in `apply_started`; handling
            // both would duplicate the divider.
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
            ThreadItem::Reasoning {
                id,
                summary,
                content,
            } => {
                self.entries.push(Entry::Reasoning {
                    id: id.clone(),
                    summary: summary.clone(),
                    content: content.clone(),
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
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                status,
                arguments,
                ..
            } => {
                self.entries.push(Entry::McpToolCall {
                    id: id.clone(),
                    server: server.clone(),
                    tool: tool.clone(),
                    status: mcp_status_label(status),
                    arguments: pretty_json(arguments),
                    result: None,
                    error: None,
                    duration_ms: None,
                });
            }
            ThreadItem::FileChange { id, changes, .. } => {
                self.entries.push(Entry::FileChange {
                    id: id.clone(),
                    changes: file_change_records(changes),
                });
            }
            ThreadItem::ContextCompaction { id } => {
                self.entries.push(Entry::SystemNote {
                    id: id.clone(),
                    text: String::from("上下文已自动压缩"),
                });
            }
            ThreadItem::EnteredReviewMode { id, .. } => {
                self.entries.push(Entry::SystemNote {
                    id: id.clone(),
                    text: String::from("开始代码审查"),
                });
            }
            ThreadItem::ExitedReviewMode { id, .. } => {
                self.entries.push(Entry::SystemNote {
                    id: id.clone(),
                    text: String::from("代码审查完成"),
                });
            }
            // Hook prompts, plans, sub-agent traffic, ... get their own
            // entry kinds in later stages; skip them for now.
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
            ThreadItem::Reasoning {
                id,
                summary,
                content,
            } => {
                // Trust the completed snapshot over the streamed deltas.
                match self.reasoning_entry(id) {
                    Some(Entry::Reasoning {
                        summary: entry_summary,
                        content: entry_content,
                        ..
                    }) => {
                        *entry_summary = summary.clone();
                        *entry_content = content.clone();
                    }
                    _missing => self.entries.push(Entry::Reasoning {
                        id: id.clone(),
                        summary: summary.clone(),
                        content: content.clone(),
                    }),
                }
            }
            ThreadItem::McpToolCall {
                id,
                status,
                result,
                error,
                duration_ms,
                ..
            } => {
                // Trust the completed snapshot over the started item.
                let result_text = result.as_deref().map(mcp_result_text);
                let error_text = error.as_ref().map(|error| error.message.clone());
                match self.mcp_entry(id) {
                    Some(Entry::McpToolCall {
                        status: entry_status,
                        result: entry_result,
                        error: entry_error,
                        duration_ms: entry_duration,
                        ..
                    }) => {
                        *entry_status = mcp_status_label(status);
                        *entry_result = result_text;
                        *entry_error = error_text;
                        *entry_duration = *duration_ms;
                    }
                    _missing => self.entries.push(Entry::McpToolCall {
                        id: id.clone(),
                        server: String::new(),
                        tool: String::new(),
                        status: mcp_status_label(status),
                        arguments: String::new(),
                        result: result_text,
                        error: error_text,
                        duration_ms: *duration_ms,
                    }),
                }
            }
            _other => {}
        }
    }

    /// The reasoning entry with the given item id, when it already exists.
    fn reasoning_entry(&mut self, item_id: &str) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::Reasoning { id, .. } if id == item_id))
    }

    /// The MCP call entry with the given item id, when it already exists.
    fn mcp_entry(&mut self, item_id: &str) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::McpToolCall { id, .. } if id == item_id))
    }

    /// Appends one reasoning delta into the part list named by `index`,
    /// materializing the entry (and the part slot) when delivery is out of
    /// order.
    fn append_reasoning_delta(&mut self, item_id: &str, index: i64, delta: &str, summary: bool) {
        let part = usize::try_from(index.max(0)).unwrap_or(0);
        let position = match self
            .entries
            .iter()
            .position(|entry| matches!(entry, Entry::Reasoning { id, .. } if id == item_id))
        {
            Some(position) => position,
            None => {
                self.entries.push(Entry::Reasoning {
                    id: item_id.to_string(),
                    summary: Vec::new(),
                    content: Vec::new(),
                });
                self.entries.len() - 1
            }
        };
        let Some(Entry::Reasoning {
            summary: parts,
            content,
            ..
        }) = self.entries.get_mut(position)
        else {
            return;
        };
        let parts = if summary { parts } else { content };
        while parts.len() <= part {
            parts.push(String::new());
        }
        parts[part].push_str(delta);
    }

    /// Ensures the part slot named by `index` exists so subsequent deltas
    /// for that index append into a fresh section.
    fn ensure_reasoning_part(&mut self, item_id: &str, index: i64, summary: bool) {
        let part = usize::try_from(index.max(0)).unwrap_or(0);
        let position = match self
            .entries
            .iter()
            .position(|entry| matches!(entry, Entry::Reasoning { id, .. } if id == item_id))
        {
            Some(position) => position,
            None => {
                self.entries.push(Entry::Reasoning {
                    id: item_id.to_string(),
                    summary: Vec::new(),
                    content: Vec::new(),
                });
                self.entries.len() - 1
            }
        };
        let Some(Entry::Reasoning {
            summary: parts,
            content,
            ..
        }) = self.entries.get_mut(position)
        else {
            return;
        };
        let parts = if summary { parts } else { content };
        while parts.len() <= part {
            parts.push(String::new());
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

/// Display-ready label for an MCP tool call status.
fn mcp_status_label(status: &McpToolCallStatus) -> String {
    match status {
        McpToolCallStatus::InProgress => String::from("running…"),
        McpToolCallStatus::Completed => String::from("done"),
        McpToolCallStatus::Failed => String::from("failed"),
    }
}

/// Pretty-printed JSON for the detail wells; a JSON value always
/// serializes, so the fallback is unreachable in practice.
fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Pretty-printed MCP tool result (content blocks plus structured
/// content) for the detail well; serialization of these wire-shaped
/// values cannot fail in practice.
fn mcp_result_text(result: &codex_app_server_protocol::McpToolCallResult) -> String {
    serde_json::to_value(result)
        .map(|value| pretty_json(&value))
        .unwrap_or_default()
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
