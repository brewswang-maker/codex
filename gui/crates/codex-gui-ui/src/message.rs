//! Messages flowing through the iced update loop.

use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerOauthLoginResponse;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginReadResponse;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::ThreadListResponse;
use codex_gui_bridge::Error;
use codex_gui_bridge::GuiEvent;
use codex_gui_core::Decision;
use codex_gui_core::GitInfo;
use codex_gui_core::SessionHistory;
use codex_gui_core::SettingField;
use codex_gui_core::SkillsTab;
use iced::widget::markdown;
use iced_swdir_tree::DirectoryTreeEvent;
use serde_json::Value;
use std::path::PathBuf;

/// Which sidebar pane the activity bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    /// The resumable-thread list.
    Threads,
    /// The working-directory tree.
    Files,
    /// The search pane.
    Search,
    /// The source-control (git changes) pane.
    SourceControl,
    /// The repository wiki pane.
    Wiki,
    /// The run-and-debug pane.
    Debug,
    /// The remote-explorer pane.
    Remote,
    /// The extensions pane.
    Extensions,
}

/// Which shell the window shows: the editor chrome or the Quest layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Editor mode: menu bar, activity rail, sidebar panes.
    Editor,
    /// Quest mode: quest list rail plus the task dialog.
    Quest,
}

/// Which native picker the Skills page's import button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillImportTarget {
    /// A whole skill directory holding a `SKILL.md`.
    Directory,
    /// A single `SKILL.md`-shaped file.
    File,
}

/// The scenario picked when starting a Quest; the launch template seeds
/// the composer so the first turn carries the intent (Qoder's scenario
/// menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestScenario {
    /// Let the agent decide from the first prompt.
    Auto,
    /// Spec-driven: agree on a spec first, then implement.
    Spec,
    /// Prototype: explore a throwaway prototype first.
    Prototype,
    /// Tool building: create a reusable tool or script.
    Tool,
}

impl QuestScenario {
    /// The picker order.
    pub const ALL: [QuestScenario; 4] = [
        QuestScenario::Auto,
        QuestScenario::Spec,
        QuestScenario::Prototype,
        QuestScenario::Tool,
    ];

    /// The visible title.
    pub fn title(self) -> &'static str {
        match self {
            QuestScenario::Auto => "自动判断",
            QuestScenario::Spec => "Spec 驱动",
            QuestScenario::Prototype => "原型探索",
            QuestScenario::Tool => "创建工具",
        }
    }

    /// The one-line explanation under the title.
    pub fn hint(self) -> &'static str {
        match self {
            QuestScenario::Auto => "根据首条消息自动选择处理方式",
            QuestScenario::Spec => "先达成规格，再按规格实现",
            QuestScenario::Prototype => "先快速验证原型，再决定是否完善",
            QuestScenario::Tool => "构建可复用的工具或脚本",
        }
    }

    /// The composer seed; blank for [`QuestScenario::Auto`].
    pub fn template(self) -> &'static str {
        match self {
            QuestScenario::Auto => "",
            QuestScenario::Spec => "请先为以下任务写一份实现规格，确认后再按规格实现：\n",
            QuestScenario::Prototype => "请先做一个可快速验证的原型来探索这个想法：\n",
            QuestScenario::Tool => "请帮我构建一个可复用的工具/脚本，需求：\n",
        }
    }
}

/// One dropdown of the VS Code-style main menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    Selection,
    View,
    Go,
    Run,
    Terminal,
    Help,
}

impl MenuId {
    /// The eight menus in reference order.
    pub const ALL: [MenuId; 8] = [
        MenuId::File,
        MenuId::Edit,
        MenuId::Selection,
        MenuId::View,
        MenuId::Go,
        MenuId::Run,
        MenuId::Terminal,
        MenuId::Help,
    ];

    /// The visible title, with the reference mnemonic hints.
    pub fn title(self) -> &'static str {
        match self {
            MenuId::File => "文件(F)",
            MenuId::Edit => "编辑(E)",
            MenuId::Selection => "选择(S)",
            MenuId::View => "查看(V)",
            MenuId::Go => "转到(G)",
            MenuId::Run => "运行(R)",
            MenuId::Terminal => "终端(T)",
            MenuId::Help => "帮助(H)",
        }
    }
}

/// Feedback on one agent message; one direction at a time per message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Up,
    Down,
}

/// Which bootstrap call a result belongs to.
#[derive(Debug, Clone)]
pub enum Bootstrap {
    /// `initialize` finished; the payload is its raw result.
    Initialized(Result<Value, Error>),
    /// `thread/start` finished; the payload carries the new thread id.
    ThreadStarted(Result<Value, Error>),
}

/// All events the GUI reacts to.
///
/// The size skew comes from `Event`'s protocol payloads versus scalar
/// variants; boxing would contaminate every match site in the update loop.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Message {
    /// Events coming from the app-server connection.
    Event(GuiEvent),
    /// Results of the startup handshake chain.
    Bootstrap(Bootstrap),
    /// The composer text changed.
    ComposerChanged(String),
    /// The user submitted the composer.
    Submit,
    /// A submitted turn was accepted by the backend.
    TurnAcked,
    /// The `turn/start` RPC settled; `prompt` and `images` are what was
    /// submitted, put back into the composer when the start itself failed.
    TurnSettled {
        prompt: String,
        images: Vec<PathBuf>,
        result: Result<Value, Error>,
    },
    /// A markdown link was clicked; the payload is the target URI.
    LinkClicked(markdown::Uri),
    /// Renders everything buffered since the previous tick.
    Tick,
    /// The user answered an approval dialog.
    ApprovalDecided {
        /// Wire id of the answered request.
        request_id: String,
        decision: Decision,
    },
    /// The user picked one option row of a question in the input dialog.
    QuestionOptionPicked {
        /// The question's wire id.
        question_id: String,
        /// Row index into the option list (including the free-choice row).
        index: usize,
    },
    /// The notes draft of one question changed.
    QuestionNotesChanged {
        /// The question's wire id.
        question_id: String,
        notes: String,
    },
    /// The user submitted the input dialog; every question answers at once.
    QuestionAnswered,
    /// One elicitation text/number field changed.
    ElicitationFieldTextChanged {
        /// The schema property key.
        key: String,
        text: String,
    },
    /// One elicitation boolean field was toggled.
    ElicitationFieldToggled {
        /// The schema property key.
        key: String,
    },
    /// One elicitation single-select value was picked.
    ElicitationFieldPicked {
        /// The schema property key.
        key: String,
        /// The picked wire value.
        value: String,
    },
    /// One elicitation multi-select value was toggled.
    ElicitationFieldValueToggled {
        /// The schema property key.
        key: String,
        /// The toggled wire value.
        value: String,
    },
    /// The user answered the elicitation dialog (accept/decline/cancel).
    ElicitationAnswered(McpServerElicitationAction),
    /// Copies one elicitation URL (or verification challenge) to the
    /// clipboard so the user can open it in a browser.
    ElicitationLinkCopied(String),
    /// A `thread/list` page arrived for the sidebar.
    SessionsLoaded(Result<ThreadListResponse, Error>),
    /// The user picked a thread in the sidebar to resume.
    SessionSelected(String),
    /// `thread/resume` finished; success carries the history to replay.
    SessionResumed(Result<SessionHistory, Error>),
    /// An in-place model switch (`thread/settings/update`) settled; the
    /// error side carries why the running thread kept its model.
    ModelAppliedInPlace(Result<Value, Error>),
    /// A cross-vendor model switch forked the running thread onto the new
    /// provider; `Ok` carries the fork's thread id to reopen.
    ModelSwitchForked(Result<String, Error>),
    /// The user asked to archive one thread.
    SessionArchived(String),
    /// The user asked to restore one archived thread.
    SessionUnarchived(String),
    /// The user expanded or collapsed the archived section.
    ArchivedToggled,
    /// A `thread/list` page arrived for the archived partition.
    ArchivedSessionsLoaded(Result<ThreadListResponse, Error>),
    /// A `model/list` page arrived for the status-bar picker.
    ModelsLoaded(Result<ModelListResponse, Error>),
    /// An `account/read` response arrived for the login badge.
    AccountLoaded(Result<GetAccountResponse, Error>),
    /// The user toggled the settings panel; opening triggers a config read.
    SettingsToggled,
    /// A `config/read` result arrived for the settings panel.
    ConfigLoaded(Result<Value, Error>),
    /// The user edited one settings field.
    SettingEdited { field: SettingField, value: String },
    /// The user asked to save the drafted settings.
    SettingsSave,
    /// A `config/batchWrite` result arrived.
    SettingsSaved(Result<Value, Error>),
    /// The user typed into the account API-key box.
    AccountApiKeyChanged(String),
    /// The user asked to sign in with the drafted API key.
    LoginSubmit,
    /// An `account/login/start` result arrived.
    LoginCompleted(Result<Value, Error>),
    /// The user asked to sign out.
    LogoutSubmit,
    /// An `account/logout` result arrived.
    LogoutCompleted(Result<Value, Error>),
    /// The user picked a provider preset in the settings panel.
    ProviderPresetSelected(String),
    /// The user picked a billing type for the drafted provider.
    ProviderBillingSelected(String),
    /// A `skills/list` response arrived for the composer picker and the
    /// settings toggles.
    SkillsLoaded(Result<SkillsListResponse, Error>),
    /// The user opened or closed the composer skill picker.
    SkillsPickerToggled,
    /// The user picked an enabled skill to mention in the composer.
    SkillPicked(String),
    /// The user flipped a skill's enabled state in the settings panel.
    SkillEnabledChanged { name: String, enabled: bool },
    /// A `skills/config/write` settled for one skill.
    SkillEnabledSaved {
        name: String,
        result: Result<Value, Error>,
    },
    /// The user opened or closed the full-screen Skills page.
    SkillsPageToggled,
    /// The user picked a half of the Skills page.
    SkillsTabPicked(SkillsTab),
    /// The user opened the add form.
    SkillAddOpened,
    /// The add form's name draft changed.
    SkillAddNameChanged(String),
    /// The add form's description draft changed.
    SkillAddDescriptionChanged(String),
    /// The add form's Git link draft changed.
    SkillAddLinkChanged(String),
    /// The user asked for the native import picker.
    SkillAddImportRequested(SkillImportTarget),
    /// The native import picker returned; `None` means it was cancelled.
    SkillAddSourcePicked(Option<PathBuf>),
    /// The user asked to create or import the drafted skill.
    SkillAddSubmitted,
    /// The user cancelled the add form.
    SkillAddCancelled,
    /// The user opened the edit form for one row.
    SkillEditOpened(String),
    /// The edit form's name draft changed.
    SkillEditNameChanged(String),
    /// The edit form's description draft changed.
    SkillEditDescriptionChanged(String),
    /// The user confirmed the edits.
    SkillEditSubmitted,
    /// The user cancelled the edit form.
    SkillEditCancelled,
    /// The user pressed one row's delete button; arms the confirmation.
    SkillDeleteArmed(String),
    /// The user dismissed the delete confirmation.
    SkillDeleteCancelled,
    /// The user confirmed removing one skill.
    SkillDeleteConfirmed(String),
    /// A skill disk mutation settled; `Ok` carries the success text.
    SkillMutationSettled(Result<String, String>),
    /// A `plugin/list` response arrived for the market tab.
    SkillMarketLoaded(Result<PluginListResponse, Error>),
    /// The user expanded or collapsed one market plugin's detail.
    SkillMarketPluginToggled(String),
    /// A `plugin/read` response arrived for the expanded plugin.
    SkillMarketDetailLoaded(Result<PluginReadResponse, Error>),
    /// The user asked to install one market plugin.
    SkillMarketInstallRequested(String),
    /// The user asked to uninstall one plugin.
    SkillUninstallRequested(String),
    /// A `plugin/install` settled; `Ok` carries the auth policy notice.
    SkillInstalled {
        /// The `name@marketplace` id the install targeted.
        id: String,
        /// The call outcome.
        result: Result<Value, Error>,
    },
    /// A `plugin/uninstall` settled.
    SkillUninstalled {
        /// The `name@marketplace` id the uninstall targeted.
        id: String,
        /// The call outcome.
        result: Result<Value, Error>,
    },
    /// The marketplace-source draft changed.
    SkillMarketSourceChanged(String),
    /// The user asked to add the drafted marketplace source.
    SkillMarketSourceSubmitted,
    /// A `marketplace/add` settled.
    SkillMarketSourceAdded(Result<Value, Error>),
    /// The user asked to refresh the market catalog.
    SkillMarketRefreshRequested,
    /// The user dismissed the error banner.
    ErrorDismissed,
    /// A file drag moved over the window.
    DragHovered,
    /// The file drag left the window.
    DragLeft,
    /// The user dropped a file onto the window; non-images are ignored.
    FileDropped(PathBuf),
    /// The user asked for the native image picker.
    AttachImages,
    /// The native picker returned; empty means it was cancelled.
    ImagesPicked(Vec<PathBuf>),
    /// The user asked to open a folder and start a session inside it.
    FolderPickRequested,
    /// The native folder picker returned; `None` means it was cancelled.
    FolderPicked(Option<PathBuf>),
    /// A directory-tree interaction happened in the Files sidebar.
    Tree(DirectoryTreeEvent),
    /// A file preview finished loading; the body is what to render.
    FilePreviewLoaded {
        /// The file the read was for.
        path: PathBuf,
        /// Loaded body (text, binary marker, or failure reason).
        body: crate::state::FileBody,
    },
    /// The user switched to an already-open preview tab.
    FilePreviewSelected(PathBuf),
    /// The user closed one preview tab.
    FilePreviewClosed(PathBuf),
    /// The user switched the sidebar between threads and files.
    SidebarToggled,
    /// The user picked a sidebar pane in the activity bar.
    SidebarTab(SidebarTab),
    /// The user asked to copy one message body to the clipboard; `id`
    /// drives the "copied" button feedback, `text` is the payload.
    CopyMessage { id: String, text: String },
    /// The user toggled like/dislike on one agent message.
    ReactionToggled { id: String, reaction: Reaction },
    /// The user expanded or collapsed one command tool card.
    CommandCardToggled(String),
    /// The user opened or closed the turn-diff overlay.
    DiffOverlayToggled,
    /// The user picked a file in the diff overlay's file list.
    DiffFileSelected(usize),
    /// The user opened or closed the plan review overlay.
    PlanOverlayToggled,
    /// The user asked to quote the live plan into the composer as a
    /// follow-up instruction.
    PlanQuoted,
    /// The user opened or closed the commit-composer overlay.
    GitOverlayToggled,
    /// The user toggled one file checkbox in the commit composer.
    GitFileToggled(usize),
    /// Select or clear every file checkbox in the commit composer.
    GitFilesSelectAll(bool),
    /// The commit message input changed.
    GitCommitDraftChanged(String),
    /// The user asked to commit the selected files.
    GitCommitRequested,
    /// The user asked for an AI-drafted commit message.
    GitAiDraftRequested,
    /// A stage-and-commit finished; the error side carries git's stderr.
    CommitFinished(Result<(), String>),
    /// The throwaway draft thread opened; ready for the draft turn.
    DraftThreadStarted {
        /// The staged diff the draft turn will describe.
        diff: String,
        /// The new thread id, or why the open failed.
        result: Result<String, codex_gui_bridge::Error>,
    },
    /// The AI draft could not even start (nothing staged, no cwd, ...).
    CommitDraftFailed(String),
    /// The user removed one pending attachment chip by position.
    AttachmentRemoved(usize),
    /// The user asked to relaunch the app-server after a disconnect.
    Reconnect,
    /// A no-op placeholder for option rows that resolve to nothing.
    Noop,
    /// The user opened or closed the command palette (Ctrl+Shift+P).
    PaletteToggled,
    /// The user opened or closed the multi-vendor model menu.
    ModelMenuToggled,
    /// The model menu was dismissed without a pick (Esc or mask click).
    ModelMenuClosed,
    /// The user picked a model row inside the vendor menu.
    VendorModelPicked {
        /// The provider key written as `model_provider`.
        provider_id: String,
        /// The model id written as `model`.
        slug: String,
    },
    /// The user asked for a fresh conversation thread.
    NewThreadRequested,
    /// The palette query text changed.
    PaletteQueryChanged(String),
    /// The user moved the palette selection (`delta` is +1/-1).
    PaletteMoved(i32),
    /// The user confirmed the highlighted palette action.
    PaletteConfirmed,
    /// A git probe finished for the working directory.
    GitInfoArrived(GitInfo),
    /// The workspace search query changed (filters the directory tree).
    SearchQueryChanged(String),
    /// The workspace replace-with draft text changed.
    SearchReplaceChanged(String),
    /// The extension-marketplace filter text changed.
    ExtensionFilterChanged(String),
    /// A remote-explorer probe finished: SSH host aliases plus whether a
    /// working Docker daemon answered (`None` when it could not run).
    RemoteInfoArrived {
        targets: Vec<String>,
        docker_available: Option<bool>,
    },
    /// The user opened the transcript search bar (Ctrl+F).
    ChatSearchOpened,
    /// The user closed the transcript search bar.
    ChatSearchClosed,
    /// The transcript search query changed.
    ChatSearchQueryChanged(String),
    /// The user stepped to the next transcript hit.
    ChatSearchNext,
    /// The user stepped to the previous transcript hit.
    ChatSearchPrev,
    /// The user expanded or collapsed one group of consecutive commands.
    CommandGroupToggled(String),
    /// The sidebar thread-filter text changed.
    SidebarFilterChanged(String),
    /// The user pinned or unpinned one thread.
    SessionPinToggled(String),
    /// The user opened, switched, or closed a top-menu dropdown.
    MenuToggled(MenuId),
    /// The open top-menu dropdown was dismissed (Esc or a chosen item).
    MenuClosed,
    /// The user switched between the editor and Quest shells.
    ModeToggled,
    /// The user pressed the Quest rail's create button; raises the scenario
    /// picker.
    QuestCreatePressed,
    /// The user picked a scenario for the new Quest; `None` closes the
    /// picker without starting anything.
    QuestScenarioPicked(Option<QuestScenario>),
    /// The user toggled one quest row's action menu (payload: thread id).
    QuestMenuToggled(String),
    /// The user dismissed the open quest action menu.
    QuestMenuClosed,
    /// The user pressed the menu's delete entry once; arms the
    /// confirmation.
    QuestDeleteArmed,
    /// The user confirmed deleting the quest from the action menu.
    QuestDeleteRequested(String),
    /// The user asked to fork one quest; the payload is the source thread
    /// id.
    QuestForkRequested(String),
    /// The `thread/fork` call settled; `Ok` carries the new thread id.
    QuestForked {
        /// The thread the fork started from.
        source: String,
        /// The new thread id, or why the fork failed.
        result: Result<String, Error>,
    },
    /// The user asked to rename one quest; opens the dialog.
    QuestRenameRequested(String),
    /// The rename dialog draft changed.
    QuestRenameDraftChanged(String),
    /// The user confirmed the rename draft.
    QuestRenameConfirmed,
    /// The user cancelled the rename dialog.
    QuestRenameCancelled,
    /// A `thread/name/set` call settled.
    QuestRenamed {
        /// Thread the rename targeted.
        thread_id: String,
        /// The name that was sent.
        name: String,
        /// The call outcome.
        result: Result<Value, Error>,
    },
    /// The user opened or closed the My Quests board overlay.
    QuestBoardToggled,
    /// The launch page's workspace chip toggled its project dropdown.
    QuestWorkspaceToggled,
    /// The rail's show-more hint expanded (or collapsed) the quest list.
    QuestListExpanded,
    /// The board search query changed.
    QuestBoardFilterChanged(String),
    /// The user picked a board project tab; `None` is "all projects".
    QuestBoardProjectPicked(Option<String>),
    /// Dismisses every open Quest overlay (Esc, mode switches).
    QuestOverlaysClosed,
    /// The user cleared the composer buffer from the Edit menu.
    ComposerCleared,
    /// The user asked for a fresh git probe from the Run menu.
    GitRefreshRequested,
    /// The user asked to interrupt the streaming turn (the stop button).
    TurnInterruptRequested,
    /// The `turn/interrupt` RPC settled.
    TurnInterrupted(Result<Value, Error>),
    /// An `@` mention search page arrived; the echoed query guards against
    /// stale pages that resolve after the token changed.
    MentionSearchLoaded {
        /// The query the page was searched for.
        query: String,
        /// The call outcome.
        result: Result<codex_app_server_protocol::FuzzyFileSearchResponse, Error>,
    },
    /// The user clicked one mention row.
    MentionPicked(usize),
    /// The user stepped the mention highlight (`delta` is +1/-1).
    MentionMoved(i32),
    /// The user dismissed the mention popup (Esc).
    MentionClosed,
    /// The user expanded or collapsed one reasoning card.
    ReasoningToggled(String),
    /// The user expanded or collapsed one MCP call card.
    McpCardToggled(String),
    /// A `mcpServerStatus/list` page arrived for the settings panel.
    McpServersLoaded(Result<codex_app_server_protocol::ListMcpServerStatusResponse, Error>),
    /// The user asked to start the OAuth login for one MCP server.
    McpLoginRequested(String),
    /// A `mcpServer/oauth/login` call settled; `Ok` carries the URL the
    /// user must open to finish signing in.
    McpLoginStarted {
        /// The MCP server the login targets.
        name: String,
        /// The sign-in URL, or why the login could not start.
        result: Result<McpServerOauthLoginResponse, Error>,
    },
    /// The user asked to copy the pending sign-in link.
    McpLoginLinkCopied,
    /// The user dismissed the pending sign-in banner.
    McpLoginDismissed,
    /// A `review/start` call settled.
    ReviewStarted(Result<Value, Error>),
    /// A `thread/compact/start` call settled.
    CompactionStarted(Result<Value, Error>),
    /// The user clicked one `/` command row; the payload is the command.
    SlashPicked(String),
}
