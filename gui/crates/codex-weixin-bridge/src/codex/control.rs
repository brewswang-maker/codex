//! WeChat command helpers: model browsing (`/model`), thread browsing
//! (`/resume`), and the shared help text.
//!
//! Pure parsing lives here so it is unit-testable; stateful dispatch stays
//! in [`crate::codex::runtime`].

use serde_json::Value;

use crate::codex::session::CodexBridge;
use crate::error::{BridgeError, Result};

/// How many threads `/resume` lists.
pub const RESUME_LIST_LIMIT: u32 = 5;

pub fn help_text() -> String {
    "命令列表：\n\
     /new — 重置会话，下条消息开新线程\n\
     /stop — 中断当前任务\n\
     /status — 查看运行状态\n\
     /model [序号] — 查看或选择模型（新会话生效）\n\
     /resume [序号] — 查看最近线程或切回某个线程\n\
     /bindquest 名称 — 把当前线程归入指定 Quest 组\n\
     /help — 显示本帮助\n\
     审批请求用「y」/「n」/「y 反馈」回复；其他任何文本都会作为任务交给 codex 执行。"
        .to_string()
}

/// Lists model ids from `model/list`.
pub async fn model_list(codex: &CodexBridge) -> Result<Vec<String>> {
    let value = codex
        .client()
        .call(
            |request_id| codex_app_server_protocol::ClientRequest::ModelList {
                request_id,
                params: codex_app_server_protocol::ModelListParams::default(),
            },
        )
        .await?;
    Ok(value["data"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|model| {
                    model["id"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| model["model"].as_str().map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Renders the `/model` picker text for the given id list.
pub fn model_list_text(models: &[String]) -> String {
    if models.is_empty() {
        return "未获取到可用模型列表。".to_string();
    }
    let mut lines = vec!["可用模型（发送 /model 序号 选择，新会话生效）：".to_string()];
    for (index, model) in models.iter().enumerate() {
        lines.push(format!("{}. {model}", index + 1));
    }
    lines.join("\n")
}

/// Parses `/model <n>` into a 1-based pick.
pub fn parse_model_pick(arg: &str) -> Option<usize> {
    arg.trim().parse::<usize>().ok().filter(|n| *n >= 1)
}

/// Lists recent threads `(id, title)` from `thread/list`, newest first.
pub async fn thread_list(codex: &CodexBridge, limit: u32) -> Result<Vec<(String, String)>> {
    let value = codex
        .client()
        .call(
            |request_id| codex_app_server_protocol::ClientRequest::ThreadList {
                request_id,
                params: codex_app_server_protocol::ThreadListParams {
                    limit: Some(limit),
                    // The params struct has no `Default`; mirror the e2e helper.
                    cursor: None,
                    excluded_thread_ids: None,
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
                },
            },
        )
        .await?;
    Ok(thread_pairs(&value))
}

/// Extracts `(id, title)` pairs from a `thread/list` response.
pub fn thread_pairs(value: &Value) -> Vec<(String, String)> {
    value["data"]
        .as_array()
        .map(|threads| {
            threads
                .iter()
                .filter_map(|thread| {
                    let id = thread["id"].as_str()?.to_string();
                    let title = thread["name"]
                        .as_str()
                        .or_else(|| thread["title"].as_str())
                        .unwrap_or("(无标题)")
                        .to_string();
                    Some((id, title))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Renders the `/resume` picker text.
pub fn thread_list_text(threads: &[(String, String)]) -> String {
    if threads.is_empty() {
        return "没有可恢复的线程。".to_string();
    }
    let mut lines = vec!["最近的线程（发送 /resume 序号 切换）：".to_string()];
    for (index, (_id, title)) in threads.iter().enumerate() {
        lines.push(format!("{}. {title}", index + 1));
    }
    lines.join("\n")
}

/// Parses `/resume <n>` into a 1-based pick.
pub fn parse_resume_pick(arg: &str) -> Option<usize> {
    arg.trim().parse::<usize>().ok().filter(|n| *n >= 1)
}

/// Fixed notice for contacts that are not on the allowlist yet.
pub fn unauthorized_notice() -> String {
    "你好，此机器人尚未授权当前联系人使用。\n\
     请由机器人管理员在 GUI 面板放行，或运行 `codex-weixin-bridge allow <联系人>` 后再试。"
        .to_string()
}

/// Validates a contact id for the `allow` CLI (defensive: the value ends up
/// in a JSON file and shell output).
pub fn sanitize_contact(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err(BridgeError::Other("联系人标识为空或过长".to_string()));
    }
    Ok(trimmed.to_string())
}

/// Binds a thread to the named Quest section, creating the section when it
/// does not exist yet; returns the user-facing confirmation.
pub async fn bind_thread_to_quest(
    codex: &CodexBridge,
    thread_id: &str,
    name: &str,
) -> Result<String> {
    let listed = codex
        .client()
        .call(
            |request_id| codex_app_server_protocol::ClientRequest::ThreadSectionList {
                request_id,
                params: codex_app_server_protocol::ThreadSectionListParams {
                    cursor: None,
                    limit: None,
                },
            },
        )
        .await?;
    let section_id = match find_section(&listed, name) {
        Some(section_id) => section_id,
        None => {
            let created = codex
                .client()
                .call(
                    |request_id| codex_app_server_protocol::ClientRequest::ThreadSectionCreate {
                        request_id,
                        params: codex_app_server_protocol::ThreadSectionCreateParams {
                            name: name.to_string(),
                            appearance: None,
                        },
                    },
                )
                .await?;
            created["section"]["id"]
                .as_str()
                .ok_or_else(|| {
                    BridgeError::Other("threadSection/create 响应缺少 section.id".to_string())
                })?
                .to_string()
        }
    };
    codex
        .client()
        .call(
            |request_id| codex_app_server_protocol::ClientRequest::ThreadSectionMove {
                request_id,
                params: codex_app_server_protocol::ThreadSectionMoveParams {
                    thread_id: thread_id.to_string(),
                    section_id: Some(section_id),
                    before_thread_id: None,
                },
            },
        )
        .await?;
    Ok(format!("已把线程加入 Quest「{name}」。"))
}

/// Finds the section id whose name matches exactly (after trimming).
pub fn find_section(listed: &Value, name: &str) -> Option<String> {
    listed["data"].as_array()?.iter().find_map(|section| {
        if section["name"].as_str()? == name {
            section["id"].as_str().map(str::to_string)
        } else {
            None
        }
    })
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
