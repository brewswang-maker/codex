//! Bridge orchestration: the WeChat long poll, command dispatch, the
//! per-contact serial session queue, and the codex event pump.
//!
//! Layout: a dedicated task runs `get_updates` (a 35s long poll that must not
//! be cancelled by codex event traffic) and forwards responses over an mpsc
//! channel; the main loop selects between that channel and the app-server
//! event stream, mutating runtime state only between awaits.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use codex_app_server_protocol::{
    CommandExecutionApprovalDecision, CommandExecutionRequestApprovalResponse,
    FileChangeApprovalDecision, FileChangeRequestApprovalResponse, ServerNotification,
    ServerRequest, ThreadItem,
};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::codex::progress::{clean_for_wechat, describe_duration, final_reply, summarize_item};
use crate::codex::session::{CodexBridge, SessionStore};
use crate::error::{BridgeError, Result};
use crate::wechat::auth::Store;
use crate::wechat::client::WeixinClient;
use crate::wechat::types::{self, GetUpdatesResp, WeixinMessage, generate_client_id};

/// Global cap on codex turns running at once (plan: guard against runaway).
pub const MAX_CONCURRENT_TURNS: usize = 2;
/// WeChat long poll retries before the backoff grows.
const POLL_FAILURE_BACKOFF_THRESHOLD: u32 = 3;

/// In-memory session state for one contact; `thread_id` mirrors the
/// persisted mapping while `turn_id` tracks the live turn.
#[derive(Debug, Default)]
struct ContactSession {
    thread_id: Option<String>,
    turn_id: Option<String>,
    queue: VecDeque<String>,
    /// Most recent inbound `context_token`; echoed on replies.
    context_token: Option<String>,
}

pub struct Runtime {
    wx: WeixinClient,
    sessions: SessionStore,
    codex: CodexBridge,
    live: HashMap<String, ContactSession>,
    by_thread: HashMap<String, String>,
    active_turns: usize,
}

impl Runtime {
    fn new(wx: WeixinClient, sessions: SessionStore, codex: CodexBridge) -> Self {
        Self {
            wx,
            sessions,
            codex,
            live: HashMap::new(),
            by_thread: HashMap::new(),
            active_turns: 0,
        }
    }

    async fn reply(&self, contact: &str, text: &str) {
        let context_token = self
            .live
            .get(contact)
            .and_then(|s| s.context_token.clone());
        let msg = WeixinMessage::text_reply(
            contact,
            text,
            context_token.as_deref(),
            &generate_client_id(),
        );
        if let Err(err) = self.wx.send_message(msg).await {
            tracing::warn!(contact, %err, "微信回复发送失败");
        }
    }

    async fn handle_weixin(&mut self, resp: GetUpdatesResp) -> Result<()> {
        if resp.is_api_error() {
            // A stale token (-14) or a server-side rejection is fatal for the
            // daemon: credentials must be refreshed via `login`.
            return Err(BridgeError::Api {
                ret: resp.ret.unwrap_or_default(),
                errcode: resp.errcode,
                errmsg: resp.errmsg,
            });
        }
        for msg in resp.msgs.unwrap_or_default() {
            self.handle_message(msg).await;
        }
        Ok(())
    }

    async fn handle_message(&mut self, msg: WeixinMessage) {
        // Only user-authored direct chats drive tasks; bot echoes would loop,
        // group chats are out of scope for now, media arrives in D3.
        if msg.message_type != Some(types::message_type::USER) {
            return;
        }
        if msg.group_id.as_ref().is_some_and(|g| !g.is_empty()) {
            return;
        }
        let Some(contact) = msg.from_user_id.clone().filter(|c| !c.is_empty()) else {
            return;
        };
        let text = msg.text();
        {
            let session = self.live.entry(contact.clone()).or_default();
            session.context_token = msg.context_token.clone();
        }
        if text.trim().is_empty() {
            self.reply(&contact, "语音、图片与文件消息将在后续版本支持，请先发送文本。")
                .await;
            return;
        }
        let trimmed = text.trim().to_string();
        if let Some(reply) = self.handle_command(&contact, &trimmed).await {
            self.reply(&contact, &reply).await;
            return;
        }
        self.enqueue_task(&contact, trimmed).await;
    }

    /// WeChat command dispatch (`/new` `/stop` `/status` `/help`); plain text
    /// is a task. `None` means the text is not a command.
    async fn handle_command(&mut self, contact: &str, text: &str) -> Option<String> {
        let rest = text.strip_prefix('/')?;
        let (cmd, _arg) = rest.split_once([' ', '\n']).unwrap_or((rest, ""));
        match cmd {
            "new" => {
                let had = self.sessions.unbind(contact).unwrap_or(false);
                let session = self.live.remove(contact);
                if session.is_some_and(|s| s.turn_id.is_some()) {
                    self.active_turns = self.active_turns.saturating_sub(1);
                }
                Some(if had {
                    "已重置：下一条消息将开启新的 codex 线程。".to_string()
                } else {
                    "当前没有历史会话；下一条消息将开启新线程。".to_string()
                })
            }
            "stop" => {
                let (thread_id, turn_id) = match self.live.get(contact) {
                    Some(session) => match (session.thread_id.clone(), session.turn_id.clone()) {
                        (Some(thread_id), Some(turn_id)) => (thread_id, turn_id),
                        _ => return Some("当前没有正在运行的任务。".to_string()),
                    },
                    None => return Some("当前没有正在运行的任务。".to_string()),
                };
                match self.codex.interrupt(&thread_id, &turn_id).await {
                    Ok(()) => Some("已请求中断当前任务。".to_string()),
                    Err(err) => Some(format!("中断失败: {err}")),
                }
            }
            "status" => {
                let session = self.live.get(contact);
                let thread = session
                    .and_then(|s| s.thread_id.clone())
                    .or_else(|| self.sessions.thread_for(contact).map(str::to_string));
                let mut lines = vec!["codex-weixin-bridge 运行中".to_string()];
                match (&thread, session.and_then(|s| s.turn_id.as_ref())) {
                    (Some(_thread), Some(turn)) => {
                        lines.push(format!("当前任务运行中（turn {turn}）"))
                    }
                    (Some(thread), None) => lines.push(format!("绑定线程 {thread}，空闲")),
                    (None, _) => lines.push("尚未开启会话，发任意任务即可开始".to_string()),
                }
                if let Some(session) = session
                    && !session.queue.is_empty()
                {
                    lines.push(format!("队列中还有 {} 条任务", session.queue.len()));
                }
                lines.push(format!(
                    "全局并发 {}/{}",
                    self.active_turns, MAX_CONCURRENT_TURNS
                ));
                Some(lines.join("\n"))
            }
            "help" => Some(
                "命令列表：\n\
                 /new — 重置会话，下条消息开新线程\n\
                 /stop — 中断当前任务\n\
                 /status — 查看运行状态\n\
                 /help — 显示本帮助\n\
                 其他任何文本都会作为任务交给 codex 执行。"
                    .to_string(),
            ),
            "model" | "resume" | "bindquest" => {
                Some(format!("/{cmd} 将在后续版本提供。"))
            }
            _ => Some(format!("未知命令 /{cmd}；发送 /help 查看用法。")),
        }
    }

    async fn enqueue_task(&mut self, contact: &str, text: String) {
        if let Err(err) = self.ensure_thread(contact).await {
            self.reply(contact, &format!("无法准备 codex 会话: {err}"))
                .await;
            return;
        }
        let busy = self
            .live
            .get(contact)
            .is_some_and(|s| s.turn_id.is_some())
            || self.active_turns >= MAX_CONCURRENT_TURNS;
        if busy {
            if let Some(session) = self.live.get_mut(contact) {
                session.queue.push_back(text);
            }
            self.reply(contact, "任务已排队，当前任务完成后自动开始。")
                .await;
        } else {
            self.start_turn(contact, text).await;
        }
    }

    /// Makes sure the contact has a live thread, resuming the persisted one
    /// when the bridge (or app-server) restarted.
    async fn ensure_thread(&mut self, contact: &str) -> Result<String> {
        if let Some(session) = self.live.get(contact)
            && let Some(thread_id) = session.thread_id.clone()
        {
            return Ok(thread_id);
        }
        let persisted = self.sessions.thread_for(contact).map(str::to_string);
        let thread_id = match persisted {
            Some(thread_id) => match self.codex.resume_thread(&thread_id).await {
                Ok(()) => thread_id,
                Err(err) => {
                    tracing::warn!(contact, thread = %thread_id, %err, "resume 失败，改用新线程");
                    self.codex.start_thread(None).await?
                }
            },
            None => self.codex.start_thread(None).await?,
        };
        self.sessions.bind(contact, &thread_id)?;
        self.by_thread.insert(thread_id.clone(), contact.to_string());
        if let Some(session) = self.live.get_mut(contact) {
            session.thread_id = Some(thread_id.clone());
        }
        Ok(thread_id)
    }

    async fn start_turn(&mut self, contact: &str, text: String) {
        // Queue control guarantees at most one live turn per contact; the
        // guard here keeps that invariant locally checkable.
        let thread_id = match self.live.get(contact) {
            Some(session) if session.turn_id.is_none() => session.thread_id.clone(),
            _ => return,
        };
        let Some(thread_id) = thread_id else {
            return;
        };
        match self.codex.submit_turn(&thread_id, &text).await {
            Ok(turn_id) => {
                self.active_turns += 1;
                if let Some(session) = self.live.get_mut(contact) {
                    session.turn_id = Some(turn_id);
                }
            }
            Err(err) => {
                self.reply(contact, &format!("任务提交失败: {err}")).await;
            }
        }
    }

    async fn on_turn_finished(&mut self, contact: &str) {
        self.active_turns = self.active_turns.saturating_sub(1);
        if let Some(session) = self.live.get_mut(contact) {
            session.turn_id = None;
        }
        if self.active_turns < MAX_CONCURRENT_TURNS
            && let Some(next) = self
                .live
                .get_mut(contact)
                .and_then(|session| session.queue.pop_front())
        {
            self.start_turn(contact, next).await;
        }
    }

    async fn handle_notification(&mut self, notification: ServerNotification) {
        match notification {
            ServerNotification::ItemCompleted(done) => {
                let Some(contact) = self.by_thread.get(&done.thread_id).cloned() else {
                    return;
                };
                if matches!(done.item, ThreadItem::AgentMessage { .. }) {
                    // The final reply rides on TurnCompleted; nothing to send.
                    return;
                }
                if let Some(line) = summarize_item(&done.item) {
                    self.reply(&contact, &line).await;
                }
            }
            ServerNotification::TurnCompleted(done) => {
                let Some(contact) = self.by_thread.get(&done.thread_id).cloned() else {
                    return;
                };
                let duration = describe_duration(done.turn.duration_ms);
                let body = final_reply(&done.turn).map_or_else(
                    || "任务完成，但没有回复文本。".to_string(),
                    |text| clean_for_wechat(&text),
                );
                let reply = if duration.is_empty() {
                    body
                } else {
                    format!("{body}\n\n— {duration}")
                };
                self.reply(&contact, &reply).await;
                self.on_turn_finished(&contact).await;
            }
            ServerNotification::Error(err) => {
                let Some(contact) = self.by_thread.get(&err.thread_id).cloned() else {
                    tracing::warn!(thread = %err.thread_id, message = %err.error.message, "app-server 错误");
                    return;
                };
                if !err.will_retry {
                    self.reply(&contact, &format!("任务出错: {}", err.error.message))
                        .await;
                    self.on_turn_finished(&contact).await;
                }
            }
            _ => {}
        }
    }

    /// Minimal approval posture until D3 forwards approvals to WeChat:
    /// decline explicitly so the turn continues instead of hanging, and tell
    /// the contact what happened.
    async fn handle_server_request(&mut self, request: ServerRequest) {
        match &request {
            ServerRequest::CommandExecutionRequestApproval { params, .. } => {
                let contact = self.by_thread.get(&params.thread_id).cloned();
                let what = params.command.clone().unwrap_or_default();
                let _ = self
                    .codex
                    .client()
                    .respond(
                        &request,
                        serde_json::to_value(CommandExecutionRequestApprovalResponse {
                            decision: CommandExecutionApprovalDecision::Decline,
                        })
                        .unwrap_or(Value::Null),
                    )
                    .await;
                if let Some(contact) = contact {
                    let why = params
                        .reason
                        .clone()
                        .unwrap_or_else(|| "沙箱范围之外的命令".to_string());
                    self.reply(
                        &contact,
                        &format!(
                            "需要审批的操作已被自动拒绝（微信审批转发将在后续版本提供）：{why}\n{what}"
                        ),
                    )
                    .await;
                }
            }
            ServerRequest::FileChangeRequestApproval { params, .. } => {
                let contact = self.by_thread.get(&params.thread_id).cloned();
                let _ = self
                    .codex
                    .client()
                    .respond(
                        &request,
                        serde_json::to_value(FileChangeRequestApprovalResponse {
                            decision: FileChangeApprovalDecision::Decline,
                        })
                        .unwrap_or(Value::Null),
                    )
                    .await;
                if let Some(contact) = contact {
                    let why = params
                        .reason
                        .clone()
                        .unwrap_or_else(|| "沙箱范围之外的文件修改".to_string());
                    self.reply(
                        &contact,
                        &format!("文件修改审批已被自动拒绝（微信审批转发将在后续版本提供）：{why}"),
                    )
                    .await;
                }
            }
            other => {
                tracing::warn!(method = ?other, "不支持的服务端请求，回复错误");
                let _ = self
                    .codex
                    .client()
                    .respond_error_for_id(other.id().clone(), -32601, "bridge 不支持该请求")
                    .await;
            }
        }
    }
}

/// Runs the bridge until the WeChat token expires or the app-server exits.
pub async fn run(store: Store) -> Result<()> {
    let wx = store.load_client()?;
    let (codex, mut events) = CodexBridge::connect().await?;
    let sessions = SessionStore::load(store.codex_home());
    let runtime = Runtime::new(wx, sessions, codex);

    // The long poll lives in its own task: codex event bursts must never
    // cancel an in-flight `get_updates`, or inbound WeChat messages starve.
    let (poll_tx, mut poll_rx) = mpsc::channel::<GetUpdatesResp>(16);
    let poll_client = runtime.wx.clone();
    let poll_store = store.clone();
    tokio::spawn(async move {
        let mut cursor = poll_store.load_sync_buf();
        let mut failures: u32 = 0;
        loop {
            match poll_client.get_updates(&cursor).await {
                Ok(resp) => {
                    failures = 0;
                    if let Some(buf) = resp.get_updates_buf.clone() {
                        cursor = buf;
                        let _ = poll_store.save_sync_buf(&cursor);
                    }
                    if poll_tx.send(resp).await.is_err() {
                        break; // runtime gone: shutdown
                    }
                }
                Err(err) => {
                    failures += 1;
                    tracing::warn!(%err, failures, "微信长轮询失败");
                    let delay = if failures >= POLL_FAILURE_BACKOFF_THRESHOLD {
                        Duration::from_secs(30)
                    } else {
                        Duration::from_secs(2)
                    };
                    tokio::time::sleep(delay).await;
                }
            }
        }
    });

    println!(
        "桥接已启动：bot={}，并发上限 {}；Ctrl-C 退出。",
        store
            .load_account()
            .map(|a| a.account_id)
            .unwrap_or_else(|| "unknown".to_string()),
        MAX_CONCURRENT_TURNS,
    );

    let mut runtime = runtime;
    loop {
        tokio::select! {
            resp = poll_rx.recv() => match resp {
                Some(resp) => runtime.handle_weixin(resp).await?,
                None => {
                    return Err(BridgeError::Other("微信长轮询任务已退出".to_string()));
                }
            },
            event = events.recv() => match event {
                Some(codex_gui_bridge::GuiEvent::Notification(notification)) => {
                    runtime.handle_notification(notification).await;
                }
                Some(codex_gui_bridge::GuiEvent::ServerRequest(request)) => {
                    runtime.handle_server_request(request).await;
                }
                Some(codex_gui_bridge::GuiEvent::Undecodable(line)) => {
                    tracing::warn!(line, "无法解析的 app-server 帧");
                }
                Some(codex_gui_bridge::GuiEvent::Disconnected(status)) => {
                    return Err(BridgeError::Other(format!("codex app-server 已退出: {status}")));
                }
                Some(_) => {}
                None => {
                    return Err(BridgeError::Other("codex app-server 事件流已关闭".to_string()));
                }
            },
        }
    }
}
