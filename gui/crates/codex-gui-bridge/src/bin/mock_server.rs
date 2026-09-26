//! A fake `codex app-server` used by bridge integration tests.
//!
//! std-only on purpose: it is spawned as a child process by the tests, so it
//! must not depend on the async runtime under test.
//!
//! Scenarios (selected via `--scenario <name>`, default `basic-turn`):
//! - `basic-turn`: answers `initialize`, `thread/start`, and `turn/start`;
//!   after `turn/start` it streams a commandExecution item followed by an
//!   agentMessage item with deltas.
//! - `approval`: same, but after `turn/start` it first emits a
//!   command-execution approval request and holds the item stream back until
//!   the GUI's decision reply arrives, mirroring the real gating.
//! - `error`: answers `initialize`, `thread/start`, and `turn/start`, but
//!   then emits an `error` notification instead of any item stream.
//!
//! Independently of the scenario, the plugin/marketplace family the Skill
//! market tab speaks is served too: `plugin/list` and `plugin/read` describe
//! one `pdf-tools@local-market` plugin, `plugin/install`/`plugin/uninstall`
//! flip its installed state, `marketplace/add` acknowledges a source, and
//! `skills/list` materializes the plugin's `pdf-split` skill while installed.

use std::io::BufRead;
use std::io::Write;

const APPROVAL_REQUEST_ID: &str = "srv-approval-1";

/// The account state the login/logout RPCs mutate; `account/read` mirrors it
/// so tests can round-trip the whole sign-in flow.
#[derive(Clone, Copy)]
enum MockAccount {
    Chatgpt,
    ApiKey,
    SignedOut,
}

fn main() {
    // argv mirrors the real backend: `codex-gui-mock-server app-server ...
    // --scenario <name>`. The positional `app-server` subcommand is consumed
    // (and required) just like the real CLI, then flags are scanned.
    let args: Vec<String> = std::env::args().collect();
    assert!(
        args.get(1)
            .is_some_and(|subcommand| subcommand == "app-server"),
        "expected the `app-server` subcommand like the real codex CLI"
    );
    let mut scenario = "basic-turn".to_string();
    for pair in args[2..].chunks(2) {
        if pair[0] == "--scenario"
            && let Some(name) = pair.get(1)
        {
            scenario = name.clone();
        }
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    // Set while the approval request is outstanding; the item stream resumes
    // once the client's decision reply with the matching id arrives.
    let mut awaiting_decision = false;
    let mut account = MockAccount::Chatgpt;
    // The Skill market's installed state; `skills/list` materializes the
    // plugin's `pdf-split` skill while it is set.
    let mut market_installed = false;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if message.get("method").is_none() {
            // A client reply (JSONRPCResponse/Error): the approval gate.
            if awaiting_decision
                && message.get("id").and_then(|id| id.as_str()) == Some(APPROVAL_REQUEST_ID)
            {
                awaiting_decision = false;
                run_item_stream(&mut stdout);
            }
            continue;
        }

        let Some(method) = message.get("method").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(id) = message.get("id").cloned() else {
            continue;
        };

        match method {
            "initialize" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "userAgent": "mock",
                        "codexHome": "/tmp/codex-mock",
                        "platformFamily": "unix",
                        "platformOs": "linux"
                    }
                }),
            ),
            "thread/start" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {"thread": {"id": "thread-1"}}
                }),
            ),
            "thread/list" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "data": [thread_json("thread-1", "resumable conversation")],
                        "nextCursor": null,
                        "backwardsCursor": null
                    }
                }),
            ),
            "thread/resume" => {
                let thread_id = message["params"]["threadId"]
                    .as_str()
                    .unwrap_or("thread-1")
                    .to_string();
                let mut thread = thread_json(&thread_id, "resumable conversation");
                thread["turns"] = serde_json::json!([
                    {
                        "id": "turn-h1",
                        "items": [
                            {
                                "type": "userMessage",
                                "id": "item-h1",
                                "content": [
                                    {"type": "text", "text": "hello from history", "textElements": []}
                                ]
                            },
                            {"type": "agentMessage", "id": "item-h2", "text": "History replay"}
                        ],
                        "status": "completed",
                        "error": null,
                        "startedAt": null,
                        "completedAt": null,
                        "durationMs": null
                    }
                ]);
                write_frame(
                    &mut stdout,
                    serde_json::json!({
                        "id": id,
                        "result": {
                            "thread": thread,
                            "model": "mock-model",
                            "modelProvider": "mock",
                            "serviceTier": null,
                            "disabledPluginIds": [],
                            "cwd": "/tmp",
                            "runtimeWorkspaceRoots": [],
                            "instructionSources": [],
                            "approvalPolicy": "on-request",
                            "approvalsReviewer": "user",
                            "sandbox": {"type": "readOnly"},
                            "activePermissionProfile": null,
                            "reasoningEffort": null,
                            "collaborationMode": null,
                            "multiAgentMode": "explicitRequestOnly",
                            "initialTurnsPage": null,
                            "turnsBackwardsCursor": null,
                            "itemsBackwardsCursor": null
                        }
                    }),
                );
            }
            "thread/archive" => {
                write_frame(&mut stdout, serde_json::json!({"id": id, "result": {}}));
                write_frame(
                    &mut stdout,
                    serde_json::json!({
                        "method": "thread/archived",
                        "params": {"threadId": message["params"]["threadId"]}
                    }),
                );
            }
            "model/list" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "data": [model_json()],
                        "nextCursor": null
                    }
                }),
            ),
            "account/read" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "account": match account {
                            MockAccount::Chatgpt => serde_json::json!({
                                "type": "chatgpt",
                                "email": "dev@example.com",
                                "planType": "pro"
                            }),
                            MockAccount::ApiKey => serde_json::json!({"type": "apiKey"}),
                            MockAccount::SignedOut => serde_json::Value::Null,
                        },
                        "requiresOpenaiAuth": false,
                        "workspaceRouting": null
                    }
                }),
            ),
            "account/login/start" => {
                // The GUI must send the tagged apiKey variant with a
                // non-empty key; anything else is a wire-shape bug, so
                // reject it loudly.
                let api_key = message["params"]["apiKey"].as_str();
                if message["params"]["type"] == "apiKey"
                    && api_key.is_some_and(|key| !key.is_empty())
                {
                    account = MockAccount::ApiKey;
                    write_frame(
                        &mut stdout,
                        serde_json::json!({"id": id, "result": {"type": "apiKey"}}),
                    );
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the account/login/start shape"
                            }
                        }),
                    );
                }
            }
            "account/logout" => {
                account = MockAccount::SignedOut;
                write_frame(&mut stdout, serde_json::json!({"id": id, "result": {}}));
            }
            "skills/list" => {
                let mut skills = vec![
                    skill_json("mock-skill", "The mock skill for tests", "user", true, None),
                    skill_json("off-skill", "A disabled mock skill", "repo", false, None),
                ];
                if market_installed {
                    skills.push(skill_json(
                        "pdf-split",
                        "Split a PDF into pages",
                        "user",
                        true,
                        Some("pdf-tools@local-market"),
                    ));
                }
                write_frame(
                    &mut stdout,
                    serde_json::json!({
                        "id": id,
                        "result": {
                            "data": [{"cwd": "/tmp", "errors": [], "skills": skills}]
                        }
                    }),
                );
            }
            "skills/config/write" => {
                // The GUI must select by name (or path) with a boolean
                // target state; anything else is a wire-shape bug, so
                // reject it loudly.
                let has_selector =
                    message["params"]["name"].is_string() || message["params"]["path"].is_string();
                let enabled = message["params"]["enabled"].clone();
                if has_selector && enabled.is_boolean() {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({"id": id, "result": {"effectiveEnabled": enabled}}),
                    );
                    // Not a real file change, but the toggle is the same
                    // invalidation signal the GUI must honor.
                    write_frame(
                        &mut stdout,
                        serde_json::json!({"method": "skills/changed", "params": {}}),
                    );
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the skills/config/write shape"
                            }
                        }),
                    );
                }
            }
            "plugin/list" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "marketplaces": [{
                            "name": "local-market",
                            "path": "/tmp/codex-mock/marketplace.json",
                            "interface": {"displayName": "本地市场"},
                            "plugins": [plugin_summary(market_installed)]
                        }],
                        "marketplaceLoadErrors": [],
                        "featuredPluginIds": []
                    }
                }),
            ),
            "plugin/read" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {"plugin": plugin_detail(market_installed)}
                }),
            ),
            "plugin/install" => {
                // The GUI must address the plugin by name plus one marketplace
                // selector (a local path or the remote name).
                let has_marketplace = message["params"]["marketplacePath"].is_string()
                    || message["params"]["remoteMarketplaceName"].is_string();
                let plugin_name = message["params"]["pluginName"].as_str();
                if has_marketplace && plugin_name == Some("pdf-tools") {
                    market_installed = true;
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "result": {"authPolicy": "ON_USE", "appsNeedingAuth": []}
                        }),
                    );
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the plugin/install shape"
                            }
                        }),
                    );
                }
            }
            "plugin/uninstall" => {
                if message["params"]["pluginId"] == "pdf-tools@local-market" {
                    market_installed = false;
                    write_frame(&mut stdout, serde_json::json!({"id": id, "result": {}}));
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the plugin/uninstall shape"
                            }
                        }),
                    );
                }
            }
            "marketplace/add" => {
                let source = message["params"]["source"].as_str().unwrap_or_default();
                if source.is_empty() {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the marketplace/add shape"
                            }
                        }),
                    );
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "result": {
                                "marketplaceName": "local-market",
                                "installedRoot": "/tmp/codex-mock/marketplaces/local-market",
                                "alreadyAdded": false
                            }
                        }),
                    );
                }
            }
            "config/read" => write_frame(
                &mut stdout,
                serde_json::json!({
                    "id": id,
                    "result": {
                        "config": {
                            "model": "mock-model",
                            "approval_policy": "on-request",
                            "sandbox_mode": "read-only",
                            "model_reasoning_effort": "medium",
                            "model_verbosity": "medium",
                            "web_search": "cached"
                        },
                        "origins": {}
                    }
                }),
            ),
            "config/batchWrite" => {
                // The GUI must send camelCase params with a non-empty edits
                // list of upsert edits and the hot-reload flag; anything else
                // is a wire-shape bug, so reject it loudly.
                let edits = message["params"]["edits"].as_array();
                let well_formed = message["params"]["reloadUserConfig"]
                    == serde_json::Value::Bool(true)
                    && edits.is_some_and(|list| {
                        !list.is_empty()
                            && list.iter().all(|edit| {
                                let key = edit["keyPath"].as_str().unwrap_or_default();
                                edit.get("keyPath").is_some()
                                    && edit.get("value").is_some()
                                    && edit["mergeStrategy"] == "upsert"
                                    // Provider table entries must be objects.
                                    && (!key.starts_with("model_providers.")
                                        || edit["value"].is_object())
                            })
                    });
                if well_formed {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "result": {
                                "status": "ok",
                                "version": "2",
                                "filePath": "/tmp/codex-mock/config.toml",
                                "overriddenMetadata": null
                            }
                        }),
                    );
                } else {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": "mock rejected the config/batchWrite shape"
                            }
                        }),
                    );
                }
            }
            "turn/start" => {
                write_frame(&mut stdout, serde_json::json!({"id": id, "result": {}}));
                if scenario == "error" {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "method": "error",
                            "params": {
                                "error": {"message": "backend exploded", "codexErrorInfo": null},
                                "willRetry": false,
                                "threadId": "thread-1",
                                "turnId": "turn-1"
                            }
                        }),
                    );
                    continue;
                }
                if scenario == "status-notifications" {
                    for notification in status_notifications() {
                        write_frame(&mut stdout, notification);
                    }
                    continue;
                }
                write_frame(
                    &mut stdout,
                    serde_json::json!({
                        "method": "item/started",
                        "params": {
                            "item": {
                                "type": "userMessage",
                                "id": "item-0",
                                "content": [{"type": "text", "text": "hi", "textElements": []}]
                            },
                            "threadId": "thread-1",
                            "turnId": "turn-1",
                            "startedAtMs": 0
                        }
                    }),
                );
                if scenario == "approval" {
                    write_frame(
                        &mut stdout,
                        serde_json::json!({
                            "method": "item/commandExecution/requestApproval",
                            "id": APPROVAL_REQUEST_ID,
                            "params": {
                                "threadId": "thread-1",
                                "turnId": "turn-1",
                                "itemId": "item-approval-1",
                                "startedAtMs": 0,
                                "command": "ls -la",
                                "reason": "needs approval before running"
                            }
                        }),
                    );
                    awaiting_decision = true;
                } else {
                    run_item_stream(&mut stdout);
                }
            }
            // Any other method gets an empty acknowledgment.
            _ => write_frame(&mut stdout, serde_json::json!({"id": id, "result": {}})),
        }
    }
}

/// Emits the commandExecution item and the streaming agentMessage item.
fn run_item_stream(stdout: &mut std::io::Stdout) {
    write_frame(
        stdout,
        serde_json::json!({
            "method": "item/started",
            "params": {
                "item": {
                    "type": "commandExecution",
                    "id": "item-cmd-1",
                    "command": "echo hi",
                    "cwd": "/tmp",
                    "status": "inProgress",
                    "commandActions": []
                },
                "threadId": "thread-1",
                "turnId": "turn-1",
                "startedAtMs": 1
            }
        }),
    );
    write_frame(
        stdout,
        serde_json::json!({
            "method": "item/commandExecution/outputDelta",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-cmd-1",
                "delta": "hi\n"
            }
        }),
    );
    write_frame(
        stdout,
        serde_json::json!({
            "method": "item/completed",
            "params": {
                "item": {
                    "type": "commandExecution",
                    "id": "item-cmd-1",
                    "command": "echo hi",
                    "cwd": "/tmp",
                    "status": "completed",
                    "commandActions": [],
                    "aggregatedOutput": "hi\n",
                    "exitCode": 0,
                    "durationMs": 5
                },
                "threadId": "thread-1",
                "turnId": "turn-1",
                "completedAtMs": 2
            }
        }),
    );
    write_frame(
        stdout,
        serde_json::json!({
            "method": "item/started",
            "params": {
                "item": {"type": "agentMessage", "id": "item-1", "text": ""},
                "threadId": "thread-1",
                "turnId": "turn-1",
                "startedAtMs": 1
            }
        }),
    );
    for delta in ["Hello", " ", "world"] {
        write_frame(
            stdout,
            serde_json::json!({
                "method": "item/agentMessage/delta",
                "params": {
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "delta": delta
                }
            }),
        );
    }
    write_frame(
        stdout,
        serde_json::json!({
            "method": "item/completed",
            "params": {
                "item": {"type": "agentMessage", "id": "item-1", "text": "Hello world"},
                "threadId": "thread-1",
                "turnId": "turn-1",
                "completedAtMs": 2
            }
        }),
    );
}

/// A full `Thread` payload; every non-optional field is present because the
/// v2 deserializer does not default them.
fn thread_json(id: &str, preview: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "extra": null,
        "sessionId": "session-1",
        "forkedFromId": null,
        "parentThreadId": null,
        "preview": preview,
        "ephemeral": false,
        "modelProvider": "mock",
        "model": null,
        "reasoningEffort": null,
        "createdAt": 1,
        "updatedAt": 2,
        "recencyAt": null,
        "status": {"type": "idle"},
        "path": null,
        "cwd": "/tmp",
        "cliVersion": "0.0.0",
        "originator": null,
        "source": "cli",
        "canAcceptDirectInput": null,
        "threadSource": null,
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": null,
        "daybreakEnabled": null,
        "turns": []
    })
}

/// A full `SkillMetadata` payload; every non-optional field is present.
fn skill_json(
    name: &str,
    description: &str,
    scope: &str,
    enabled: bool,
    plugin_id: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "description": description,
        "path": format!("/tmp/skills/{name}/SKILL.md"),
        "scope": scope,
        "enabled": enabled,
        "pluginId": plugin_id
    })
}

/// A full `PluginSummary` payload for the one mock market plugin.
fn plugin_summary(installed: bool) -> serde_json::Value {
    serde_json::json!({
        "id": "pdf-tools@local-market",
        "remotePluginId": null,
        "name": "pdf-tools",
        "shareContext": null,
        "source": {"type": "local", "path": "/tmp/codex-mock/plugins/pdf-tools"},
        "installed": installed,
        "enabled": installed,
        "installPolicy": "AVAILABLE",
        "installPolicySource": null,
        "authPolicy": "ON_USE",
        "interface": plugin_interface(),
        "keywords": []
    })
}

/// A full `PluginDetail` payload; its skills are what the market row
/// expands into.
fn plugin_detail(installed: bool) -> serde_json::Value {
    serde_json::json!({
        "marketplaceName": "local-market",
        "marketplacePath": "/tmp/codex-mock/marketplace.json",
        "summary": plugin_summary(installed),
        "shareUrl": null,
        "description": "PDF utility skills",
        "skills": [
            {
                "name": "pdf-split",
                "description": "Split a PDF into pages",
                "shortDescription": null,
                "interface": null,
                "path": null,
                "enabled": true
            },
            {
                "name": "pdf-merge",
                "description": "Merge several PDFs into one",
                "shortDescription": null,
                "interface": null,
                "path": null,
                "enabled": true
            }
        ],
        "onboardingSkill": null,
        "hooks": [],
        "apps": [],
        "appTemplates": [],
        "mcpServers": [],
        "scheduledTasks": null
    })
}

/// A full `PluginInterface` payload; every optional key is present because
/// the v2 deserializer does not default them.
fn plugin_interface() -> serde_json::Value {
    serde_json::json!({
        "displayName": "PDF 工具",
        "shortDescription": "Split and merge PDF files",
        "longDescription": null,
        "developerName": null,
        "category": null,
        "capabilities": [],
        "websiteUrl": null,
        "privacyPolicyUrl": null,
        "termsOfServiceUrl": null,
        "defaultPrompt": null,
        "brandColor": null,
        "composerIcon": null,
        "composerIconUrl": null,
        "logo": null,
        "logoDark": null,
        "logoUrl": null,
        "logoUrlDark": null,
        "screenshots": [],
        "screenshotUrls": []
    })
}

/// A full `Model` catalog entry; every non-optional field is present.
fn model_json() -> serde_json::Value {
    serde_json::json!({
        "id": "mock-model-1",
        "model": "mock-model",
        "upgrade": null,
        "upgradeInfo": null,
        "availabilityNux": null,
        "displayName": "Mock Model",
        "description": "The mock catalog entry",
        "hidden": false,
        "supportedReasoningEfforts": [],
        "defaultReasoningEffort": "medium",
        "multiAgentVersion": null,
        "isDefault": true
    })
}

fn write_frame(stdout: &mut impl Write, frame: serde_json::Value) {
    // A broken pipe means the test harness died; crashing the mock is the
    // clearest signal.
    #[allow(clippy::expect_used)]
    {
        writeln!(stdout, "{frame}").expect("mock server stdout write");
        stdout.flush().expect("mock server stdout flush");
    }
}

/// The `status-notifications` stream: one notification per spec section 9
/// status family, in wire form.
fn status_notifications() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "method": "turn/plan/updated",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "explanation": "shipping the fix",
                "plan": [
                    {"step": "reproduce", "status": "completed"},
                    {"step": "patch", "status": "inProgress"}
                ]
            }
        }),
        serde_json::json!({
            "method": "account/updated",
            "params": {"authMode": "chatgpt", "planType": "pro"}
        }),
        serde_json::json!({
            "method": "model/rerouted",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "fromModel": "gpt-5",
                "toModel": "gpt-5-mini",
                "reason": "highRiskCyberActivity"
            }
        }),
        serde_json::json!({
            "method": "warning",
            "params": {"threadId": "thread-1", "message": "rate limit approaching"}
        }),
        serde_json::json!({
            "method": "deprecationNotice",
            "params": {"summary": "old flag is deprecated", "details": null}
        }),
    ]
}
