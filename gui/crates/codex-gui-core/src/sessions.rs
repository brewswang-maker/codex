//! Sidebar session inventory: the thread list the user can resume or archive.
//!
//! The GUI keeps a single page of [`ThreadSummary`] rows produced by
//! `thread/list`; resuming and archiving act on those ids and refresh the
//! page through the same call.

use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadActiveFlag;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::Turn;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Quest lifecycle state projected from the backend's [`ThreadStatus`].
///
/// Mirrors Qoder's four card states: running, waiting on the user, parked,
/// and failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum QuestStatus {
    /// Parked: loaded but nothing is running.
    #[default]
    Idle,
    /// A turn is streaming.
    Running,
    /// The agent is blocked on the user (approval or an answer).
    Waiting,
    /// The last turn or the system failed.
    Error,
}

impl QuestStatus {
    /// Short label for the row badge and the board columns.
    pub fn label(self) -> &'static str {
        match self {
            QuestStatus::Idle => "空闲",
            QuestStatus::Running => "执行中",
            QuestStatus::Waiting => "等待操作",
            QuestStatus::Error => "错误",
        }
    }

    /// Whether the quest is still live (running or blocked), i.e. belongs
    /// in the board's active columns.
    pub fn is_active(self) -> bool {
        matches!(self, QuestStatus::Running | QuestStatus::Waiting)
    }
}

/// Projects the wire status down to the four waypoint states; a waiting
/// flag wins over a plain active run.
impl From<&ThreadStatus> for QuestStatus {
    fn from(status: &ThreadStatus) -> Self {
        match status {
            ThreadStatus::Active { active_flags } => {
                let waiting = active_flags.iter().any(|flag| {
                    matches!(
                        flag,
                        ThreadActiveFlag::WaitingOnApproval | ThreadActiveFlag::WaitingOnUserInput
                    )
                });
                if waiting {
                    QuestStatus::Waiting
                } else {
                    QuestStatus::Running
                }
            }
            ThreadStatus::SystemError => QuestStatus::Error,
            ThreadStatus::Idle | ThreadStatus::NotLoaded => QuestStatus::Idle,
        }
    }
}

/// One sidebar row: enough metadata to recognize, group, and resume a
/// thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummary {
    pub id: String,
    pub preview: String,
    pub updated_at: i64,
    /// Working directory the thread runs in; the sidebar groups rows by it.
    pub cwd: String,
    /// Server-side lifecycle state; drives the row badge and the board.
    pub status: QuestStatus,
    /// User-visible name once renamed; falls back to the preview.
    pub name: Option<String>,
}

impl ThreadSummary {
    /// The label to show: the rename over the auto preview, with the
    /// untitled fallback.
    pub fn label(&self) -> &str {
        if let Some(name) = self.name.as_deref().filter(|name| !name.is_empty()) {
            return name;
        }
        if self.preview.is_empty() {
            "(untitled)"
        } else {
            &self.preview
        }
    }
}

/// Lightweight resume outcome: the active thread id, the `(provider,
/// model)` pair it is bound to, its working directory, and the persisted
/// turns to replay into a transcript. Projection keeps the fat
/// [`ThreadResumeResponse`] out of the UI message enum.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionHistory {
    pub thread_id: String,
    /// The provider the thread's next turns route through.
    pub model_provider: String,
    /// The model the thread's next turns use; it can differ from the
    /// configured default when the thread predates a config change.
    pub model: String,
    pub cwd: String,
    pub turns: Vec<Turn>,
}

impl SessionHistory {
    /// Projects a resume response down to what the transcript needs.
    pub fn from_resume(response: &ThreadResumeResponse) -> Self {
        Self {
            thread_id: response.thread.id.clone(),
            model_provider: response.model_provider.clone(),
            model: response.model.clone(),
            cwd: response.cwd.to_string_lossy().into_owned(),
            turns: response.thread.turns.clone(),
        }
    }
}

/// The sidebar inventory plus the pagination cursor for "load more".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sessions {
    threads: Vec<ThreadSummary>,
    next_cursor: Option<String>,
}

impl Sessions {
    /// Replaces the visible page with a fresh `thread/list` response.
    pub fn apply_list(&mut self, response: &ThreadListResponse) {
        self.threads = response.data.iter().map(ThreadSummary::from).collect();
        self.next_cursor = response.next_cursor.clone();
    }

    /// Drops one thread (e.g. it got archived); returns whether it was
    /// present.
    pub fn remove(&mut self, thread_id: &str) -> bool {
        let before = self.threads.len();
        self.threads.retain(|thread| thread.id != thread_id);
        self.threads.len() != before
    }

    /// Applies one `thread/status/changed` notification; returns whether
    /// the row was present.
    pub fn apply_status(&mut self, thread_id: &str, status: QuestStatus) -> bool {
        let Some(thread) = self
            .threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            return false;
        };
        thread.status = status;
        true
    }

    /// Applies one `thread/name/updated` notification; returns whether the
    /// row was present.
    pub fn apply_name(&mut self, thread_id: &str, name: Option<String>) -> bool {
        let Some(thread) = self
            .threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            return false;
        };
        thread.name = name;
        true
    }

    /// Sidebar rows in server order.
    pub fn threads(&self) -> &[ThreadSummary] {
        &self.threads
    }

    /// Whether another page exists.
    pub fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// Projects the full thread payload down to the sidebar fields.
impl From<&Thread> for ThreadSummary {
    fn from(thread: &Thread) -> Self {
        Self {
            id: thread.id.clone(),
            preview: thread.preview.clone(),
            updated_at: thread.updated_at,
            cwd: thread.cwd.to_string_lossy().into_owned(),
            status: QuestStatus::from(&thread.status),
            name: thread.name.clone(),
        }
    }
}

/// Splits the sidebar inventory into the pinned partition and the
/// per-project groups (ZCode's pinned-then-grouped ordering), both with a
/// case-insensitive text filter applied. A blank filter keeps everything;
/// matching looks at the preview text, the rename, and the working
/// directory.
pub fn organized_sidebar<'a>(
    threads: &'a [ThreadSummary],
    pinned_ids: &BTreeSet<String>,
    filter: &str,
) -> (
    Vec<&'a ThreadSummary>,
    BTreeMap<&'a str, Vec<&'a ThreadSummary>>,
) {
    let needle = filter.trim().to_lowercase();
    let kept = |thread: &ThreadSummary| {
        needle.is_empty()
            || thread.preview.to_lowercase().contains(&needle)
            || thread.cwd.to_lowercase().contains(&needle)
            || thread
                .name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().contains(&needle))
    };

    let mut pinned = Vec::new();
    let mut groups: BTreeMap<&str, Vec<&ThreadSummary>> = BTreeMap::new();
    for thread in threads {
        if !kept(thread) {
            continue;
        }
        if pinned_ids.contains(&thread.id) {
            pinned.push(thread);
        } else {
            groups.entry(thread.cwd.as_str()).or_default().push(thread);
        }
    }
    (pinned, groups)
}
