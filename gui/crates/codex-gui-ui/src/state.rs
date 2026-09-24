//! The GUI state and its connection lifecycle.

use crate::attachments::Attachments;
use crate::message::AppMode;
use crate::message::MenuId;
use crate::message::QuestScenario;
use crate::message::SidebarTab;
use codex_app_server_protocol::ThreadTokenUsage;
use codex_gui_bridge::Client;
use codex_gui_bridge::Flags;
use codex_gui_core::Approvals;
use codex_gui_core::GitInfo;
use codex_gui_core::MarkdownStream;
use codex_gui_core::PinnedThreads;
use codex_gui_core::RecentProjects;
use codex_gui_core::Sessions;
use codex_gui_core::Settings;
use codex_gui_core::SkillsBoard;
use codex_gui_core::StatusBoard;
use codex_gui_core::Transcript;
use iced::widget::markdown;
use iced_swdir_tree::DirectoryTree;
use std::collections::BTreeSet;
use std::collections::HashMap;
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

/// Everything the update loop and the views need.
pub struct State {
    /// Launch flags of the app-server connection (subscription identity).
    pub flags: Flags,
    /// Handle onto the backend; delivered through
    /// [`GuiEvent::Connected`](codex_gui_bridge::GuiEvent::Connected).
    pub client: Option<Client>,
    /// Active thread once bootstrap finished.
    pub thread_id: Option<String>,
    /// Conversation model.
    pub transcript: Transcript,
    /// Parsed markdown per agent-message item id, grown incrementally via
    /// [`markdown::Content::push_str`](iced::widget::markdown::Content::push_str).
    pub markdowns: HashMap<String, markdown::Content>,
    /// Streaming deltas waiting for the next render tick.
    pub stream: MarkdownStream,
    /// Server-initiated approvals awaiting a decision.
    pub approvals: Approvals,
    /// Sidebar inventory of resumable threads.
    pub sessions: Sessions,
    /// Status bar domain: model catalog, login badge, error banner.
    pub status_board: StatusBoard,
    /// Settings panel domain: config drafts and save lifecycle.
    pub settings: Settings,
    /// Skills domain: the flattened inventory and the picker visibility.
    pub skills: SkillsBoard,
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
    /// The open top-menu dropdown, when any.
    pub menu: Option<MenuId>,
    /// Arrival time (unix seconds) per user-message entry id, stamped by
    /// the update loop for the transcript timestamps.
    pub user_times: HashMap<String, i64>,
    /// The message id whose copy button shows "copied" feedback.
    pub copied_id: Option<String>,
    /// Like/dislike per agent-message id; absent means no reaction.
    pub reactions: HashMap<String, crate::message::Reaction>,
    /// Ids of command tool cards currently showing their output well.
    pub expanded_commands: BTreeSet<String>,
    /// Persisted recent-project list behind the welcome page.
    pub recents: RecentProjects,
    /// Working-tree facts of the active project (branch, changes).
    pub git: GitInfo,
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
    /// Connection/turn lifecycle for the status bar.
    pub status: Status,
    /// Bumped on every user-requested relaunch; part of the subscription
    /// identity so a new epoch spawns a fresh backend connection.
    pub connection_epoch: u64,
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

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `DirectoryTree` is not `Debug`; project it down to its root.
        f.debug_struct("State")
            .field("flags", &self.flags)
            .field("client", &self.client)
            .field("thread_id", &self.thread_id)
            .field("transcript", &self.transcript)
            .field("markdowns", &self.markdowns)
            .field("stream", &self.stream)
            .field("approvals", &self.approvals)
            .field("sessions", &self.sessions)
            .field("status_board", &self.status_board)
            .field("settings", &self.settings)
            .field("skills", &self.skills)
            .field("attachments", &self.attachments)
            .field("tree_root", &self.tree.root_path())
            .field("editor_tabs", &self.editor.tabs.len())
            .field("sidebar_tab", &self.sidebar_tab)
            .field("mode", &self.mode)
            .field("menu", &self.menu)
            .field("user_times", &self.user_times.len())
            .field("copied_id", &self.copied_id)
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
            transcript: Transcript::default(),
            markdowns: HashMap::new(),
            stream: MarkdownStream::default(),
            approvals: Approvals::default(),
            sessions: Sessions::default(),
            status_board: StatusBoard::default(),
            settings: Settings::default(),
            skills: SkillsBoard::default(),
            attachments: Attachments::default(),
            tree: DirectoryTree::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            editor: EditorPane::default(),
            // Qoder's editor shell opens on the file tree, not the chat list.
            sidebar_tab: SidebarTab::Files,
            mode: AppMode::Editor,
            menu: None,
            user_times: HashMap::new(),
            copied_id: None,
            reactions: HashMap::new(),
            expanded_commands: BTreeSet::new(),
            recents: RecentProjects::load(RecentProjects::default_path()),
            git: GitInfo::default(),
            token_usage: None,
            turn_diff: None,
            diff_overlay_open: false,
            diff_selected: None,
            plan_overlay_open: false,
            git_overlay: GitOverlayState::default(),
            palette: PaletteState::default(),
            chat_search: ChatSearch::default(),
            quest: QuestOverlays::default(),
            pins: PinnedThreads::load(PinnedThreads::default_path()),
            sidebar_filter: String::new(),
            expanded_command_groups: BTreeSet::new(),
            composer: String::new(),
            status: Status::Bootstrapping,
            connection_epoch: 0,
        }
    }

    /// Whether the composer may submit a new turn: text, pending image
    /// attachments, or both.
    pub fn can_submit(&self) -> bool {
        self.client.is_some()
            && self.thread_id.is_some()
            && matches!(self.status, Status::Ready)
            && (!self.composer.trim().is_empty() || !self.attachments.images.is_empty())
    }
}
