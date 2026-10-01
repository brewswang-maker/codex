//! Outgoing app-server requests: every [`Task`] the UI produces to talk to
//! the app-server lives here, keeping `update.rs` a pure dispatcher.

use crate::message::Bootstrap;
use crate::message::Message;
use crate::message::SkillImportTarget;
use crate::state::State;
use crate::state::Status;
use crate::state::VoiceInputStatus;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::FuzzyFileSearchParams;
use codex_app_server_protocol::FuzzyFileSearchResponse;
use codex_app_server_protocol::GetAccountParams;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::ListMcpServerStatusParams;
use codex_app_server_protocol::ListMcpServerStatusResponse;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::MarketplaceAddParams;
use codex_app_server_protocol::McpServerOauthLoginParams;
use codex_app_server_protocol::McpServerOauthLoginResponse;
use codex_app_server_protocol::McpServerStatusDetail;
use codex_app_server_protocol::MergeStrategy;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::PluginInstallParams;
use codex_app_server_protocol::PluginListParams;
use codex_app_server_protocol::PluginListResponse;
use codex_app_server_protocol::PluginReadParams;
use codex_app_server_protocol::PluginReadResponse;
use codex_app_server_protocol::PluginUninstallParams;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartParams;
use codex_app_server_protocol::ReviewTarget;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SkillsConfigWriteParams;
use codex_app_server_protocol::SkillsListParams;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::SortDirection;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadCompactStartParams;
use codex_app_server_protocol::ThreadDeleteParams;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadResumeInitialTurnsPageParams;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadSettingsUpdateParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadTurnsListParams;
use codex_app_server_protocol::ThreadTurnsListResponse;
use codex_app_server_protocol::ThreadUnarchiveParams;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnInterruptParams;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use codex_gui_bridge::Error;
use codex_gui_core::Approvals;
use codex_gui_core::Decision;
use codex_gui_core::MarkdownStream;
use codex_gui_core::MarketSelector;
use codex_gui_core::SessionHistory;
use codex_gui_core::compose_inputs;
use codex_utils_absolute_path::AbsolutePathBuf;
use iced::Task;
use iced::futures::SinkExt as _;
use std::path::PathBuf;

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

/// Handshakes with the app-server; the reply drives [`Bootstrap::Initialized`].
pub(crate) fn initialize(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::Initialize {
                request_id,
                params: initialize_params(),
            })
            .await;
        Message::Bootstrap(Bootstrap::Initialized(result))
    })
}

fn initialize_params() -> InitializeParams {
    InitializeParams {
        client_info: ClientInfo {
            name: "codex-gui".to_string(),
            title: Some("Codex GUI".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        // The model menu flips same-vendor picks on the running thread
        // through `thread/settings/update`, which is an experimental RPC;
        // the client opts in so those calls and their notifications pass.
        capabilities: Some(InitializeCapabilities {
            experimental_api: true,
            ..InitializeCapabilities::default()
        }),
    }
}

/// `thread/start` payload; `cwd` pins the session to a project directory.
fn thread_start_params(cwd: Option<String>) -> ThreadStartParams {
    ThreadStartParams {
        cwd,
        // The server default for untrusted projects is `UnlessTrusted`, which
        // prompts on every patch, and the session-scoped "always allow" memory
        // only covers the same file. Declaring the policy here lets workspace
        // edits run without per-file approval while keeping prompts for
        // anything outside the writable sandbox.
        approval_policy: Some(AskForApproval::OnRequest),
        sandbox: Some(SandboxMode::WorkspaceWrite),
        ..ThreadStartParams::default()
    }
}

/// Page size for the paginated resume bootstrap; it matches the server's
/// own turn page cap (100).
const RESUME_TURNS_PAGE_LIMIT: u32 = 100;

/// Resumes a thread with the same approval posture as
/// [`thread_start_params`]: without it, an older thread keeps the
/// server-default policy that prompts on every patch. History is
/// bootstrapped through `initialTurnsPage` instead of the deprecated
/// full-history hydration; [`collect_resume_turns`] pages the rest.
fn thread_resume_params(thread_id: String) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id,
        approval_policy: Some(AskForApproval::OnRequest),
        sandbox: Some(SandboxMode::WorkspaceWrite),
        exclude_turns: true,
        initial_turns_page: Some(ThreadResumeInitialTurnsPageParams {
            limit: Some(RESUME_TURNS_PAGE_LIMIT),
            sort_direction: Some(SortDirection::Asc),
            items_view: Some(TurnItemsView::Full),
        }),
        ..ThreadResumeParams::default()
    }
}

/// Opens the bootstrap thread; the reply drives [`Bootstrap::ThreadStarted`].
pub(crate) fn start_thread(state: &State) -> Task<Message> {
    start_thread_in(state, None)
}

/// Opens the bootstrap thread, optionally pinned to a project directory.
pub(crate) fn start_thread_in(state: &State, cwd: Option<String>) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params(cwd),
            })
            .await;
        Message::Bootstrap(Bootstrap::ThreadStarted(result))
    })
}

/// Opens a fresh conversation thread: config edits only apply to threads
/// started after they are saved, so model-setup saves restart the
/// conversation on the new settings.
pub(crate) fn restart_thread(state: &mut State) -> Task<Message> {
    restart_thread_in(state, None)
}

/// Drops the live conversation and starts a fresh one, optionally inside a
/// project directory chosen through the native folder picker.
pub(crate) fn restart_thread_in(state: &mut State, cwd: Option<String>) -> Task<Message> {
    state.status = Status::Bootstrapping;
    state.transcript.replay(&[]);
    state.markdowns.clear();
    state.stream = MarkdownStream::default();
    state.thread_id = None;
    state.active_turn = None;
    // The old thread's binding dies with it; the replacement reports
    // its own pair through `ThreadStarted`.
    state.active_binding = None;
    start_thread_in(state, cwd)
}

/// Sends the composer text plus any pending image attachments and `$skill`
/// mentions as a new turn.
pub(crate) fn submit(state: &mut State) -> Task<Message> {
    if !state.can_submit() {
        return Task::none();
    }

    let text = std::mem::take(&mut state.composer);
    let images = std::mem::take(&mut state.attachments.images);
    submit_text(state, text, images)
}

/// Sends `text` plus attachments as a new turn on the active thread,
/// bypassing the composer buffer; the composer submit and the
/// edit-and-resubmit flow share this path. The reply drives
/// [`Message::TurnSettled`].
pub(crate) fn submit_text(state: &mut State, text: String, images: Vec<PathBuf>) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.thread_id.clone() else {
        return Task::none();
    };

    state.status = Status::Thinking;
    let prompt = text.clone();
    let submitted_images = images.clone();
    let input = compose_inputs(&text, &images, &state.skills);

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::TurnStart {
                    request_id,
                    params: TurnStartParams {
                        thread_id,
                        input,
                        ..TurnStartParams::default()
                    },
                })
                .await;
            // The turn itself is tracked through notifications; this result
            // only tells whether the start itself succeeded.
            Message::TurnSettled {
                prompt,
                images: submitted_images,
                result,
            }
        },
        |settled| settled,
    )
}

/// Opens the native image picker; the selection drives
/// [`Message::ImagesPicked`] and cancellation arrives as an empty list.
pub(crate) fn attach_images() -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title("Attach images")
        .add_filter(
            "Images",
            &[
                "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff",
            ],
        );

    Task::future(async move {
        let picked = dialog
            .pick_files()
            .await
            .unwrap_or_default()
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect::<Vec<PathBuf>>();
        Message::ImagesPicked(picked)
    })
}

/// Opens the native folder picker; the selection drives
/// [`Message::FolderPicked`] and cancellation arrives as `None`.
pub(crate) fn pick_folder() -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new().set_title("Open project folder");

    Task::future(async move {
        let picked = dialog
            .pick_folder()
            .await
            .map(|folder| folder.path().to_path_buf());
        Message::FolderPicked(picked)
    })
}

/// Sends the user's decision for one approval and drops the dialog.
pub(crate) fn decide_approval(
    state: &mut State,
    request_id: String,
    decision: Decision,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(approval) = state.approvals.take(&request_id) else {
        // The server resolved it while the dialog was open; nothing to send.
        return Task::none();
    };

    let payload = Approvals::response_payload(&approval, decision);

    Task::perform(
        async move {
            let _ignored = client.respond_for_id(approval.request_id, payload).await;
        },
        |()| Message::TurnAcked,
    )
}

/// Sends the payload answering one server-initiated request (agent
/// questions or an MCP elicitation); the queue entry was taken by the
/// caller before the dialog fell away.
pub(crate) fn respond_to_request(
    state: &State,
    request_id: RequestId,
    payload: serde_json::Value,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            if let Err(error) = client.respond_for_id(request_id, payload).await {
                tracing::warn!(%error, "answering server request failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Starts the OAuth sign-in flow for one MCP server; the reply carries the
/// URL the user must open to finish signing in.
pub(crate) fn mcp_login(state: &State, name: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let thread_id = state.thread_id.clone();

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::McpServerOauthLogin {
                    request_id,
                    params: McpServerOauthLoginParams {
                        name: name.clone(),
                        thread_id,
                        client_registration: None,
                        scopes: None,
                        timeout_secs: None,
                    },
                })
                .await
                .and_then(|value| {
                    serde_json::from_value::<McpServerOauthLoginResponse>(value)
                        .map_err(Error::Json)
                });
            Message::McpLoginStarted { name, result }
        },
        |msg| msg,
    )
}

/// Refreshes the sidebar inventory from the first `thread/list` page.
pub(crate) fn load_sessions(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadList {
                request_id,
                params: thread_list_params(None),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadListResponse>(value).map_err(Error::Json)
            });
        Message::SessionsLoaded(result)
    })
}

/// Refreshes the archived inventory behind the sidebar's collapsible
/// section; the reply drives [`Message::ArchivedSessionsLoaded`].
pub(crate) fn load_archived_sessions(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadList {
                request_id,
                params: thread_list_params(Some(true)),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadListResponse>(value).map_err(Error::Json)
            });
        Message::ArchivedSessionsLoaded(result)
    })
}

/// A `thread/list` page in server-default order (newest first); `archived`
/// selects the archived partition instead of the live one.
fn thread_list_params(archived: Option<bool>) -> ThreadListParams {
    ThreadListParams {
        cursor: None,
        limit: None,
        sort_key: None,
        sort_direction: None,
        // An explicit empty list means "every provider". Omitting the field
        // would make the server filter to the *current* config provider,
        // hiding threads recorded under another vendor right after a model
        // switch.
        model_providers: Some(Vec::new()),
        source_kinds: None,
        originators: None,
        archived,
        section_id: None,
        project_id: None,
        cwd: None,
        use_state_db_only: false,
        search_term: None,
        parent_thread_id: None,
        ancestor_thread_id: None,
    }
}

/// Loads the model catalog for the status-bar picker.
pub(crate) fn load_models(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ModelList {
                request_id,
                params: ModelListParams::default(),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ModelListResponse>(value).map_err(Error::Json)
            });
        Message::ModelsLoaded(result)
    })
}

/// Loads the login state for the status-bar badge.
pub(crate) fn load_account(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::GetAccount {
                request_id,
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<GetAccountResponse>(value).map_err(Error::Json)
            });
        Message::AccountLoaded(result)
    })
}

/// Switches to the picked thread: clears the panes, then replays the
/// persisted turns once the resume response arrives.
pub(crate) fn resume_thread(state: &mut State, thread_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    state.status = Status::Bootstrapping;
    state.transcript.replay(&[]);
    state.markdowns.clear();
    state.stream = MarkdownStream::default();
    state.thread_id = Some(thread_id.clone());
    // Until the resume answer lands the thread's binding is unknown; a
    // stale binding could misroute an in-flight model pick.
    state.active_binding = None;

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadResume {
                    request_id,
                    params: thread_resume_params(thread_id),
                })
                .await;
            match result {
                Err(error) => Err(error),
                Ok(value) => {
                    let response: ThreadResumeResponse =
                        serde_json::from_value(value).map_err(Error::Json)?;
                    let turns = collect_resume_turns(&client, &response).await;
                    Ok(SessionHistory::from_resume(&response, turns))
                }
            }
        },
        Message::SessionResumed,
    )
}

/// Replays a resumed thread's stored history: the bootstrap page from
/// `initialTurnsPage` followed by every `thread/turns/list` continuation.
/// A failing continuation keeps the turns already collected so the
/// transcript still renders what was loaded.
async fn collect_resume_turns(
    client: &codex_gui_bridge::Client,
    response: &ThreadResumeResponse,
) -> Vec<Turn> {
    let mut turns = match response.initial_turns_page.as_ref() {
        Some(page) => page.data.clone(),
        None => response.thread.turns.clone(),
    };
    let mut cursor = response
        .initial_turns_page
        .as_ref()
        .and_then(|page| page.next_cursor.clone());
    while let Some(page_cursor) = cursor.take() {
        let result = client
            .call(|request_id| ClientRequest::ThreadTurnsList {
                request_id,
                params: ThreadTurnsListParams {
                    thread_id: response.thread.id.clone(),
                    cursor: Some(page_cursor.clone()),
                    limit: Some(RESUME_TURNS_PAGE_LIMIT),
                    sort_direction: Some(SortDirection::Asc),
                    items_view: Some(TurnItemsView::Full),
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadTurnsListResponse>(value).map_err(Error::Json)
            });
        let page = match result {
            Ok(page) => page,
            Err(error) => {
                tracing::warn!(%error, "thread/turns/list continuation failed");
                break;
            }
        };
        if page.next_cursor.as_deref() == Some(page_cursor.as_str()) {
            break;
        }
        turns.extend(page.data);
        cursor = page.next_cursor;
    }
    turns
}

/// Archives one thread; the caller chains a sidebar refresh afterwards.
pub(crate) fn archive_thread(state: &State, thread_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadArchive {
                    request_id,
                    params: ThreadArchiveParams { thread_id },
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "thread/archive failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Restores one archived thread through `thread/unarchive`; the caller
/// chains the list refreshes that move the row back into the sidebar.
pub(crate) fn unarchive_thread(state: &State, thread_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadUnarchive {
                    request_id,
                    params: ThreadUnarchiveParams { thread_id },
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "thread/unarchive failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Forks one quest into a new thread carrying its history; the reply
/// drives [`Message::QuestForked`].
pub(crate) fn fork_thread(state: &State, source: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadFork {
                    request_id,
                    params: ThreadForkParams {
                        thread_id: source.clone(),
                        // The reply only needs the new thread id; history
                        // replays through the resume bootstrap page.
                        exclude_turns: true,
                        ..ThreadForkParams::default()
                    },
                })
                .await
                .and_then(|value| {
                    serde_json::from_value::<ThreadForkResponse>(value)
                        .map(|response| response.thread.id)
                        .map_err(Error::Json)
                });
            Message::QuestForked { source, result }
        },
        |msg| msg,
    )
}

/// Forks the running thread onto another provider/model pair; the fork
/// carries the conversation's history, so a vendor switch keeps the
/// transcript instead of reopening an empty thread. The reply drives
/// [`Message::ModelSwitchForked`].
pub(crate) fn fork_thread_with_model(
    state: &State,
    thread_id: String,
    provider_id: String,
    model: String,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadFork {
                    request_id,
                    params: ThreadForkParams {
                        thread_id,
                        model: Some(model),
                        model_provider: Some(provider_id),
                        // The reply only needs the new thread id; history
                        // replays through the resume bootstrap page.
                        exclude_turns: true,
                        ..ThreadForkParams::default()
                    },
                })
                .await
                .and_then(|value| {
                    serde_json::from_value::<ThreadForkResponse>(value)
                        .map(|response| response.thread.id)
                        .map_err(Error::Json)
                });
            Message::ModelSwitchForked(result)
        },
        |msg| msg,
    )
}

/// Forks the running thread up to (but not including) one turn, dropping
/// that turn and everything after it so an edited message can be resent
/// on a clean slate. The reply drives [`Message::EditForked`], which
/// resumes the fork; the pending edit's prompt goes out as the resumed
/// thread's first new turn.
pub(crate) fn fork_thread_before_turn(
    state: &State,
    thread_id: String,
    before_turn_id: String,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadFork {
                    request_id,
                    params: ThreadForkParams {
                        thread_id,
                        before_turn_id: Some(before_turn_id),
                        // The reply only needs the new thread id; history
                        // replays through the resume bootstrap page.
                        exclude_turns: true,
                        ..ThreadForkParams::default()
                    },
                })
                .await
                .and_then(|value| {
                    serde_json::from_value::<ThreadForkResponse>(value)
                        .map(|response| response.thread.id)
                        .map_err(Error::Json)
                });
            Message::EditForked(result)
        },
        |msg| msg,
    )
}

/// Deletes one thread; the caller chains a sidebar refresh afterwards.
pub(crate) fn delete_thread(state: &State, thread_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadDelete {
                    request_id,
                    params: ThreadDeleteParams { thread_id },
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "thread/delete failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Renames one thread through `thread/name/set`; the reply drives
/// [`Message::QuestRenamed`].
pub(crate) fn set_thread_name(state: &State, thread_id: String, name: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadSetName {
                    request_id,
                    params: ThreadSetNameParams {
                        thread_id: thread_id.clone(),
                        name: name.clone(),
                    },
                })
                .await;
            Message::QuestRenamed {
                thread_id,
                name,
                result,
            }
        },
        |msg| msg,
    )
}

/// Reads the effective config for the settings panel.
pub(crate) fn load_config(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ConfigRead {
                request_id,
                params: ConfigReadParams {
                    include_layers: false,
                    cwd: None,
                },
            })
            .await;
        Message::ConfigLoaded(result)
    })
}

/// Writes the drafted fields through one `config/batchWrite`, hot-reloading
/// the user config so running threads pick the new values up.
pub(crate) fn save_settings(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    let edits: Vec<ConfigEdit> = state
        .settings
        .edits()
        .into_iter()
        .map(|(key_path, value)| ConfigEdit {
            key_path,
            value,
            merge_strategy: MergeStrategy::Upsert,
        })
        .collect();
    if edits.is_empty() {
        state.settings.notice = Some("No changes to save".to_string());
        return Task::none();
    }
    state.settings.notice = Some("Saving…".to_string());

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ConfigBatchWrite {
                request_id,
                params: ConfigBatchWriteParams {
                    edits,
                    file_path: None,
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await;
        Message::SettingsSaved(result)
    })
}

/// Switches the vendor-picked model through one config write: the
/// provider key first (so routing lands before the model), then the
/// model id. The shared save path reopens the thread so the next turn
/// runs on the new vendor.
pub(crate) fn select_vendor_model(
    state: &State,
    provider_id: String,
    slug: String,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ConfigBatchWrite {
                request_id,
                params: ConfigBatchWriteParams {
                    edits: vec![
                        ConfigEdit {
                            key_path: "model_provider".to_string(),
                            value: serde_json::Value::String(provider_id),
                            merge_strategy: MergeStrategy::Upsert,
                        },
                        ConfigEdit {
                            key_path: "model".to_string(),
                            value: serde_json::Value::String(slug),
                            merge_strategy: MergeStrategy::Upsert,
                        },
                    ],
                    file_path: None,
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await;
        Message::SettingsSaved(result)
    })
}

/// Switches the live thread's model in place through
/// `thread/settings/update` (same vendor only; the provider is fixed per
/// thread); the reply drives [`Message::ModelAppliedInPlace`].
pub(crate) fn apply_model_in_place(
    state: &State,
    thread_id: String,
    model: String,
) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadSettingsUpdate {
                request_id,
                params: ThreadSettingsUpdateParams {
                    thread_id,
                    model: Some(model),
                    ..ThreadSettingsUpdateParams::default()
                },
            })
            .await;
        Message::ModelAppliedInPlace(result)
    })
}

/// Loads the skills inventory for the composer picker and the toggles.
pub(crate) fn load_skills(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::SkillsList {
                request_id,
                params: SkillsListParams::default(),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<SkillsListResponse>(value).map_err(Error::Json)
            });
        Message::SkillsLoaded(result)
    })
}

/// Writes one skill's enabled state through its name selector; the reply
/// drives [`Message::SkillEnabledSaved`].
pub(crate) fn set_skill_enabled(state: &State, name: String, enabled: bool) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::SkillsConfigWrite {
                request_id,
                params: SkillsConfigWriteParams {
                    path: None,
                    name: Some(name.clone()),
                    enabled,
                },
            })
            .await;
        Message::SkillEnabledSaved { name, result }
    })
}

/// Reloads the skills inventory bypassing the server cache, so the page's
/// mutations read straight back from disk.
pub(crate) fn reload_skills(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::SkillsList {
                request_id,
                params: SkillsListParams {
                    cwds: Vec::new(),
                    force_reload: true,
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<SkillsListResponse>(value).map_err(Error::Json)
            });
        Message::SkillsLoaded(result)
    })
}

/// The user-level skills root behind the page's disk mutations.
fn skills_root(state: &State) -> Option<PathBuf> {
    state
        .codex_home
        .as_ref()
        .map(|home| home.join(codex_gui_core::SKILLS_DIR))
}

/// The failure every disk mutation reports while `codexHome` is unknown.
fn missing_home() -> Task<Message> {
    Task::done(Message::SkillMutationSettled(Err(
        "app-server 尚未返回 codexHome，等待连接完成后再试".to_string(),
    )))
}

/// Creates a fresh user-level skill under `$CODEX_HOME/skills`.
pub(crate) fn create_skill(state: &State, name: String, description: String) -> Task<Message> {
    let Some(root) = skills_root(state) else {
        return missing_home();
    };

    Task::future(async move {
        let result = codex_gui_core::create_skill(&root, &name, &description)
            .map(|path| format!("已创建 skill `{name}`（{}）", path.display()));
        Message::SkillMutationSettled(result)
    })
}

/// Imports a skill: a whole directory or a single `SKILL.md` file.
pub(crate) fn import_skill(state: &State, source: PathBuf) -> Task<Message> {
    let Some(root) = skills_root(state) else {
        return missing_home();
    };

    Task::future(async move {
        let imported = if source.is_dir() {
            codex_gui_core::import_skill_directory(&root, &source)
        } else {
            codex_gui_core::import_skill_file(&root, &source)
        };
        let result = imported.map(|path| format!("已导入 skill（{}）", path.display()));
        Message::SkillMutationSettled(result)
    })
}

/// Imports the skills behind a Git link; the clone runs off the UI
/// thread since it hits the network. A link that names several skills
/// imports them as one batch.
pub(crate) fn import_skill_from_link(state: &State, link: String) -> Task<Message> {
    let Some(root) = skills_root(state) else {
        return missing_home();
    };

    Task::future(async move {
        let result = codex_gui_core::import_skill_from_link(&root, &link).and_then(|report| {
            if report.failed.is_empty() {
                Ok(link_import_notice(&report.imported))
            } else {
                Err(format!(
                    "{} 个 skill 未能导入（已成功 {} 个）：{}",
                    report.failed.len(),
                    report.imported.len(),
                    report.failed[0]
                ))
            }
        });
        Message::SkillMutationSettled(result)
    })
}

/// The success notice for one link import: a single skill is named by
/// its `SKILL.md` path, a batch by count and its first directory names.
fn link_import_notice(imported: &[PathBuf]) -> String {
    match imported {
        [skill_md] => format!("已从链接导入 skill（{}）", skill_md.display()),
        skills => {
            let names: Vec<String> = skills
                .iter()
                .take(3)
                .filter_map(|skill_md| {
                    let dir = skill_md.parent()?.file_name()?;
                    Some(dir.to_string_lossy().into_owned())
                })
                .collect();
            let rest = if skills.len() > names.len() {
                " 等"
            } else {
                ""
            };
            format!(
                "已从链接导入 {} 个 skill：{}{rest}",
                skills.len(),
                names.join("、")
            )
        }
    }
}

/// Rewrites one local skill's name and description from the edit form.
pub(crate) fn edit_skill(path: PathBuf, name: String, description: String) -> Task<Message> {
    Task::future(async move {
        let result = codex_gui_core::update_skill(&path, &name, &description)
            .map(|()| format!("已更新 skill `{name}`"));
        Message::SkillMutationSettled(result)
    })
}

/// Removes one user-level skill directory from disk.
pub(crate) fn delete_skill_files(path: PathBuf, name: String) -> Task<Message> {
    Task::future(async move {
        let result = codex_gui_core::delete_skill(&path).map(|()| format!("已删除 skill `{name}`"));
        Message::SkillMutationSettled(result)
    })
}

/// Opens the native picker for an import source; the selection drives
/// [`Message::SkillAddSourcePicked`] and cancellation arrives as `None`.
pub(crate) fn pick_skill_source(target: SkillImportTarget) -> Task<Message> {
    let dialog = rfd::AsyncFileDialog::new().set_title(match target {
        SkillImportTarget::Directory => "Import skill directory",
        SkillImportTarget::File => "Import SKILL.md",
    });
    let dialog = match target {
        SkillImportTarget::Directory => dialog,
        SkillImportTarget::File => dialog.add_filter("Markdown", &["md"]),
    };

    Task::future(async move {
        let picked = match target {
            SkillImportTarget::Directory => dialog.pick_folder().await,
            SkillImportTarget::File => dialog.pick_file().await,
        };
        Message::SkillAddSourcePicked(picked.map(|handle| handle.path().to_path_buf()))
    })
}

/// Loads the plugin marketplaces for the market tab.
pub(crate) fn load_market(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::PluginList {
                request_id,
                params: PluginListParams {
                    cwds: None,
                    marketplace_kinds: None,
                    force_refetch: false,
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<PluginListResponse>(value).map_err(Error::Json)
            });
        Message::SkillMarketLoaded(result)
    })
}

/// Fetches one plugin's bundled skills for its expanded market row.
pub(crate) fn load_market_detail(state: &State, plugin_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(plugin) = market_plugin(state, &plugin_id) else {
        return Task::none();
    };
    let fields = marketplace_fields(&plugin.marketplace_selector());
    let plugin_name = plugin.wire_name.clone();

    Task::future(async move {
        let result = match fields {
            Ok((marketplace_path, remote_marketplace_name)) => client
                .call(|request_id| ClientRequest::PluginRead {
                    request_id,
                    params: PluginReadParams {
                        marketplace_path,
                        remote_marketplace_name,
                        plugin_name,
                    },
                })
                .await
                .and_then(|value| {
                    serde_json::from_value::<PluginReadResponse>(value).map_err(Error::Json)
                }),
            Err(error) => Err(error),
        };
        Message::SkillMarketDetailLoaded(result)
    })
}

/// Installs one market plugin, bringing its bundled skills onto the page.
pub(crate) fn install_market_plugin(state: &State, plugin_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(plugin) = market_plugin(state, &plugin_id) else {
        return Task::none();
    };
    let fields = marketplace_fields(&plugin.marketplace_selector());
    let plugin_name = plugin.wire_name.clone();

    Task::future(async move {
        let result = match fields {
            Ok((marketplace_path, remote_marketplace_name)) => {
                client
                    .call(|request_id| ClientRequest::PluginInstall {
                        request_id,
                        params: PluginInstallParams {
                            marketplace_path,
                            remote_marketplace_name,
                            install_attempt_id: None,
                            plugin_name,
                        },
                    })
                    .await
            }
            Err(error) => Err(error),
        };
        Message::SkillInstalled {
            id: plugin_id,
            result,
        }
    })
}

/// Uninstalls one plugin by its `name@marketplace` id.
pub(crate) fn uninstall_plugin(state: &State, plugin_id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let wire_id = plugin_id.clone();

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::PluginUninstall {
                request_id,
                params: PluginUninstallParams { plugin_id: wire_id },
            })
            .await;
        Message::SkillUninstalled {
            id: plugin_id,
            result,
        }
    })
}

/// Adds one marketplace source (`owner/repo`, a git URL, or a local path).
pub(crate) fn add_marketplace(state: &State, source: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::MarketplaceAdd {
                request_id,
                params: MarketplaceAddParams {
                    source,
                    ref_name: None,
                    sparse_paths: None,
                },
            })
            .await;
        Message::SkillMarketSourceAdded(result)
    })
}

/// One market row by its `name@marketplace` id.
fn market_plugin<'a>(
    state: &'a State,
    plugin_id: &str,
) -> Option<&'a codex_gui_core::MarketPlugin> {
    state
        .skills
        .market
        .plugins
        .iter()
        .find(|plugin| plugin.id == plugin_id)
}

/// Maps a market selector onto the wire marketplace fields; a local path
/// that cannot convert to an absolute path fails loudly instead of
/// silently targeting the wrong marketplace.
fn marketplace_fields(
    selector: &MarketSelector,
) -> Result<(Option<AbsolutePathBuf>, Option<String>), Error> {
    match selector {
        MarketSelector::Path(path) => {
            let absolute = AbsolutePathBuf::try_from(path.clone()).map_err(|_ignored| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("marketplace path {} is not absolute", path.display()),
                ))
            })?;
            Ok((Some(absolute), None))
        }
        MarketSelector::RemoteName(name) => Ok((None, Some(name.clone()))),
    }
}

/// Starts an API-key login; the reply drives [`Message::LoginCompleted`].
pub(crate) fn login_with_api_key(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let api_key = state.settings.take_api_key();
    if api_key.is_empty() {
        state.settings.notice = Some("Enter an API key first".to_string());
        return Task::none();
    }
    state.settings.notice = Some("Signing in…".to_string());

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::LoginAccount {
                request_id,
                params: LoginAccountParams::ApiKey { api_key },
            })
            .await;
        Message::LoginCompleted(result)
    })
}

/// Signs out; the reply drives [`Message::LogoutCompleted`].
pub(crate) fn logout(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::LogoutAccount {
                request_id,
                params: None,
            })
            .await;
        Message::LogoutCompleted(result)
    })
}

/// Probes the working tree of `cwd`; the result drives
/// [`Message::GitInfoArrived`]. The synchronous git shell-out runs inside
/// the task's executor thread — a tens-of-milliseconds cost per refresh.
pub(crate) fn refresh_git(cwd: String) -> Task<Message> {
    Task::future(async move {
        let info = codex_gui_core::GitInfo::probe(&std::path::PathBuf::from(cwd));
        Message::GitInfoArrived(info)
    })
}

/// Probes the remote-explorer facts once: SSH host aliases from
/// `~/.ssh/config` plus a Docker daemon check; the reply drives
/// [`Message::RemoteInfoArrived`]. Both probes run synchronously inside
/// the task's executor thread, matching the git probe's cost profile.
pub(crate) fn probe_remote() -> Task<Message> {
    Task::future(async move {
        let targets = codex_gui_core::ssh_targets()
            .into_iter()
            .map(|target| target.host)
            .collect();
        Message::RemoteInfoArrived {
            targets,
            docker_available: codex_gui_core::docker_available(),
        }
    })
}

/// The composer's selected files as paths, in working-tree order.
fn selected_paths(state: &State) -> Vec<String> {
    state
        .git
        .changed_files()
        .into_iter()
        .enumerate()
        .filter(|(index, _)| state.git_overlay.selected.contains(index))
        .map(|(_, (path, _))| path)
        .collect()
}

/// Stages the composer's selected files and commits them; the reply
/// drives [`Message::CommitFinished`].
pub(crate) fn commit_selected(state: &mut State) -> Task<Message> {
    let Some(cwd) = state.status_board.cwd().map(str::to_string) else {
        return Task::none();
    };
    let paths = selected_paths(state);
    let message = state.git_overlay.message.clone();
    if paths.is_empty() || message.trim().is_empty() {
        return Task::none();
    }

    Task::future(async move {
        Message::CommitFinished(codex_gui_core::GitInfo::stage_and_commit(
            &std::path::PathBuf::from(cwd),
            &paths,
            &message,
        ))
    })
}

/// The ✨ optimizer instruction: rewrite the user's draft into one
/// structured, actionable prompt. The rewrite turn runs on a throwaway
/// thread pinned to the project, so the model can look around for
/// context before answering.
const PROMPT_OPTIMIZE_INSTRUCTION: &str = "You are a senior prompt engineer for a coding agent. \
Rewrite the user's draft below into one structured, actionable development-task prompt. \
Include: a clear goal, key constraints, and a short acceptance checklist. \
Ground it in the current project when you can (read files as needed). \
Reply with only the rewritten prompt text - no preamble, no explanations, no code fences. \
Match the draft's language (Chinese draft -> Chinese rewrite).";

/// ✨ prompt optimizer, step 1: open a throwaway thread pinned to the
/// current project for the rewrite turn.
pub(crate) fn request_prompt_optimize(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let draft = state.composer.trim().to_string();
    if draft.is_empty() {
        return Task::none();
    }
    let Some(cwd) = state.status_board.cwd().map(str::to_string) else {
        state.prompt_optimize.status =
            crate::state::PromptOptimizeStatus::Failed(String::from("no working directory"));
        return Task::none();
    };
    state.prompt_optimize.status = crate::state::PromptOptimizeStatus::Running;
    state.prompt_optimize.original = None;

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params(Some(cwd.clone())),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadStartResponse>(value)
                    .map(|response| response.thread.id)
                    .map_err(Error::Json)
            });
        Message::OptimizeThreadStarted {
            original: draft,
            result,
        }
    })
}

/// ✨ prompt optimizer, step 2: one turn on the throwaway thread asking
/// for the restructured prompt over the captured draft.
pub(crate) fn start_optimize_turn(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.prompt_optimize.thread.clone() else {
        return Task::none();
    };
    let Some(original) = state.prompt_optimize.original.clone() else {
        return Task::none();
    };

    let prompt = format!("{PROMPT_OPTIMIZE_INSTRUCTION}\n\nDraft:\n{original}");
    let input = vec![UserInput::Text {
        text: prompt,
        text_elements: Vec::new(),
    }];

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::TurnStart {
                    request_id,
                    params: TurnStartParams {
                        thread_id,
                        input,
                        ..TurnStartParams::default()
                    },
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "optimize turn start failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Voice dictation, step 1: open a throwaway thread pinned to the current
/// project; the realtime session runs on it (same isolation as the commit
/// draft and the ✨ optimizer).
pub(crate) fn request_voice_thread(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        state.voice_input.status =
            VoiceInputStatus::Failed(String::from("not connected to the app-server"));
        return Task::none();
    };
    let Some(cwd) = state.status_board.cwd().map(str::to_string) else {
        state.voice_input.status = VoiceInputStatus::Failed(String::from("no working directory"));
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params(Some(cwd.clone())),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadStartResponse>(value)
                    .map(|response| response.thread.id)
                    .map_err(Error::Json)
            });
        Message::VoiceThreadStarted { result }
    })
}

/// The actionable hint shown when the realtime session refuses to open;
/// the domestic preset models (GLM/DeepSeek/Kimi/Qwen/Doubao/MiniMax) have no
/// realtime channel, and nothing should pretend otherwise.
pub(crate) const VOICE_UNSUPPORTED_HINT: &str =
    "当前模型不支持语音输入，请切换至支持实时语音的模型";

/// Voice dictation, step 2: the whole session as one streaming task -
/// realtime start, microphone capture, 100 ms PCM16 batches, teardown.
///
/// The task owns the capture handle and the stop signal; the update loop
/// owns every state transition and, through notification routing, the
/// transcript itself. On stop the device is released first (the mic never
/// stays open), the already-captured tail is uploaded, the session ends,
/// and the throwaway thread archives.
pub(crate) fn start_voice_session(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.voice_input.thread.clone() else {
        return Task::none();
    };
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    state.voice_input.stop = Some(stop_tx);

    Task::stream(iced::stream::channel(
        16,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            if let Err(error) = codex_gui_bridge::realtime::start(&client, &thread_id).await {
                let reason = format!("{VOICE_UNSUPPORTED_HINT}（{error}）");
                let _ignored = output
                    .send(Message::VoiceSessionStarted {
                        result: Err(reason),
                    })
                    .await;
                archive_voice_thread(&client, &thread_id).await;
                let _ignored = output.send(Message::VoiceInputStopped).await;
                return;
            }

            // The cpal callback runs on the audio thread and must never
            // block, so chunks hop over an unbounded channel; opening the
            // device itself blocks, so it runs on a blocking worker.
            let (audio_tx, mut audio_rx) =
                tokio::sync::mpsc::unbounded_channel::<codex_gui_core::AudioChunk>();
            let capture = tokio::task::spawn_blocking(move || {
                codex_gui_core::start_audio_capture(move |chunk| {
                    let _ignored = audio_tx.send(chunk);
                })
            })
            .await
            .unwrap_or_else(|error| Err(codex_gui_core::CaptureError::Thread(error.to_string())));
            let capture = match capture {
                Ok(capture) => capture,
                Err(error) => {
                    let reason = format!("无法访问麦克风：{error}");
                    let _ignored = output
                        .send(Message::VoiceSessionStarted {
                            result: Err(reason),
                        })
                        .await;
                    let _ignored = codex_gui_bridge::realtime::stop(&client, &thread_id).await;
                    archive_voice_thread(&client, &thread_id).await;
                    let _ignored = output.send(Message::VoiceInputStopped).await;
                    return;
                }
            };
            let _ignored = output
                .send(Message::VoiceSessionStarted { result: Ok(()) })
                .await;

            // 100 ms batches at the protocol's 24 kHz mono PCM16.
            let target = codex_gui_core::TARGET_SAMPLE_RATE as usize / 10;
            let mut pending: Vec<i16> = Vec::new();
            let mut consecutive_errors = 0u32;
            let mut stop_rx = stop_rx;
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    maybe = audio_rx.recv() => {
                        let Some(chunk) = maybe else { break };
                        pending.extend_from_slice(&chunk.pcm16);
                        if pending.len() < target {
                            continue;
                        }
                        let batch = std::mem::take(&mut pending);
                        match codex_gui_bridge::realtime::append_audio(
                            &client,
                            &thread_id,
                            &batch,
                            codex_gui_core::TARGET_SAMPLE_RATE,
                        )
                        .await
                        {
                            Ok(()) => consecutive_errors = 0,
                            Err(error) => {
                                consecutive_errors += 1;
                                tracing::warn!(%error, "voice audio append failed");
                                // The session is gone; the notification
                                // side reports it to the user.
                                if consecutive_errors >= 5 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Stopped: release the device first, then flush whatever was
            // already captured, then end the session cleanly.
            capture.stop();
            while let Ok(chunk) = audio_rx.try_recv() {
                pending.extend_from_slice(&chunk.pcm16);
            }
            if !pending.is_empty() {
                let _ignored = codex_gui_bridge::realtime::append_audio(
                    &client,
                    &thread_id,
                    &pending,
                    codex_gui_core::TARGET_SAMPLE_RATE,
                )
                .await;
            }
            let _ignored = codex_gui_bridge::realtime::stop(&client, &thread_id).await;
            // Let the server's final transcript notifications land while
            // the thread id is still routed to the composer.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            archive_voice_thread(&client, &thread_id).await;
            let _ignored = output.send(Message::VoiceInputStopped).await;
        },
    ))
}

/// Archives the throwaway dictation thread; failure only logs - the UI has
/// already been told the session ended either way.
async fn archive_voice_thread(client: &codex_gui_bridge::Client, thread_id: &str) {
    let result = client
        .call(|request_id| ClientRequest::ThreadArchive {
            request_id,
            params: ThreadArchiveParams {
                thread_id: thread_id.to_string(),
            },
        })
        .await;
    if let Err(error) = result {
        tracing::warn!(%error, "voice thread archive failed");
    }
}

/// AI commit draft, step 1: stage the selection, capture the staged
/// diff, and open a throwaway thread for the draft turn.
pub(crate) fn request_commit_draft(state: &mut State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(cwd) = state.status_board.cwd().map(str::to_string) else {
        state.git_overlay.status =
            crate::state::GitOverlayStatus::Failed(String::from("no working directory"));
        return Task::none();
    };
    let paths = selected_paths(state);
    if paths.is_empty() {
        state.git_overlay.status =
            crate::state::GitOverlayStatus::Failed(String::from("select files to describe first"));
        return Task::none();
    }
    state.git_overlay.status = crate::state::GitOverlayStatus::Drafting;

    Task::future(async move {
        let root = std::path::PathBuf::from(&cwd);
        // Stage first so the draft (and the later Commit press) describe
        // exactly the same content.
        if let Err(error) = codex_gui_core::GitInfo::stage(&root, &paths) {
            return Message::CommitDraftFailed(error);
        }
        let Some(diff) = codex_gui_core::GitInfo::staged_diff(&root) else {
            return Message::CommitDraftFailed(String::from("git diff --cached failed"));
        };
        if diff.trim().is_empty() {
            return Message::CommitDraftFailed(String::from("nothing staged to describe"));
        }

        let result = client
            .call(|request_id| ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params(Some(cwd.clone())),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadStartResponse>(value)
                    .map(|response| response.thread.id)
                    .map_err(Error::Json)
            });
        Message::DraftThreadStarted { diff, result }
    })
}

/// AI commit draft, step 2: one turn on the throwaway thread asking for
/// a conventional commit message over the staged diff.
pub(crate) fn start_draft_turn(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.git_overlay.draft_thread.clone() else {
        return Task::none();
    };
    let Some(diff) = state.git_overlay.draft_diff.clone() else {
        return Task::none();
    };

    let prompt = format!(
        "Write one conventional commit message for the following staged diff.\
Reply with only the commit message body:\n\n{diff}"
    );
    let input = vec![UserInput::Text {
        text: prompt,
        text_elements: Vec::new(),
    }];

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::TurnStart {
                    request_id,
                    params: TurnStartParams {
                        thread_id,
                        input,
                        ..TurnStartParams::default()
                    },
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "draft turn start failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// The largest file the central preview will render (512 KiB).
const PREVIEW_LIMIT: u64 = 512 * 1024;

/// Reads a file for the central preview; the reply drives
/// [`Message::FilePreviewLoaded`]. The blocking `std::fs` calls run on
/// the async runtime's worker threads, off the render path.
pub(crate) fn read_preview(path: PathBuf) -> Task<Message> {
    let read_path = path.clone();
    Task::perform(async move { preview_body(&read_path) }, move |body| {
        Message::FilePreviewLoaded { path, body }
    })
}

/// Classifies one file for the preview pane: text is truncated at the
/// cap, non-UTF-8 bytes report binary, and IO failures carry their
/// reason.
fn preview_body(path: &std::path::Path) -> crate::state::FileBody {
    use crate::state::FileBody;
    use std::sync::Arc;

    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > PREVIEW_LIMIT => return FileBody::TooLarge,
        Ok(_) => {}
        Err(error) => return FileBody::Failed(error.to_string()),
    }
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).map_or(FileBody::Binary, |mut text| {
            if text.len() > PREVIEW_LIMIT as usize {
                // Truncate on a char boundary: `String::truncate` panics
                // when the cap lands inside a multi-byte glyph.
                let mut end = PREVIEW_LIMIT as usize;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                text.push_str("\n… (已截断)");
            }
            FileBody::Text(Arc::from(text))
        }),
        Err(error) => FileBody::Failed(error.to_string()),
    }
}

/// Interrupts the streaming turn behind the composer's stop button; the
/// reply drives [`Message::TurnInterrupted`].
pub(crate) fn interrupt_turn(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.thread_id.clone() else {
        return Task::none();
    };
    let Some(turn_id) = state.active_turn.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams { thread_id, turn_id },
            })
            .await;
        Message::TurnInterrupted(result)
    })
}

/// The shared cancellation token for `@` mention searches: reusing one
/// token lets a newer keystroke cancel the in-flight older search.
const MENTION_CANCEL_TOKEN: &str = "codex-gui-mentions";

/// Runs one `fuzzyFileSearch` for the composer's `@` mentions; the reply
/// drives [`Message::MentionSearchLoaded`] with the query echoed back.
pub(crate) fn search_files(state: &State, query: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(cwd) = state.status_board.cwd().map(str::to_string) else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::FuzzyFileSearch {
                request_id,
                params: FuzzyFileSearchParams {
                    query: query.clone(),
                    roots: vec![cwd],
                    cancellation_token: Some(MENTION_CANCEL_TOKEN.to_string()),
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<FuzzyFileSearchResponse>(value).map_err(Error::Json)
            });
        Message::MentionSearchLoaded { query, result }
    })
}

/// Starts an inline review of the working tree; the reply drives
/// [`Message::ReviewStarted`], while the review itself streams through
/// the thread's normal notifications.
pub(crate) fn start_review(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.thread_id.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ReviewStart {
                request_id,
                params: ReviewStartParams {
                    thread_id,
                    target: ReviewTarget::UncommittedChanges,
                    delivery: None,
                },
            })
            .await;
        Message::ReviewStarted(result)
    })
}

/// Compacts the current thread's context in place; the reply drives
/// [`Message::CompactionStarted`].
pub(crate) fn compact_thread(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.thread_id.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadCompactStart {
                request_id,
                params: ThreadCompactStartParams { thread_id },
            })
            .await;
        Message::CompactionStarted(result)
    })
}

/// Loads the MCP server inventory for the settings panel; the reply
/// drives [`Message::McpServersLoaded`].
pub(crate) fn load_mcp_servers(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let thread_id = state.thread_id.clone();

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::McpServerStatusList {
                request_id,
                params: ListMcpServerStatusParams {
                    cursor: None,
                    limit: None,
                    detail: Some(McpServerStatusDetail::ToolsAndAuthOnly),
                    thread_id,
                    server_name: None,
                },
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ListMcpServerStatusResponse>(value).map_err(Error::Json)
            });
        Message::McpServersLoaded(result)
    })
}

/// JSON-RPC "method not found", the standard answer for a server request
/// this client cannot handle.
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;

/// Answers one server request the GUI has no dialog for with a
/// `method not found` error, so the request never dangles until the
/// server's own timeout resolves it.
pub(crate) fn reject_server_request(state: &State, request: &ServerRequest) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let id = request.id().clone();
    // `ServerRequest` has no method accessor; its serde tag is the only
    // stable name for the answer text.
    let method = serde_json::to_value(request)
        .ok()
        .and_then(|value| {
            value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| String::from("unknown"));

    Task::perform(
        async move {
            let result = client
                .respond_error_for_id(
                    id,
                    JSONRPC_METHOD_NOT_FOUND,
                    format!("codex-gui does not implement {method}"),
                )
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "rejecting server request failed");
            }
        },
        |()| Message::TurnAcked,
    )
}

/// Slash commands the composer understands in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlashAction {
    /// `/review` — review the uncommitted changes.
    Review,
    /// `/compact` — compact the context.
    Compact,
}

/// The `/` command palette rows: `(command, hint)`, in render order.
pub(crate) const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/review", "审查未提交的变更（review/start）"),
    ("/compact", "压缩当前上下文（thread/compact/start）"),
];

/// Resolves a parsed composer submission into a slash action; `None`
/// means the text is an ordinary prompt.
pub(crate) fn slash_action(text: &str) -> Option<SlashAction> {
    match text.trim() {
        "/review" => Some(SlashAction::Review),
        "/compact" => Some(SlashAction::Compact),
        _ => None,
    }
}

/// The rows matching the composer text while the user types a bare
/// `/word`; an empty vec hides the picker.
pub(crate) fn slash_options(text: &str) -> Vec<(&'static str, &'static str)> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(char::is_whitespace) {
        return Vec::new();
    }
    SLASH_COMMANDS
        .iter()
        .filter(|(command, _)| command.starts_with(trimmed))
        .copied()
        .collect()
}
