//! The GUI state and its connection lifecycle.

use crate::attachments::Attachments;
use crate::message::AppMode;
use crate::message::ArtifactTab;
use crate::message::MenuId;
use crate::message::QuestScenario;
use crate::message::SidebarTab;
use codex_app_server_protocol::McpServerStatus;
use codex_app_server_protocol::ThreadTokenUsage;
use codex_gui_bridge::Client;
use codex_gui_bridge::Flags;
use codex_gui_core::Approvals;
use codex_gui_core::ElicitationDraft;
use codex_gui_core::Elicitations;
use codex_gui_core::GitInfo;
use codex_gui_core::HarnessBoard;
use codex_gui_core::KnowledgeBase;
use codex_gui_core::MarkdownStream;
use codex_gui_core::Mentions;
use codex_gui_core::PinnedThreads;
use codex_gui_core::PluginMarket;
use codex_gui_core::QuestionDraft;
use codex_gui_core::Questions;
use codex_gui_core::RecentProjects;
use codex_gui_core::Scheduler;
use codex_gui_core::Sessions;
use codex_gui_core::Settings;
use codex_gui_core::SkillsBoard;
use codex_gui_core::StatusBoard;
use codex_gui_core::Transcript;
use codex_gui_core::VendorCatalog;
use iced::widget::markdown;
use iced::widget::text_editor;
use iced_swdir_tree::DirectoryTree;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

/// The command palette's runtime bits: open flag, query, cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaletteState {
    /// Whether the palette overlay is up.
    pub open: bool,
    /// Current query typed into the palette input.
    pub query: String,
    /// Highlighted row into the filtered action list.
    pub selected: usize,
}

/// The transcript search bar (ZCode ModelTrajectory borrow): open flag,
/// query, and the highlighted hit within the filtered view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatSearch {
    /// Whether the search bar is shown above the transcript.
    pub open: bool,
    /// Current query; blank means the transcript renders unfiltered.
    pub query: String,
    /// Index of the highlighted hit inside the hit list.
    pub hit: usize,
}

/// The Quest-mode overlay cluster: scenario picker, per-row action menu,
/// rename dialog, and the My Quests board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestOverlays {
    /// Scenario picker visibility (the create button's menu).
    pub picker_open: bool,
    /// The scenario picked for the live quest, shown as a header chip.
    pub scenario: Option<QuestScenario>,
    /// The per-row action menu.
    pub menu: QuestMenu,
    /// The rename dialog, when open.
    pub rename: Option<QuestRename>,
    /// My Quests board visibility.
    pub board_open: bool,
    /// Board project tab; `None` is "all projects".
    pub board_project: Option<String>,
    /// Board search query.
    pub board_filter: String,
    /// The launch page's workspace dropdown visibility.
    pub workspace_menu: bool,
    /// Whether the rail quest list shows past its row cap.
    pub list_expanded: bool,
}

impl QuestOverlays {
    /// Dismisses every transient overlay (Esc, mode switches, resumes).
    pub fn close_transient(&mut self) {
        self.picker_open = false;
        self.menu = QuestMenu::default();
        self.rename = None;
        self.board_open = false;
        self.workspace_menu = false;
    }

    /// Whether any overlay currently owns the keyboard.
    pub fn any_open(&self) -> bool {
        self.picker_open
            || self.menu.thread_id.is_some()
            || self.rename.is_some()
            || self.board_open
            || self.workspace_menu
    }
}

/// The Quest artifact panel: the task dialog's right-hand column with
/// the live plan (Spec) and the changed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestArtifacts {
    /// Whether the panel is part of the Quest layout.
    pub open: bool,
    /// Which tab is on screen.
    pub tab: ArtifactTab,
}

impl Default for QuestArtifacts {
    fn default() -> Self {
        // The reference's Quest shell opens with the output column up.
        Self {
            open: true,
            tab: ArtifactTab::Spec,
        }
    }
}

/// The per-quest action menu: which row is open and whether its delete
/// entry awaits confirmation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestMenu {
    /// The thread whose menu is open, when any.
    pub thread_id: Option<String>,
    /// Whether the delete entry shows its confirm state.
    pub confirm_delete: bool,
}

/// The rename dialog: the target thread plus the draft text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestRename {
    /// Thread being renamed.
    pub thread_id: String,
    /// Draft name bound to the dialog input.
    pub draft: String,
}

/// The central file-preview area: open tabs in click order, the active
/// tab, and the body loaded per tab (Qoder's editor group, read-only).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorPane {
    /// Open preview tabs, oldest first.
    pub tabs: Vec<PathBuf>,
    /// The tab whose body is on screen.
    pub active: Option<PathBuf>,
    /// Loaded body per tab; [`FileBody::Loading`] until the read settles.
    pub bodies: HashMap<PathBuf, FileBody>,
}

impl EditorPane {
    /// Opens the file or focuses its existing tab; returns `true` when a
    /// fresh tab was created (the caller loads the body).
    pub fn open(&mut self, path: PathBuf) -> bool {
        if self.tabs.contains(&path) {
            self.active = Some(path);
            return false;
        }
        self.bodies.insert(path.clone(), FileBody::Loading);
        self.tabs.push(path.clone());
        self.active = Some(path);
        true
    }

    /// Focuses an already-open tab.
    pub fn select(&mut self, path: &std::path::Path) {
        if self.tabs.iter().any(|tab| tab.as_path() == path) {
            self.active = Some(path.to_path_buf());
        }
    }

    /// Closes a tab; closing the active one activates its left neighbor.
    pub fn close(&mut self, path: &std::path::Path) {
        let Some(index) = self.tabs.iter().position(|tab| tab == path) else {
            return;
        };
        self.tabs.remove(index);
        self.bodies.remove(path);
        if self.active.as_deref() == Some(path) {
            self.active = index
                .checked_sub(1)
                .and_then(|left| self.tabs.get(left).cloned());
        }
    }
}

/// The body shown for one preview tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBody {
    /// The read is still running.
    Loading,
    /// Decodable UTF-8 text, truncated at the preview size cap. Shared
    /// (`Arc<str>`) so per-frame view rebuilds clone a reference count
    /// instead of copying the file body.
    Text(Arc<str>),
    /// Not valid UTF-8; there is nothing readable to show.
    Binary,
    /// Larger than the preview size cap.
    TooLarge,
    /// The read itself failed; the payload is the reason.
    Failed(String),
}

/// How a model-menu pick applies once its config write settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingModelApply {
    /// Same vendor as the live binding: flip the model on the running
    /// thread in place through `thread/settings/update`; no restart.
    InPlace {
        /// The thread to retarget.
        thread_id: String,
        /// The model id to switch to.
        model: String,
    },
    /// Different vendor: the provider is fixed per thread, so the
    /// running thread is forked onto the picked pair (its history comes
    /// along) once the config write lands.
    Retarget {
        /// The provider the thread moves to.
        provider_id: String,
        /// The model id the thread moves to.
        model: String,
    },
}

/// A one-line panel notice: the text plus whether it reports a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelNotice {
    /// The message shown in the panel.
    pub text: String,
    /// Whether the notice reports a failure.
    pub error: bool,
}

impl PanelNotice {
    /// A success notice.
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }

    /// A failure notice.
    pub fn err(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }
}

/// The scheduled-task panel: visibility, the new-task form, feedback.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchedulerPanel {
    /// Whether the panel replaces the whole surface.
    pub open: bool,
    /// New-task name draft.
    pub draft_name: String,
    /// `true` schedules a one-shot run, `false` a recurring one.
    pub draft_once: bool,
    /// One-shot delay draft (`90s` / `30m` / `2h` / `1d`).
    pub draft_delay: String,
    /// Recurring interval draft.
    pub draft_interval: String,
    /// New-task prompt draft (submitted as a composer turn).
    pub draft_prompt: String,
    /// The last form outcome.
    pub notice: Option<PanelNotice>,
}

/// The knowledge panel: visibility, the search query, the entry form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnowledgePanel {
    /// Whether the panel replaces the whole surface.
    pub open: bool,
    /// Search query filtering the entry list.
    pub query: String,
    /// Entry-form title draft.
    pub draft_title: String,
    /// Entry-form body draft.
    pub draft_body: String,
    /// Entry-form tag draft, comma-separated.
    pub draft_tags: String,
    /// The entry being edited; `None` creates a new one.
    pub editing: Option<String>,
    /// The last form outcome.
    pub notice: Option<PanelNotice>,
}

/// The plugin-market panel: visibility and feedback; the source draft
/// lives in the persisted [`codex_gui_core::PluginMarket`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginMarketPanel {
    /// Whether the panel replaces the whole surface.
    pub open: bool,
    /// The last operation outcome.
    pub notice: Option<PanelNotice>,
}

/// The Better Harness panel: visibility, the new-session form, the
/// review form, and which session's detail is expanded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarnessPanel {
    /// Whether the panel replaces the whole surface.
    pub open: bool,
    /// The session whose detail (log, review, repairs) is expanded.
    pub selected: Option<String>,
    /// New-session name draft.
    pub draft_name: String,
    /// New-session command draft (the external agent invocation).
    pub draft_command: String,
    /// New-session working-directory draft; blank inherits the GUI's.
    pub draft_cwd: String,
    /// Review verdict draft for the selected session.
    pub review_verdict: String,
    /// Review findings draft, one finding per line.
    pub review_findings: String,
    /// The last operation outcome.
    pub notice: Option<PanelNotice>,
}

/// Everything the update loop and the views need.
pub struct State {
    /// Launch flags of the app-server connection (subscription identity).
    pub flags: Flags,
    /// Handle onto the backend; delivered through
    /// [`GuiEvent::Connected`](codex_gui_bridge::GuiEvent::Connected).
    pub client: Option<Client>,
    /// Active thread once bootstrap finished.
    pub thread_id: Option<String>,
    /// The live turn id reported by `turn/started`, answering
    /// `turn/interrupt` while the turn streams.
    pub active_turn: Option<String>,
    /// Conversation model.
    pub transcript: Transcript,
    /// Parsed markdown per agent-message item id, grown incrementally via
    /// [`markdown::Content::push_str`](iced::widget::markdown::Content::push_str).
    pub markdowns: HashMap<String, markdown::Content>,
    /// Streaming deltas waiting for the next render tick.
    pub stream: MarkdownStream,
    /// Server-initiated approvals awaiting a decision.
    pub approvals: Approvals,
    /// Agent questions (`item/tool/requestUserInput`) awaiting answers.
    pub questions: Questions,
    /// Per-question drafts of the open input dialog (keyed by question id).
    pub question_drafts: HashMap<String, QuestionDraft>,
    /// MCP elicitations awaiting a decision.
    pub elicitations: Elicitations,
    /// Form drafts of the open elicitation dialog (keyed by property key).
    pub elicitation_drafts: HashMap<String, ElicitationDraft>,
    /// Validation failure of the current elicitation draft, if any.
    pub elicitation_error: Option<String>,
    /// Sidebar inventory of resumable threads.
    pub sessions: Sessions,
    /// Archived-thread inventory behind the sidebar's collapsible section.
    pub archived: Sessions,
    /// Whether the archived section is expanded.
    pub archived_open: bool,
    /// Whether the archived inventory has loaded at least once.
    pub archived_loaded: bool,
    /// Status bar domain: model catalog, login badge, error banner.
    pub status_board: StatusBoard,
    /// Settings panel domain: config drafts and save lifecycle.
    pub settings: Settings,
    /// Skills domain: the flattened inventory and the picker visibility.
    pub skills: SkillsBoard,
    /// The backend's `CODEX_HOME`, reported by `initialize`; the Skills
    /// page's disk mutations live under it.
    pub codex_home: Option<PathBuf>,
    /// Image attachments dropped or picked, pending the next submission.
    pub attachments: Attachments,
    /// Directory tree of the Files sidebar, rooted at the working directory.
    pub tree: DirectoryTree,
    /// Central file-preview area between the sidebar and the chat panel.
    pub editor: EditorPane,
    /// Which sidebar pane the activity bar shows.
    pub sidebar_tab: SidebarTab,
    /// The shell currently on screen.
    pub mode: AppMode,
    /// Whether the left rail (the editor sidebar or the Quest rail) is
    /// part of the layout.
    pub left_rail_open: bool,
    /// The Quest artifact panel state (right Spec / changed-files column).
    pub artifacts: QuestArtifacts,
    /// The open top-menu dropdown, when any.
    pub menu: Option<MenuId>,
    /// Arrival time (unix seconds) per user-message entry id, stamped by
    /// the update loop for the transcript timestamps.
    pub user_times: HashMap<String, i64>,
    /// Feedback key of the button currently showing its "copied" badge; a
    /// timed
    /// [`CopyFeedbackExpired`](crate::message::Message::CopyFeedbackExpired)
    /// clears it. Message copies key by entry id, code-block copies by a
    /// content hash.
    pub copied_key: Option<String>,
    /// The transcript entry the pointer is over; user-message actions
    /// (copy, edit) only render while their message is hovered.
    pub hovered_message: Option<String>,
    /// The transcript's active drag selection (a partial-text highlight),
    /// if any. Esc or a click elsewhere clears it; Ctrl+C copies it.
    pub text_selection: Option<TextSelection>,
    /// Monotonic id of the latest drag selection; line widgets report
    /// their plain text once per epoch so a fresh drag always re-reports.
    pub selection_epoch: u64,
    /// The inline editor draft while the user rewrites a sent message.
    pub editing: Option<EditDraft>,
    /// A submitted edit waiting on its fork/resume chain.
    pub pending_edit: Option<PendingEdit>,
    /// Like/dislike per agent-message id; absent means no reaction.
    pub reactions: HashMap<String, crate::message::Reaction>,
    /// Ids of command tool cards currently showing their output well.
    pub expanded_commands: BTreeSet<String>,
    /// Persisted recent-project list behind the welcome page.
    pub recents: RecentProjects,
    /// Working-tree facts of the active project (branch, changes).
    pub git: GitInfo,
    /// The multi-vendor model menu's merged catalog.
    pub vendors: VendorCatalog,
    /// Whether the Qoder-style model menu overlay is open.
    pub model_menu_open: bool,
    /// Whether a vendor-pick config write is in flight; its save
    /// completion reopens the thread so the pick takes effect.
    pub model_switch_pending: bool,
    /// The `(provider, model)` pair the live thread is bound to, as
    /// reported by its start/resume response or its settings updates;
    /// `None` while no binding has been observed.
    pub active_binding: Option<(String, String)>,
    /// How the in-flight model-menu pick should apply once its config
    /// write lands (in-place flip or thread retarget).
    pub pending_model_apply: Option<PendingModelApply>,
    /// Remote-explorer facts (SSH targets, Docker), probed lazily.
    pub remote: RemotePane,
    /// Workspace search replace-with draft; the query lives in `tree`.
    pub search_replace: String,
    /// Extension-marketplace filter text.
    pub extension_filter: String,
    /// Latest thread token usage reported by the backend; drives the
    /// context chip in the task bar.
    pub token_usage: Option<ThreadTokenUsage>,
    /// Cumulative unified diff of the running turn (`turn/diff/updated`).
    pub turn_diff: Option<String>,
    /// Diff overlay visibility.
    pub diff_overlay_open: bool,
    /// Selected file index inside the diff overlay.
    pub diff_selected: Option<usize>,
    /// Plan review overlay visibility.
    pub plan_overlay_open: bool,
    /// Commit-composer overlay state.
    pub git_overlay: GitOverlayState,
    /// Composer ✨ prompt-optimizer state.
    pub prompt_optimize: PromptOptimizeState,
    /// Composer voice-input (dictation) state.
    pub voice_input: VoiceInputState,
    /// Command palette visibility and query.
    pub palette: PaletteState,
    /// Transcript search bar state.
    pub chat_search: ChatSearch,
    /// Quest-mode overlay cluster (picker, menu, rename, board).
    pub quest: QuestOverlays,
    /// Persisted pinned-thread set behind the sidebar partition.
    pub pins: PinnedThreads,
    /// Sidebar thread-filter text; blank shows everything.
    pub sidebar_filter: String,
    /// Ids of expanded command groups (consecutive finished commands
    /// collapse into one group card otherwise).
    pub expanded_command_groups: BTreeSet<String>,
    /// Composer buffer.
    pub composer: String,
    /// Multi-line editor buffer mirroring `composer` for the widget;
    /// `set_composer` keeps the two in lockstep.
    pub composer_draft: iced::widget::text_editor::Content,
    /// Whether Shift is held right now: Enter sends, Shift+Enter breaks.
    pub shift_down: bool,
    /// The smart approval policy consulted before dialogs appear.
    pub approval_policy: codex_gui_core::ApprovalPolicy,
    /// Scheduled tasks driving future composer submissions.
    pub scheduler: Scheduler,
    /// Persisted knowledge base behind the knowledge panel.
    pub knowledge: KnowledgeBase,
    /// Installed cdylib-plugin registry behind the plugin market.
    pub plugin_market: PluginMarket,
    /// Managed external-agent sessions behind the Better Harness panel.
    pub harness: HarnessBoard,
    /// Scheduled-task panel visibility and form state.
    pub scheduler_panel: SchedulerPanel,
    /// Knowledge-panel visibility and form state.
    pub knowledge_panel: KnowledgePanel,
    /// Plugin-market panel visibility and feedback.
    pub plugin_market_panel: PluginMarketPanel,
    /// Better-Harness panel visibility and form state.
    pub harness_panel: HarnessPanel,
    /// The composer's `@file` mention tracker: token query, hits, highlight.
    pub mentions: Mentions,
    /// Ids of reasoning cards the user collapsed; absent means expanded.
    pub collapsed_reasoning: BTreeSet<String>,
    /// Ids of MCP call cards currently showing their detail well.
    pub expanded_mcp: BTreeSet<String>,
    /// MCP server inventory rendered by the settings panel.
    pub mcp_servers: Vec<McpServerStatus>,
    /// Whether the MCP inventory has loaded at least once.
    pub mcp_loaded: bool,
    /// The MCP server whose OAuth sign-in is pending, with the URL the
    /// user must open to finish it.
    pub mcp_login: Option<McpLogin>,
    /// Connection/turn lifecycle for the status bar.
    pub status: Status,
    /// Bumped on every user-requested relaunch; part of the subscription
    /// identity so a new epoch spawns a fresh backend connection.
    pub connection_epoch: u64,
}

/// A pending MCP OAuth sign-in: which server and the URL to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLogin {
    /// The MCP server being signed in.
    pub name: String,
    /// The authorization URL the user must open in a browser.
    pub url: String,
}

/// High-level lifecycle indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Spawning and handshaking with the backend.
    Bootstrapping,
    /// Ready for user input.
    Ready,
    /// A turn is streaming.
    Thinking,
    /// The backend is gone; the payload describes why.
    Disconnected(String),
}

/// The remote-explorer pane facts and whether they have been probed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemotePane {
    /// SSH host aliases from `~/.ssh/config`.
    pub targets: Vec<String>,
    /// Whether a working Docker daemon answered; `None` until probed.
    pub docker_available: Option<bool>,
    /// Whether the pane has already run its probe.
    pub loaded: bool,
}

/// The commit-composer overlay state (borrowed from AgentCodeGUI's
/// commit composer).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitOverlayState {
    /// Overlay visibility.
    pub open: bool,
    /// Selected file indexes from the working-tree list.
    pub selected: std::collections::BTreeSet<usize>,
    /// Draft commit message.
    pub message: String,
    /// Staged diff captured for the running AI draft.
    pub draft_diff: Option<String>,
    /// The throwaway thread drafting an AI commit message.
    pub draft_thread: Option<String>,
    /// Where the AI draft stands.
    pub status: GitOverlayStatus,
}

/// The AI commit draft lifecycle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GitOverlayStatus {
    /// Nothing running.
    #[default]
    Idle,
    /// A draft turn is running on the throwaway thread.
    Drafting,
    /// The last commit or draft failed; the payload is the reason.
    Failed(String),
}

/// The composer's ✨ prompt-optimizer state (borrowed from Qoder's
/// "优化输入"): one throwaway thread rewrites the draft into a
/// structured, actionable prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptOptimizeState {
    /// The throwaway thread running the rewrite.
    pub thread: Option<String>,
    /// The draft captured when the rewrite started; kept after the
    /// rewrite lands so the user can undo it.
    pub original: Option<String>,
    /// Where the optimizer stands.
    pub status: PromptOptimizeStatus,
}

impl PromptOptimizeState {
    /// Whether a rewrite turn is still running.
    pub fn running(&self) -> bool {
        matches!(self.status, PromptOptimizeStatus::Running)
    }

    /// Whether the composer currently shows an optimized rewrite that
    /// can be rolled back.
    pub fn undo_available(&self) -> bool {
        self.original.is_some()
    }
}

/// The prompt-optimizer lifecycle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PromptOptimizeStatus {
    /// Nothing running.
    #[default]
    Idle,
    /// The rewrite turn is running on the throwaway thread.
    Running,
    /// The last rewrite failed; the payload is the reason.
    Failed(String),
}

/// The composer's voice-input (dictation) state, aligned with Qoder's
/// Ctrl+Shift+V form: transcription only, never a live voice conversation.
///
/// Live text is appended after `anchor`; the composer tail from the anchor
/// on is rebuilt from `finalized` + `current`, so a finalized part can
/// refine the deltas that preceded it without duplicating text.
#[derive(Debug, Default)]
pub struct VoiceInputState {
    /// Where dictation stands.
    pub status: VoiceInputStatus,
    /// The throwaway thread the realtime session runs on.
    pub thread: Option<String>,
    /// Composer char offset the transcription anchors to.
    pub anchor: usize,
    /// Text of transcript parts already finalized by `TranscriptDone`.
    pub finalized: String,
    /// Live deltas of the in-flight transcript part.
    pub current: String,
    /// Recording-indicator heartbeat phase.
    pub pulse: bool,
    /// Signals the voice task to tear down (capture, session, thread).
    pub stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl VoiceInputState {
    /// Whether the dictation pipeline owns the composer and the mic right
    /// now (starting, live, or tearing down).
    pub fn active(&self) -> bool {
        matches!(
            self.status,
            VoiceInputStatus::Starting | VoiceInputStatus::Recording | VoiceInputStatus::Stopping
        )
    }

    /// Whether the microphone is live.
    pub fn recording(&self) -> bool {
        matches!(self.status, VoiceInputStatus::Recording)
    }

    /// Re-anchors the transcription at the end of the composer and drops
    /// the previous run's text; used when a session starts and whenever
    /// the user edits mid-dictation (the edit becomes part of the prefix).
    pub fn reanchor(&mut self, composer: &str) {
        self.anchor = composer.chars().count();
        self.finalized.clear();
        self.current.clear();
    }

    /// Applies one live delta: appended to the in-flight part, then the
    /// composer tail is rebuilt.
    pub fn apply_delta(&mut self, delta: &str, composer: &mut String) {
        self.current.push_str(delta);
        self.rebuild(composer);
    }

    /// Ends the in-flight part with its finalized text (which may refine
    /// the deltas it supersedes), then rebuilds the composer tail.
    pub fn apply_done(&mut self, text: &str, composer: &mut String) {
        self.finalized.push_str(text);
        self.current.clear();
        self.rebuild(composer);
    }

    /// Rebuilds the composer from its anchor prefix plus the transcript.
    fn rebuild(&self, composer: &mut String) {
        let byte_end = composer
            .char_indices()
            .nth(self.anchor)
            .map(|(byte, _)| byte)
            .unwrap_or(composer.len());
        composer.truncate(byte_end);
        composer.push_str(&self.finalized);
        composer.push_str(&self.current);
    }
}

/// The dictation lifecycle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum VoiceInputStatus {
    /// Nothing running.
    #[default]
    Idle,
    /// Opening the throwaway thread and the realtime session.
    Starting,
    /// The microphone is live and transcript text is streaming in.
    Recording,
    /// The user asked to stop; the pipeline is tearing down.
    Stopping,
    /// The last attempt failed; the payload is the reason.
    Failed(String),
}

/// One end of a drag selection: the identity of the text line the
/// pointer was over, plus the byte offset inside that line's plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Line identity (`md:{message}:{block}:{line}` for agent markdown,
    /// `user:{message}:{line}` for user bubbles).
    pub key: String,
    /// The byte offset inside the line's plain text.
    pub offset: usize,
}

/// The transcript's drag selection: the anchor where the drag started,
/// the focus under the pointer, and the plain text of every line the
/// selection covers (reported by the line widgets, keyed by line).
///
/// A selection never crosses message boundaries: focus updates from other
/// messages are dropped, so the anchor's message stays the owning one and
/// each line's highlight range is computable from the two points alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSelection {
    /// Where the drag started.
    pub anchor: SelectionPoint,
    /// Where the pointer is now.
    pub focus: SelectionPoint,
    /// Monotonic id shared with the line widgets; a covered line reports
    /// its plain text once per epoch, so a fresh drag re-reports.
    pub epoch: u64,
    /// Plain text per covered line key, as reported by the widgets.
    pub lines: BTreeMap<String, String>,
}

impl TextSelection {
    /// The message namespace the selection lives in (`md:{message}` or
    /// `user:{message}`).
    pub fn message(&self) -> Option<&str> {
        parse_line_key(&self.anchor.key).map(|(namespace, _, _)| namespace)
    }

    /// Whether a focus point belongs to the anchor's message; points from
    /// other messages would drag the selection across the transcript.
    pub fn accepts(&self, point: &SelectionPoint) -> bool {
        parse_line_key(&point.key).map(|(namespace, _, _)| namespace) == self.message()
    }

    /// The byte range `key` should paint, given that line's content
    /// length. `None` when the line sits outside the selection.
    pub fn range_for(&self, key: &str, content_len: usize) -> Option<Range<usize>> {
        let (namespace, block, line) = parse_line_key(key)?;
        let (anchor_namespace, anchor_block, anchor_line) = parse_line_key(&self.anchor.key)?;
        let (focus_namespace, focus_block, focus_line) = parse_line_key(&self.focus.key)?;
        if namespace != anchor_namespace || namespace != focus_namespace {
            return None;
        }

        let anchor = (anchor_block, anchor_line);
        let focus = (focus_block, focus_line);
        let (start, end, start_offset, end_offset) = if anchor <= focus {
            (anchor, focus, self.anchor.offset, self.focus.offset)
        } else {
            (focus, anchor, self.focus.offset, self.anchor.offset)
        };

        let position = (block, line);
        if position < start || position > end {
            return None;
        }

        let (lo, hi) = if position == start && position == end {
            (start_offset.min(end_offset), start_offset.max(end_offset))
        } else if position == start {
            (start_offset, content_len)
        } else if position == end {
            (0, end_offset)
        } else {
            (0, content_len)
        };
        let lo = lo.min(content_len);
        let hi = hi.min(content_len);

        (lo < hi).then_some(lo..hi)
    }

    /// Assembles the clipboard payload from the reported line texts, in
    /// document order; skipped line numbers keep their blank lines, and
    /// block boundaries join with a single newline.
    pub fn text(&self) -> Option<String> {
        let mut rows = Vec::new();
        for (key, text) in &self.lines {
            if let Some((_, block, line)) = parse_line_key(key) {
                rows.push((block, line, key.as_str(), text.as_str()));
            }
        }
        rows.sort_by_key(|(block, line, ..)| (*block, *line));

        let mut payload = String::new();
        let mut previous: Option<(usize, usize)> = None;
        for (block, line, key, text) in rows {
            let Some(range) = self.range_for(key, text.len()) else {
                continue;
            };
            let Some(slice) = text.get(range) else {
                continue;
            };
            if let Some((previous_block, previous_line)) = previous {
                let breaks = if previous_block == block {
                    line.saturating_sub(previous_line).max(1)
                } else {
                    1
                };
                payload.push_str(&"\n".repeat(breaks));
            }
            payload.push_str(slice);
            previous = Some((block, line));
        }

        (!payload.is_empty()).then_some(payload)
    }
}

/// Splits a line key into `(namespace, block, line)`: agent markdown keys
/// are `md:{message}:{block}:{line}`, user-bubble keys are
/// `user:{message}:{line}` (a single implicit block). The namespace keeps
/// its `md:`/`user:` scheme so the two families never collide.
pub fn parse_line_key(key: &str) -> Option<(&str, usize, usize)> {
    if let Some(rest) = key.strip_prefix("md:") {
        let (rest, line) = rest.rsplit_once(':')?;
        let (message, block) = rest.rsplit_once(':')?;
        let block = block.parse().ok()?;
        let line = line.parse().ok()?;
        // `message` is a subslice of `key`; widening it back over the
        // scheme keeps markdown namespaces distinct from user ones.
        Some((&key[..message.len() + 3], block, line))
    } else {
        let rest = key.strip_prefix("user:")?;
        let (message, line) = rest.rsplit_once(':')?;
        let line = line.parse().ok()?;
        Some((&key[..message.len() + 5], 0, line))
    }
}

/// One open in-place edit of an already-sent user message.
pub struct EditDraft {
    /// The transcript entry id being rewritten; the fork point is its
    /// turn, looked up in the transcript at submit time.
    pub message_id: String,
    /// Attachment paths carried by the original message, resent as-is.
    pub images: Vec<PathBuf>,
    /// The multiline editor buffer, seeded with the original text.
    pub content: text_editor::Content,
}

impl std::fmt::Debug for EditDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `text_editor::Content` is not `Debug`; project it down to its
        // text instead.
        f.debug_struct("EditDraft")
            .field("message_id", &self.message_id)
            .field("images", &self.images)
            .field("text", &self.content.text())
            .finish()
    }
}

/// A submitted edit waiting on its thread fork to land: the fork runs
/// first, then the new thread resumes (history below the edited message),
/// then the rewritten prompt goes out as a fresh turn.
#[derive(Debug)]
pub struct PendingEdit {
    /// The fork's thread id; `None` while `thread/fork` is in flight,
    /// set once it settles.
    pub forked_thread_id: Option<String>,
    /// The rewritten prompt to send.
    pub text: String,
    /// Attachment paths to resend with the prompt.
    pub images: Vec<PathBuf>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `DirectoryTree` is not `Debug`; project it down to its root.
        f.debug_struct("State")
            .field("flags", &self.flags)
            .field("client", &self.client)
            .field("thread_id", &self.thread_id)
            .field("active_turn", &self.active_turn)
            .field("transcript", &self.transcript)
            .field("markdowns", &self.markdowns)
            .field("stream", &self.stream)
            .field("approvals", &self.approvals)
            .field("questions", &self.questions)
            .field("question_drafts", &self.question_drafts)
            .field("elicitations", &self.elicitations)
            .field("elicitation_drafts", &self.elicitation_drafts)
            .field("elicitation_error", &self.elicitation_error)
            .field("sessions", &self.sessions)
            .field("archived", &self.archived)
            .field("archived_open", &self.archived_open)
            .field("status_board", &self.status_board)
            .field("settings", &self.settings)
            .field("skills", &self.skills)
            .field("codex_home", &self.codex_home)
            .field("attachments", &self.attachments)
            .field("tree_root", &self.tree.root_path())
            .field("editor_tabs", &self.editor.tabs.len())
            .field("sidebar_tab", &self.sidebar_tab)
            .field("mode", &self.mode)
            .field("left_rail_open", &self.left_rail_open)
            .field("artifacts", &self.artifacts)
            .field("menu", &self.menu)
            .field("user_times", &self.user_times.len())
            .field("copied_key", &self.copied_key)
            .field("hovered_message", &self.hovered_message)
            .field("text_selection", &self.text_selection)
            .field(
                "editing",
                &self.editing.as_ref().map(|draft| draft.message_id.clone()),
            )
            .field("pending_edit", &self.pending_edit)
            .field("reactions", &self.reactions)
            .field("expanded_commands", &self.expanded_commands.len())
            .field("recents", &self.recents.entries().len())
            .field("git_branch", &self.git.branch)
            .field(
                "token_usage",
                &self
                    .token_usage
                    .as_ref()
                    .map(|usage| usage.total.total_tokens),
            )
            .field("turn_diff", &self.turn_diff.as_ref().map(|diff| diff.len()))
            .field(
                "diff_overlay",
                &(self.diff_overlay_open, self.diff_selected),
            )
            .field("plan_overlay", &self.plan_overlay_open)
            .field("git_overlay", &self.git_overlay)
            .field("prompt_optimize", &self.prompt_optimize)
            .field("voice_input", &self.voice_input)
            .field("palette", &self.palette)
            .field("chat_search", &self.chat_search)
            .field("quest", &self.quest)
            .field("pins", &self.pins.id_set().len())
            .field("sidebar_filter", &self.sidebar_filter)
            .field(
                "expanded_command_groups",
                &self.expanded_command_groups.len(),
            )
            .field("composer", &self.composer)
            .field("mentions", &self.mentions)
            .field("collapsed_reasoning", &self.collapsed_reasoning.len())
            .field("expanded_mcp", &self.expanded_mcp.len())
            .field("mcp_servers", &self.mcp_servers.len())
            .field("mcp_login", &self.mcp_login)
            .field("scheduler_tasks", &self.scheduler.tasks.len())
            .field("knowledge_entries", &self.knowledge.entries.len())
            .field("plugins", &self.plugin_market.plugins.len())
            .field("harness_sessions", &self.harness.sessions.len())
            .field("scheduler_panel", &self.scheduler_panel)
            .field("knowledge_panel", &self.knowledge_panel)
            .field("plugin_market_panel", &self.plugin_market_panel)
            .field("harness_panel", &self.harness_panel)
            .field("status", &self.status)
            .field("connection_epoch", &self.connection_epoch)
            .finish()
    }
}

impl State {
    /// Initial state for the given launch flags.
    pub fn new(flags: Flags) -> Self {
        Self {
            flags,
            client: None,
            thread_id: None,
            active_turn: None,
            transcript: Transcript::default(),
            markdowns: HashMap::new(),
            stream: MarkdownStream::default(),
            approvals: Approvals::default(),
            questions: Questions::default(),
            question_drafts: HashMap::new(),
            elicitations: Elicitations::default(),
            elicitation_drafts: HashMap::new(),
            elicitation_error: None,
            sessions: Sessions::default(),
            archived: Sessions::default(),
            archived_open: false,
            archived_loaded: false,
            status_board: StatusBoard::default(),
            settings: Settings::default(),
            skills: SkillsBoard::default(),
            codex_home: None,
            attachments: Attachments::default(),
            tree: DirectoryTree::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            editor: EditorPane::default(),
            // Qoder's editor shell opens on the file tree, not the chat list.
            sidebar_tab: SidebarTab::Files,
            mode: AppMode::Editor,
            left_rail_open: true,
            artifacts: QuestArtifacts::default(),
            menu: None,
            user_times: HashMap::new(),
            copied_key: None,
            hovered_message: None,
            text_selection: None,
            selection_epoch: 0,
            editing: None,
            pending_edit: None,
            reactions: HashMap::new(),
            expanded_commands: BTreeSet::new(),
            recents: RecentProjects::load(RecentProjects::default_path()),
            git: GitInfo::default(),
            vendors: VendorCatalog::skeleton(),
            model_menu_open: false,
            model_switch_pending: false,
            active_binding: None,
            pending_model_apply: None,
            remote: RemotePane::default(),
            search_replace: String::new(),
            extension_filter: String::new(),
            token_usage: None,
            turn_diff: None,
            diff_overlay_open: false,
            diff_selected: None,
            plan_overlay_open: false,
            git_overlay: GitOverlayState::default(),
            prompt_optimize: PromptOptimizeState::default(),
            voice_input: VoiceInputState::default(),
            palette: PaletteState::default(),
            chat_search: ChatSearch::default(),
            quest: QuestOverlays::default(),
            pins: PinnedThreads::load(PinnedThreads::default_path()),
            sidebar_filter: String::new(),
            expanded_command_groups: BTreeSet::new(),
            composer: String::new(),
            composer_draft: iced::widget::text_editor::Content::new(),
            shift_down: false,
            approval_policy: codex_gui_core::ApprovalPolicy::load_or_default(),
            scheduler: Scheduler::load_or_default(),
            knowledge: KnowledgeBase::load_or_default(),
            plugin_market: PluginMarket::load_or_default(),
            harness: HarnessBoard::load_or_default(),
            scheduler_panel: SchedulerPanel::default(),
            knowledge_panel: KnowledgePanel::default(),
            plugin_market_panel: PluginMarketPanel::default(),
            harness_panel: HarnessPanel::default(),
            mentions: Mentions::default(),
            collapsed_reasoning: BTreeSet::new(),
            expanded_mcp: BTreeSet::new(),
            mcp_servers: Vec::new(),
            mcp_loaded: false,
            mcp_login: None,
            status: Status::Bootstrapping,
            connection_epoch: 0,
        }
    }

    /// Whether the composer may submit a new turn: text, pending image
    /// attachments, or both - and no composer-side automation (prompt
    /// optimizer, dictation) holding the draft.
    /// Replaces the composer text and mirrors it into the editor buffer
    /// so the widget and the wire-side String stay in lockstep.
    pub fn set_composer(&mut self, text: impl Into<String>) {
        self.composer = text.into();
        self.composer_draft = iced::widget::text_editor::Content::with_text(&self.composer);
    }

    pub fn can_submit(&self) -> bool {
        self.client.is_some()
            && self.thread_id.is_some()
            && matches!(self.status, Status::Ready)
            && !self.prompt_optimize.running()
            && !self.voice_input.active()
            && (!self.composer.trim().is_empty() || !self.attachments.images.is_empty())
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
