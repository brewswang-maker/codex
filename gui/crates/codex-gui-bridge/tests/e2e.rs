//! Process-level integration tests against the bundled mock app-server.
#![allow(clippy::expect_used, clippy::panic)]

use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
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
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::SkillsConfigWriteParams;
use codex_app_server_protocol::SkillsListParams;
use codex_app_server_protocol::SkillsListResponse;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use codex_gui_bridge::Error;
use codex_gui_bridge::Flags;
use codex_gui_bridge::GuiEvent;
use std::path::PathBuf;
use tokio::time::Duration;
use tokio::time::timeout;

fn mock_flags(scenario: &str) -> Flags {
    Flags {
        program: PathBuf::from(env!("CARGO_BIN_EXE_codex-gui-mock-server")),
        args: vec!["--scenario".to_string(), scenario.to_string()],
    }
}

fn initialize_params() -> InitializeParams {
    InitializeParams {
        client_info: ClientInfo {
            name: "codex-gui-tests".to_string(),
            title: Some("Codex GUI tests".to_string()),
            version: "0.1.0".to_string(),
        },
        capabilities: None,
    }
}

async fn expect_notification(
    events: &mut tokio::sync::mpsc::Receiver<GuiEvent>,
) -> ServerNotification {
    match timeout(Duration::from_secs(10), events.recv()).await {
        Ok(Some(GuiEvent::Notification(notification))) => notification,
        Ok(Some(other)) => panic!("expected notification, got {other:?}"),
        Ok(None) => panic!("event stream ended before the expected notification"),
        Err(_elapsed) => panic!("timed out waiting for a notification"),
    }
}

#[tokio::test]
async fn basic_turn_streams_agent_message() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");

    let user_agent = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");
    assert_eq!(user_agent["userAgent"], "mock");

    let thread = client
        .call(|id| ClientRequest::ThreadStart {
            request_id: id,
            params: ThreadStartParams::default(),
        })
        .await
        .expect("thread/start succeeds");
    assert_eq!(thread["thread"]["id"], "thread-1");

    let _ignored = client
        .call(|id| ClientRequest::TurnStart {
            request_id: id,
            params: TurnStartParams {
                thread_id: "thread-1".to_string(),
                input: vec![UserInput::Text {
                    text: "hi".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start succeeds");

    // The mock replays the userMessage and a commandExecution item first,
    // mirroring real servers; skip everything until the agent message.
    let agent_message_id = loop {
        match expect_notification(&mut events).await {
            ServerNotification::ItemStarted(started) => {
                if let ThreadItem::AgentMessage { id, .. } = started.item {
                    break id;
                }
            }
            _other => continue,
        }
    };
    assert_eq!(agent_message_id, "item-1");

    let mut deltas = String::new();
    for _ in 0..3 {
        let notification = expect_notification(&mut events).await;
        let ServerNotification::AgentMessageDelta(delta) = notification else {
            panic!("expected agentMessage delta, got {notification:?}");
        };
        deltas.push_str(&delta.delta);
    }
    assert_eq!(deltas, "Hello world");

    let completed = expect_notification(&mut events).await;
    let ServerNotification::ItemCompleted(completed) = completed else {
        panic!("expected item/completed, got {completed:?}");
    };
    let ThreadItem::AgentMessage { id, text, .. } = completed.item else {
        panic!("expected an agentMessage item");
    };
    assert_eq!(id, "item-1");
    assert_eq!(text, "Hello world");
}

#[tokio::test]
async fn command_execution_streams_output() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");

    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    let _ignored = client
        .call(|id| ClientRequest::TurnStart {
            request_id: id,
            params: TurnStartParams {
                thread_id: "thread-1".to_string(),
                input: vec![UserInput::Text {
                    text: "run echo".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start succeeds");

    let mut output = String::new();
    loop {
        match timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Some(GuiEvent::Notification(ServerNotification::ItemStarted(started)))) => {
                // The userMessage replay also arrives as item/started.
                if let ThreadItem::CommandExecution { ref command, .. } = started.item {
                    assert_eq!(command, "echo hi");
                }
            }
            Ok(Some(GuiEvent::Notification(ServerNotification::CommandExecutionOutputDelta(
                delta,
            )))) => output.push_str(&delta.delta),
            Ok(Some(GuiEvent::Notification(ServerNotification::ItemCompleted(completed)))) => {
                let ThreadItem::CommandExecution {
                    aggregated_output, ..
                } = completed.item
                else {
                    panic!("expected a commandExecution completion");
                };
                assert_eq!(aggregated_output.as_deref(), Some("hi\n"));
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("event stream ended before the command completion"),
            Err(_elapsed) => panic!("timed out waiting for the command completion"),
        }
    }

    assert_eq!(output, "hi\n");
}

#[tokio::test]
async fn approval_request_round_trip() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("approval"))
        .await
        .expect("mock app-server starts");

    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    let _ignored = client
        .call(|id| ClientRequest::TurnStart {
            request_id: id,
            params: TurnStartParams {
                thread_id: "thread-1".to_string(),
                input: vec![UserInput::Text {
                    text: "run ls".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start succeeds");

    // The approval gate: the mock holds the item stream back until the
    // decision reply arrives.
    loop {
        match timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Some(GuiEvent::ServerRequest(request))) => {
                let ServerRequest::CommandExecutionRequestApproval { params, .. } = &request else {
                    panic!("expected a command-execution approval, got {request:?}");
                };
                assert_eq!(params.command.as_deref(), Some("ls -la"));
                let response = CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Accept,
                };
                client
                    .respond(
                        &request,
                        serde_json::to_value(response).expect("approval response serializes"),
                    )
                    .await
                    .expect("approval reply is delivered");
                break;
            }
            Ok(Some(_other)) => continue,
            Ok(None) => panic!("event stream ended before the approval request"),
            Err(_elapsed) => panic!("timed out waiting for the approval request"),
        }
    }

    // The gated stream resumes only after the decision.
    let text = loop {
        match expect_notification(&mut events).await {
            ServerNotification::ItemCompleted(completed) => {
                if let ThreadItem::AgentMessage { text, .. } = completed.item {
                    break text;
                }
            }
            _other => continue,
        }
    };
    assert_eq!(text, "Hello world");
}

#[tokio::test]
async fn thread_list_resume_and_archive_round_trip() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");

    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The sidebar page carries the resumable thread.
    let listed = client
        .call(|id| ClientRequest::ThreadList {
            request_id: id,
            params: thread_list_params(),
        })
        .await
        .expect("thread/list succeeds");
    let listed: ThreadListResponse = serde_json::from_value(listed).expect("list decodes");
    assert_eq!(listed.data.len(), 1);
    assert_eq!(listed.data[0].id, "thread-1");
    assert_eq!(listed.data[0].preview, "resumable conversation");
    assert!(listed.next_cursor.is_none());

    // Resume replays the persisted turns as complete snapshots.
    let resumed = client
        .call(|id| ClientRequest::ThreadResume {
            request_id: id,
            params: ThreadResumeParams {
                thread_id: "thread-1".to_string(),
                ..ThreadResumeParams::default()
            },
        })
        .await
        .expect("thread/resume succeeds");
    let resumed: ThreadResumeResponse = serde_json::from_value(resumed).expect("resume decodes");
    assert_eq!(resumed.thread.id, "thread-1");
    assert_eq!(resumed.thread.turns.len(), 1);
    let items = &resumed.thread.turns[0].items;
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], ThreadItem::UserMessage { .. }));
    assert!(matches!(&items[1], ThreadItem::AgentMessage { text, .. } if text == "History replay"));

    // Archive acknowledges and notifies.
    let archived = client
        .call(|id| ClientRequest::ThreadArchive {
            request_id: id,
            params: ThreadArchiveParams {
                thread_id: "thread-1".to_string(),
            },
        })
        .await
        .expect("thread/archive succeeds");
    assert_eq!(archived, serde_json::json!({}));

    let notification = expect_notification(&mut events).await;
    let ServerNotification::ThreadArchived(archived) = notification else {
        panic!("expected thread/archived, got {notification:?}");
    };
    assert_eq!(archived.thread_id, "thread-1");
}

/// A `thread/list` page in server-default order (newest first); the params
/// struct has no `Default`.
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

#[tokio::test]
async fn spawn_failure_is_reported_as_spawn_error() {
    let flags = Flags {
        program: PathBuf::from("/nonexistent/codex-binary"),
        args: Vec::new(),
    };

    let error = codex_gui_bridge::start(flags)
        .await
        .expect_err("spawn must fail");

    assert!(matches!(error, Error::Spawn { .. }));
}

#[tokio::test]
async fn status_surface_round_trip() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("error"))
        .await
        .expect("mock app-server starts");

    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The model catalog decodes with the picker display name.
    let models = client
        .call(|id| ClientRequest::ModelList {
            request_id: id,
            params: ModelListParams::default(),
        })
        .await
        .expect("model/list succeeds");
    let models: ModelListResponse = serde_json::from_value(models).expect("models decode");
    assert_eq!(models.data[0].display_name, "Mock Model");
    assert!(models.next_cursor.is_none());

    // The login badge carries the account identity.
    let account = client
        .call(|id| ClientRequest::GetAccount {
            request_id: id,
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read succeeds");
    let account: GetAccountResponse = serde_json::from_value(account).expect("account decodes");
    let codex_app_server_protocol::Account::Chatgpt { email, plan_type } =
        account.account.expect("an account is signed in")
    else {
        panic!("expected a chatgpt account");
    };
    assert_eq!(email.as_deref(), Some("dev@example.com"));
    let plan = serde_json::to_value(plan_type).expect("plan serializes");
    assert_eq!(plan, serde_json::json!("pro"));

    // The error scenario surfaces an `error` notification.
    let _ignored = client
        .call(|id| ClientRequest::TurnStart {
            request_id: id,
            params: TurnStartParams {
                thread_id: "thread-1".to_string(),
                input: vec![UserInput::Text {
                    text: "break it".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start succeeds");

    let notification = expect_notification(&mut events).await;
    let ServerNotification::Error(error) = notification else {
        panic!("expected an error notification, got {notification:?}");
    };
    assert_eq!(error.error.message, "backend exploded");
    assert!(!error.will_retry);
}

#[tokio::test]
async fn status_notifications_reach_the_ui_channel() {
    let (client, mut events) = codex_gui_bridge::start(mock_flags("status-notifications"))
        .await
        .expect("mock app-server starts");

    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");
    let _ignored = client
        .call(|id| ClientRequest::ThreadStart {
            request_id: id,
            params: ThreadStartParams::default(),
        })
        .await
        .expect("thread/start succeeds");
    let _ignored = client
        .call(|id| ClientRequest::TurnStart {
            request_id: id,
            params: TurnStartParams {
                thread_id: "thread-1".to_string(),
                input: vec![UserInput::Text {
                    text: "status".to_string(),
                    text_elements: Vec::new(),
                }],
                ..TurnStartParams::default()
            },
        })
        .await
        .expect("turn/start succeeds");

    // Every spec section 9 status family arrives decoded, in order.
    let notification = expect_notification(&mut events).await;
    let ServerNotification::TurnPlanUpdated(plan) = notification else {
        panic!("expected a plan notification, got {notification:?}");
    };
    assert_eq!(plan.explanation.as_deref(), Some("shipping the fix"));
    assert_eq!(plan.plan.len(), 2);

    let notification = expect_notification(&mut events).await;
    let ServerNotification::AccountUpdated(account) = notification else {
        panic!("expected an account notification, got {notification:?}");
    };
    assert_eq!(
        serde_json::to_value(&account).expect("account serializes"),
        serde_json::json!({
            "authMode": "chatgpt",
            "planType": "pro"
        })
    );

    let notification = expect_notification(&mut events).await;
    let ServerNotification::ModelRerouted(rerouted) = notification else {
        panic!("expected a reroute notification, got {notification:?}");
    };
    assert_eq!(rerouted.from_model, "gpt-5");
    assert_eq!(rerouted.to_model, "gpt-5-mini");

    let notification = expect_notification(&mut events).await;
    let ServerNotification::Warning(warning) = notification else {
        panic!("expected a warning notification, got {notification:?}");
    };
    assert_eq!(warning.message, "rate limit approaching");

    let notification = expect_notification(&mut events).await;
    let ServerNotification::DeprecationNotice(notice) = notification else {
        panic!("expected a deprecation notification, got {notification:?}");
    };
    assert_eq!(notice.summary, "old flag is deprecated");
}

#[tokio::test]
async fn config_read_and_batch_write_round_trip() {
    let (client, _events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");
    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The config value layer mirrors config.toml: snake_case keys and
    // kebab-case enum values, exactly what the settings panel drafts.
    let config = client
        .call(|id| ClientRequest::ConfigRead {
            request_id: id,
            params: ConfigReadParams {
                include_layers: false,
                cwd: None,
            },
        })
        .await
        .expect("config/read succeeds");
    assert_eq!(config["config"]["model"], "mock-model");
    assert_eq!(config["config"]["approval_policy"], "on-request");
    assert_eq!(config["config"]["sandbox_mode"], "read-only");

    // A well-formed batch write (camelCase params, upsert edits, hot reload)
    // is accepted; the mock rejects wrong shapes with an RPC error.
    let written = client
        .call(|id| ClientRequest::ConfigBatchWrite {
            request_id: id,
            params: ConfigBatchWriteParams {
                edits: vec![ConfigEdit {
                    key_path: "model".to_string(),
                    value: serde_json::json!("mock-model-2"),
                    merge_strategy: MergeStrategy::Upsert,
                }],
                file_path: None,
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite succeeds");
    assert_eq!(written["status"], "ok");

    // The no-op write the GUI must never send is rejected.
    let rejected = client
        .call(|id| ClientRequest::ConfigBatchWrite {
            request_id: id,
            params: ConfigBatchWriteParams {
                edits: Vec::new(),
                file_path: None,
                expected_version: None,
                reload_user_config: false,
            },
        })
        .await;
    assert!(rejected.is_err());
}

#[tokio::test]
async fn api_key_login_and_logout_round_trip() {
    let (client, _events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");
    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The mock starts signed in as chatgpt.
    let account = client
        .call(|id| ClientRequest::GetAccount {
            request_id: id,
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read succeeds");
    assert_eq!(account["account"]["type"], "chatgpt");

    // API-key login flips the account to the apiKey mode.
    let login = client
        .call(|id| ClientRequest::LoginAccount {
            request_id: id,
            params: LoginAccountParams::ApiKey {
                api_key: "sk-mock".to_string(),
            },
        })
        .await
        .expect("account/login/start succeeds");
    assert_eq!(login["type"], "apiKey");
    let account = client
        .call(|id| ClientRequest::GetAccount {
            request_id: id,
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read succeeds");
    assert_eq!(account["account"]["type"], "apiKey");

    // Logout clears the badge for good.
    let logout = client
        .call(|id| ClientRequest::LogoutAccount {
            request_id: id,
            params: None,
        })
        .await
        .expect("account/logout succeeds");
    assert_eq!(logout, serde_json::json!({}));
    let account = client
        .call(|id| ClientRequest::GetAccount {
            request_id: id,
            params: GetAccountParams {
                refresh_token: false,
            },
        })
        .await
        .expect("account/read succeeds");
    assert_eq!(account["account"], serde_json::Value::Null);

    // A login without a key is rejected as a wire-shape bug.
    let rejected = client
        .call(|id| ClientRequest::LoginAccount {
            request_id: id,
            params: LoginAccountParams::ApiKey {
                api_key: String::new(),
            },
        })
        .await;
    assert!(rejected.is_err());
}

#[tokio::test]
async fn provider_table_write_round_trip() {
    let (client, _events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");
    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The provider switch the settings panel assembles: one table upsert
    // plus the top-level pointers, all with official vendor endpoints.
    let written = client
        .call(|id| ClientRequest::ConfigBatchWrite {
            request_id: id,
            params: ConfigBatchWriteParams {
                edits: vec![
                    ConfigEdit {
                        key_path: "model_providers.deepseek".to_string(),
                        value: serde_json::json!({
                            "name": "DeepSeek",
                            "base_url": "https://api.deepseek.com",
                            "experimental_bearer_token": "sk-mock"
                        }),
                        merge_strategy: MergeStrategy::Upsert,
                    },
                    ConfigEdit {
                        key_path: "model_provider".to_string(),
                        value: serde_json::json!("deepseek"),
                        merge_strategy: MergeStrategy::Upsert,
                    },
                    ConfigEdit {
                        key_path: "model".to_string(),
                        value: serde_json::json!("deepseek-v4-flash"),
                        merge_strategy: MergeStrategy::Upsert,
                    },
                ],
                file_path: None,
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await
        .expect("config/batchWrite succeeds");
    assert_eq!(written["status"], "ok");
}

#[tokio::test]
async fn skills_list_round_trip_and_toggle() {
    let (client, _events) = codex_gui_bridge::start(mock_flags("basic-turn"))
        .await
        .expect("mock app-server starts");
    let _ignored = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: initialize_params(),
        })
        .await
        .expect("initialize succeeds");

    // The inventory the composer picker and the settings toggles consume.
    let listed = client
        .call(|id| ClientRequest::SkillsList {
            request_id: id,
            params: SkillsListParams::default(),
        })
        .await
        .expect("skills/list succeeds");
    let listed = serde_json::from_value::<SkillsListResponse>(listed).expect("valid payload");
    let names: Vec<&str> = listed.data[0]
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["mock-skill", "off-skill"]);
    assert!(listed.data[0].skills[0].enabled);
    assert!(!listed.data[0].skills[1].enabled);

    // Disabling by name round-trips the effective state.
    let written = client
        .call(|id| ClientRequest::SkillsConfigWrite {
            request_id: id,
            params: SkillsConfigWriteParams {
                path: None,
                name: Some("mock-skill".to_string()),
                enabled: false,
            },
        })
        .await
        .expect("skills/config/write succeeds");
    assert_eq!(written["effectiveEnabled"], serde_json::json!(false));
}
