//! Outgoing app-server requests: every [`Task`] the UI produces to talk to
//! the app-server lives here, keeping `update.rs` a pure dispatcher.

use crate::message::Bootstrap;
use crate::message::Message;
use crate::state::State;
use crate::state::Status;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::GetAccountParams;
use codex_app_server_protocol::GetAccountResponse;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::LoginAccountParams;
use codex_app_server_protocol::MergeStrategy;
use codex_app_server_protocol::ModelListParams;
use codex_app_server_protocol::ModelListResponse;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::SkillsConfigWriteParams;
use codex_app_server_protocol::SkillsListParams;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadDeleteParams;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadSetNameParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use codex_gui_bridge::Error;
use codex_gui_core::Approvals;
use codex_gui_core::Decision;
use codex_gui_core::MarkdownStream;
use codex_gui_core::SessionHistory;
use codex_gui_core::compose_inputs;
use iced::Task;
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
        capabilities: None,
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
    start_thread_in(state, cwd)
}

/// Sends the composer text plus any pending image attachments and `$skill`
/// mentions as a new turn.
pub(crate) fn submit(state: &mut State) -> Task<Message> {
    if !state.can_submit() {
        return Task::none();
    }

    let Some(client) = state.client.clone() else {
        return Task::none();
    };
    let Some(thread_id) = state.thread_id.clone() else {
        return Task::none();
    };

    let text = std::mem::take(&mut state.composer);
    let images = std::mem::take(&mut state.attachments.images);
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

/// Refreshes the sidebar inventory from the first `thread/list` page.
pub(crate) fn load_sessions(state: &State) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ThreadList {
                request_id,
                params: thread_list_params(),
            })
            .await
            .and_then(|value| {
                serde_json::from_value::<ThreadListResponse>(value).map_err(Error::Json)
            });
        Message::SessionsLoaded(result)
    })
}

/// A `thread/list` page in server-default order (newest first).
fn thread_list_params() -> ThreadListParams {
    ThreadListParams {
        cursor: None,
        limit: None,
        sort_key: None,
        sort_direction: None,
        model_providers: None,
        source_kinds: None,
        originators: None,
        archived: None,
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

    Task::perform(
        async move {
            let result = client
                .call(|request_id| ClientRequest::ThreadResume {
                    request_id,
                    params: ThreadResumeParams {
                        thread_id,
                        ..ThreadResumeParams::default()
                    },
                })
                .await;
            result.and_then(|value| {
                serde_json::from_value::<ThreadResumeResponse>(value)
                    .map(|response| SessionHistory::from_resume(&response))
                    .map_err(Error::Json)
            })
        },
        Message::SessionResumed,
    )
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

/// Switches the top-level `model` through one config write; the shared
/// save path reopens the thread so the next turn runs on the new model.
pub(crate) fn select_model(state: &State, id: String) -> Task<Message> {
    let Some(client) = state.client.clone() else {
        return Task::none();
    };

    Task::future(async move {
        let result = client
            .call(|request_id| ClientRequest::ConfigBatchWrite {
                request_id,
                params: ConfigBatchWriteParams {
                    edits: vec![ConfigEdit {
                        key_path: "model".to_string(),
                        value: serde_json::Value::String(id),
                        merge_strategy: MergeStrategy::Upsert,
                    }],
                    file_path: None,
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await;
        Message::SettingsSaved(result)
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
