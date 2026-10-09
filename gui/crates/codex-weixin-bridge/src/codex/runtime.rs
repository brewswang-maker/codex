//! Bridge orchestration: the WeChat long poll, command dispatch, the
//! per-contact serial session queue, and the codex event pump.
//!
//! Layout: a dedicated task runs `get_updates` (a 35s long poll that must not
//! be cancelled by codex event traffic) and forwards responses over an mpsc
//! channel; the main loop selects between that channel and the app-server
//! event stream, mutating runtime state only between awaits.

use codex_app_server_protocol::{ServerNotification, ServerRequest, ThreadItem};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::codex::allowlist::AllowList;
use crate::codex::approval::{AnswerOutcome, ApprovalBroker};
use crate::codex::bridge_state;
use crate::codex::control;
use crate::codex::progress::{
    clean_for_wechat, describe_duration, final_reply, split_weixin_actions, summarize_item,
};
use crate::codex::session::{CodexBridge, SessionStore};
use crate::error::{BridgeError, Result};
use crate::wechat::auth::Store;
use crate::wechat::client::WeixinClient;
use crate::wechat::media_transfer as media;
use crate::wechat::types::{self, GetUpdatesResp, WeixinMessage, generate_client_id};

/// Global cap on codex turns running at once (plan: guard against runaway).
pub const MAX_CONCURRENT_TURNS: usize = 2;
/// WeChat long poll retries before the backoff grows.
const POLL_FAILURE_BACKOFF_THRESHOLD: u32 = 3;
/// How often expired approvals are swept.
const APPROVAL_SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// In-memory session state for one contact; `thread_id` mirrors the
/// persisted mapping while `turn_id` tracks the live turn.
#[derive(Debug, Default)]
struct ContactSession {
    thread_id: Option<String>,
    turn_id: Option<String>,
    queue: VecDeque<String>,
    /// Most recent inbound `context_token`; echoed on replies.
    context_token: Option<String>,
    /// Model chosen through `/model <n>`; applies to the next new thread.
    model_override: Option<String>,
}

pub struct Runtime {
    wx: WeixinClient,
    sessions: SessionStore,
    allowlist: AllowList,
    broker: ApprovalBroker,
    codex: CodexBridge,
    /// Shared `~/.codex` root; the GUI panel reads the runtime files here.
    codex_home: PathBuf,
    live: HashMap<String, ContactSession>,
    by_thread: HashMap<String, String>,
    /// `/model` picker cache per contact (1-based order preserved).
    model_cache: HashMap<String, Vec<String>>,
    /// `/resume` picker cache per contact (id list in listed order).
    thread_cache: HashMap<String, Vec<String>>,
    /// Contacts with queued tasks, in submission order, so a global slot can
    /// start work for any contact rather than only the contact that finished.
    task_order: VecDeque<String>,
    /// Canonical root for model-requested outbound media.
    workspace_root: PathBuf,
    active_turns: usize,
}

impl Runtime {
    fn new(
        wx: WeixinClient,
        sessions: SessionStore,
        allowlist: AllowList,
        codex: CodexBridge,
        codex_home: PathBuf,
    ) -> Self {
        Self {
            wx,
            sessions,
            allowlist,
            broker: ApprovalBroker::new(),
            codex,
            codex_home,
            live: HashMap::new(),
            by_thread: HashMap::new(),
            model_cache: HashMap::new(),
            thread_cache: HashMap::new(),
            task_order: VecDeque::new(),
            workspace_root: canonical_workspace_root(),
            active_turns: 0,
        }
    }

    async fn reply(&self, contact: &str, text: &str) {
        let context_token = self.live.get(contact).and_then(|s| s.context_token.clone());
        let msg = WeixinMessage::text_reply(
            contact,
            text,
            context_token.as_deref(),
            &generate_client_id(),
        );
        if let Err(err) = self.wx.send_prepared(msg).await {
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
        // group chats are out of scope for now, media arrives later.
        if msg.message_type != Some(types::message_type::USER) {
            return;
        }
        if msg.group_id.as_ref().is_some_and(|g| !g.is_empty()) {
            return;
        }
        let Some(contact) = msg.from_user_id.clone().filter(|c| !c.is_empty()) else {
            return;
        };
        if !self.allowlist.is_allowed(&contact) {
            // Unauthorized contacts get the fixed notice, are surfaced in
            // the GUI panel's pending list, and never reach a codex session
            // or the approval flow.
            bridge_state::record_pending(&self.codex_home, &contact);
            self.reply(&contact, &control::unauthorized_notice()).await;
            return;
        }
        let text = msg.text();
        {
            let session = self.live.entry(contact.clone()).or_default();
            session.context_token = msg.context_token.clone();
        }
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            if let Some(reply) = self.handle_command(&contact, &trimmed).await {
                self.reply(&contact, &reply).await;
                return;
            }
            // `y`/`n`/`y <反馈>` answer a pending approval when one exists;
            // any other text falls through to the task queue.
            match self.broker.answer(&contact, &trimmed) {
                AnswerOutcome::NoPending => {}
                AnswerOutcome::Answered(answered) => {
                    crate::codex::approval::respond(&self.codex.client().clone(), &answered).await;
                    self.reply(&contact, &answered.reply).await;
                    if let Some(follow_up) = answered.follow_up {
                        self.enqueue_task(&contact, follow_up).await;
                    }
                    return;
                }
            }
        }
        // Media items (image/voice/file/video) are downloaded and decrypted
        // to `inbound/<contact>/`; their local paths ride in the task prompt.
        let attachments =
            media::fetch_inbound_attachments(&self.wx, &self.codex_home, &contact, &msg).await;
        match media::build_attachment_prompt(&trimmed, &attachments) {
            Some(task) => self.enqueue_task(&contact, task).await,
            None => {
                self.reply(
                    &contact,
                    "暂不支持空消息；请发送文本任务，或附带图片 / 语音 / 文件 / 视频。",
                )
                .await;
            }
        }
    }

    /// WeChat command dispatch; plain text is a task. `None` means the text
    /// is not a command.
    async fn handle_command(&mut self, contact: &str, text: &str) -> Option<String> {
        let rest = text.strip_prefix('/')?;
        let (cmd, arg) = rest.split_once([' ', '\n']).unwrap_or((rest, ""));
        match cmd {
            "new" => {
                if self
                    .live
                    .get(contact)
                    .is_some_and(|session| session.turn_id.is_some())
                {
                    return Some("当前任务运行中；请先发送 /stop，完成后再 /new。".to_string());
                }
                let previous_thread = self
                    .live
                    .get(contact)
                    .and_then(|session| session.thread_id.clone())
                    .or_else(|| self.sessions.thread_for(contact).map(str::to_string));
                let had = self.sessions.unbind(contact).unwrap_or(false);
                if let Some(session) = self.live.remove(contact) {
                    self.live.insert(
                        contact.to_string(),
                        ContactSession {
                            model_override: session.model_override,
                            ..ContactSession::default()
                        },
                    );
                }
                if let Some(thread_id) = previous_thread.as_deref() {
                    self.by_thread.remove(thread_id);
                }
                self.task_order.retain(|queued| queued != contact);
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
            "help" => Some(control::help_text()),
            "model" => {
                let pick = control::parse_model_pick(arg);
                match pick {
                    Some(pick) => {
                        let chosen = self
                            .model_cache
                            .get(contact)
                            .and_then(|models| models.get(pick - 1).cloned());
                        match chosen {
                            Some(model) => {
                                if let Some(session) = self.live.get_mut(contact) {
                                    session.model_override = Some(model.clone());
                                }
                                Some(format!("已选择模型 {model}；/new 后的新会话将使用它。"))
                            }
                            None => Some(format!(
                                "序号 {pick} 不在刚才的列表里；先发送 /model 获取最新列表。"
                            )),
                        }
                    }
                    None => match control::model_list(&self.codex).await {
                        Ok(models) => {
                            let text = control::model_list_text(&models);
                            self.model_cache.insert(contact.to_string(), models);
                            Some(text)
                        }
                        Err(err) => Some(format!("获取模型列表失败: {err}")),
                    },
                }
            }
            "resume" => {
                let pick = control::parse_resume_pick(arg);
                match pick {
                    Some(pick) => {
                        if self
                            .live
                            .get(contact)
                            .is_some_and(|session| session.turn_id.is_some())
                        {
                            return Some(
                                "当前任务运行中；请先发送 /stop，完成后再切换线程。".to_string(),
                            );
                        }
                        let target = self
                            .thread_cache
                            .get(contact)
                            .and_then(|ids| ids.get(pick - 1).cloned());
                        match target {
                            Some(thread_id) => match self.codex.resume_thread(&thread_id).await {
                                Ok(()) => {
                                    let _ = self.sessions.unbind(contact);
                                    if let Some(session) = self.live.get_mut(contact) {
                                        session.turn_id = None;
                                        session.thread_id = Some(thread_id.clone());
                                    }
                                    if let Err(err) = self.sessions.bind(contact, &thread_id) {
                                        return Some(format!("恢复线程失败: {err}"));
                                    }
                                    self.by_thread
                                        .insert(thread_id.clone(), contact.to_string());
                                    Some(format!("已切回线程 {thread_id}，下一条消息继续它。"))
                                }
                                Err(err) => Some(format!("恢复线程失败: {err}")),
                            },
                            None => Some(format!(
                                "序号 {pick} 不在刚才的列表里；先发送 /resume 获取线程列表。"
                            )),
                        }
                    }
                    None => {
                        match control::thread_list(&self.codex, control::RESUME_LIST_LIMIT).await {
                            Ok(threads) => {
                                let ids: Vec<String> =
                                    threads.iter().map(|(id, _)| id.clone()).collect();
                                let text = control::thread_list_text(&threads);
                                self.thread_cache.insert(contact.to_string(), ids);
                                Some(text)
                            }
                            Err(err) => Some(format!("获取线程列表失败: {err}")),
                        }
                    }
                }
            }
            "bindquest" => {
                let name = arg.trim();
                if name.is_empty() {
                    return Some("用法：/bindquest 组名 — 把当前线程归入该 Quest 组。".to_string());
                }
                let thread_id = self
                    .live
                    .get(contact)
                    .and_then(|s| s.thread_id.clone())
                    .or_else(|| self.sessions.thread_for(contact).map(str::to_string));
                let Some(thread_id) = thread_id else {
                    return Some("当前没有会话；先发一条任务再绑定 Quest。".to_string());
                };
                match control::bind_thread_to_quest(&self.codex, &thread_id, name).await {
                    Ok(reply) => Some(reply),
                    Err(err) => Some(format!("绑定 Quest 失败: {err}")),
                }
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
        let busy = self.live.get(contact).is_some_and(|s| s.turn_id.is_some())
            || self.active_turns >= MAX_CONCURRENT_TURNS;
        if busy {
            if let Some(session) = self.live.get_mut(contact) {
                session.queue.push_back(text);
            }
            if !self.task_order.iter().any(|queued| queued == contact) {
                self.task_order.push_back(contact.to_string());
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
        let model_override = self
            .live
            .get(contact)
            .and_then(|session| session.model_override.clone());
        let thread_id = match persisted {
            Some(thread_id) => match self.codex.resume_thread(&thread_id).await {
                Ok(()) => thread_id,
                Err(err) => {
                    tracing::warn!(contact, thread = %thread_id, %err, "resume 失败，改用新线程");
                    self.codex.start_thread(None, model_override).await?
                }
            },
            None => self.codex.start_thread(None, model_override).await?,
        };
        self.sessions.bind(contact, &thread_id)?;
        self.by_thread
            .insert(thread_id.clone(), contact.to_string());
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
            && let Some(next) = self.take_next_queued_contact()
            && let Some(next_task) = self
                .live
                .get_mut(&next)
                .and_then(|session| session.queue.pop_front())
        {
            self.start_turn(&next, next_task).await;
        }
    }

    fn take_next_queued_contact(&mut self) -> Option<String> {
        while let Some(contact) = self.task_order.pop_front() {
            if self
                .live
                .get(&contact)
                .is_some_and(|session| !session.queue.is_empty())
            {
                return Some(contact);
            }
        }
        None
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
                let raw = final_reply(&done.turn)
                    .unwrap_or_else(|| "任务完成，但没有回复文本。".to_string());
                // ```codex-weixin-actions``` blocks become native media
                // deliveries; the remaining text is cleaned and sent as-is.
                let (body_text, actions) = split_weixin_actions(&raw);
                let context_token = self
                    .live
                    .get(&contact)
                    .and_then(|s| s.context_token.clone());
                let workspace_root = self.workspace_root.clone();
                for raw_path in &actions {
                    let Some(path) = allowed_action_path(&workspace_root, raw_path) else {
                        tracing::warn!(contact, path = %raw_path, "拒绝工作区外的媒体回传");
                        self.reply(
                            &contact,
                            &format!("媒体发送失败（{raw_path}）: 路径不在工作区内"),
                        )
                        .await;
                        continue;
                    };
                    let path = &path;
                    if let Err(err) = media::upload_and_send(
                        &self.wx,
                        &contact,
                        context_token.as_deref(),
                        Path::new(path),
                    )
                    .await
                    {
                        tracing::warn!(contact, path = %path.display(), "媒体回传失败: {err}");
                        self.reply(
                            &contact,
                            &format!("媒体发送失败（{}）: {err}", path.display()),
                        )
                        .await;
                    }
                }
                let reply = if body_text.is_empty() {
                    // Media-only replies need no companion text.
                    String::new()
                } else {
                    let body = clean_for_wechat(&body_text);
                    if duration.is_empty() {
                        body
                    } else {
                        format!("{body}\n\n— {duration}")
                    }
                };
                if !reply.is_empty() {
                    self.reply(&contact, &reply).await;
                }
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
            // The server answered an approval itself (e.g. turn cancelled);
            // drop our pending entry so a later "y" does not hit a ghost.
            ServerNotification::ServerRequestResolved(resolved) => {
                self.broker.resolved(&resolved.request_id);
            }
            _ => {}
        }
    }

    /// Forwards server approval requests to the contact's WeChat chat; the
    /// answer (or the 60s timeout) resolves them through the broker.
    async fn handle_server_request(&mut self, request: ServerRequest) {
        // Only threads the bridge owns map to a contact; anything else gets
        // an error reply so the app-server never hangs on the bridge.
        let thread_id = request_thread_id(&request);
        let contact = thread_id
            .as_deref()
            .and_then(|id| self.by_thread.get(id).cloned());
        match contact {
            Some(contact) => {
                if let Some(prompt) = self.broker.offered(&request, &contact) {
                    self.reply(&contact, &prompt).await;
                } else {
                    let _ = self
                        .codex
                        .client()
                        .respond_error_for_id(
                            request.id().clone(),
                            -32601,
                            "bridge 不支持该审批类型",
                        )
                        .await;
                }
            }
            None => {
                tracing::warn!(?thread_id, "审批请求来自未知线程，回复错误");
                let _ = self
                    .codex
                    .client()
                    .respond_error_for_id(request.id().clone(), -32601, "bridge 不支持该请求")
                    .await;
            }
        }
    }

    /// Sweeps expired approvals, answering them with a decline and telling
    /// the contact what happened; also re-reads the allowlist so grants made
    /// in the GUI panel take effect within one sweep, clearing their pending
    /// entries from the panel.
    async fn expire_approvals(&mut self) {
        for (request_id, payload, contact, notice) in self.broker.expired() {
            let _ = self
                .codex
                .client()
                .respond_for_id(request_id, payload)
                .await;
            self.reply(&contact, &notice).await;
        }
        self.allowlist.reload();
        let home = self.codex_home.clone();
        for entry in bridge_state::read_pending(&home) {
            if self.allowlist.is_allowed(&entry.contact) {
                bridge_state::drop_pending(&home, &entry.contact);
            }
        }
    }
}

/// The thread a server-initiated request belongs to, for contact routing.
fn request_thread_id(request: &ServerRequest) -> Option<String> {
    match request {
        ServerRequest::CommandExecutionRequestApproval { params, .. } => {
            Some(params.thread_id.clone())
        }
        ServerRequest::FileChangeRequestApproval { params, .. } => Some(params.thread_id.clone()),
        ServerRequest::PermissionsRequestApproval { params, .. } => Some(params.thread_id.clone()),
        _ => None,
    }
}

fn canonical_workspace_root() -> PathBuf {
    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    std::fs::canonicalize(&current).unwrap_or(current)
}

fn allowed_action_path(workspace_root: &Path, raw_path: &str) -> Option<PathBuf> {
    let requested = Path::new(raw_path);
    let candidate = if requested.is_absolute() {
        requested
    } else {
        &workspace_root.join(requested)
    };
    let resolved = candidate.canonicalize().ok()?;
    resolved.starts_with(workspace_root).then_some(resolved)
}

/// Runs the bridge until the WeChat token expires or the app-server exits.
/// `app_server` overrides the backend binary the daemon spawns (`None`
/// resolves `codex` from `PATH`).
pub async fn run(store: Store, app_server: Option<PathBuf>) -> Result<()> {
    let account = store.load_account();
    let wx = store.load_client()?;
    // The live channel requires the client to announce itself before any
    // conversation counts as prepared; skipping this makes every
    // sendmessage fail with ret=-2 "prepare failed".
    let mut announce_error: Option<String> = None;
    for attempt in 1..=3 {
        match wx.notify_start().await {
            Ok(resp) if resp.ret.is_none() || resp.ret == Some(0) => {
                announce_error = None;
                break;
            }
            Ok(resp) => {
                announce_error = Some(format!(
                    "ret={:?} {}",
                    resp.ret,
                    resp.errmsg.clone().unwrap_or_default()
                ));
            }
            Err(err) => announce_error = Some(err.to_string()),
        }
        tracing::warn!(attempt, "msg/notifystart 失败，2s 后重试");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    if let Some(err) = announce_error {
        return Err(BridgeError::Other(format!("msg/notifystart 失败: {err}")));
    }
    let (codex, mut events) = CodexBridge::connect(app_server).await?;
    let sessions = SessionStore::load(store.codex_home());
    // First allowlist entry is the QR scanner (candidate per the plan).
    let allowlist = AllowList::load(
        store.codex_home(),
        account.as_ref().and_then(|a| a.user_id.as_deref()),
    );
    let runtime = Runtime::new(
        wx,
        sessions,
        allowlist,
        codex,
        store.codex_home().to_path_buf(),
    );

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

    // Publish the liveness mark the GUI panel reads; removed on any exit.
    let bot_id = store
        .load_account()
        .map(|a| a.account_id)
        .unwrap_or_else(|| "unknown".to_string());
    if let Err(err) = bridge_state::write_running(store.codex_home(), &bot_id) {
        tracing::warn!(%err, "bridge-state.json 写入失败，GUI 面板将显示未运行");
    }

    let mut runtime = runtime;
    let mut approval_sweep = tokio::time::interval(APPROVAL_SWEEP_INTERVAL);
    approval_sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let outcome: Result<()> = async {
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
                        return Err(BridgeError::Other(format!(
                            "codex app-server 已退出: {status}"
                        )));
                    }
                    Some(_) => {}
                    None => {
                        return Err(BridgeError::Other("codex app-server 事件流已关闭".to_string()));
                    }
                },
                _ = approval_sweep.tick() => {
                    runtime.expire_approvals().await;
                }
            }
        }
    }
    .await;
    bridge_state::clear_running(store.codex_home());
    outcome
}

#[cfg(test)]
mod tests {
    use super::allowed_action_path;
    use std::error::Error;
    use std::fs;

    #[test]
    fn action_paths_are_confined_to_the_workspace() -> Result<(), Box<dyn Error>> {
        let workspace = tempfile::tempdir()?;
        let root = workspace.path().canonicalize()?;
        fs::write(root.join("output.png"), b"media")?;

        let inside =
            allowed_action_path(&root, "output.png").ok_or("confined path should resolve")?;
        assert!(inside.starts_with(&root));
        let absolute_path = root.join("output.png");
        let absolute = absolute_path
            .to_str()
            .ok_or("workspace path should be UTF-8")?;
        assert!(allowed_action_path(&root, absolute).is_some());
        assert!(allowed_action_path(&root, "../outside.png").is_none());
        assert!(allowed_action_path(&root, "/etc/hostname").is_none());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn action_path_symlink_escape_is_rejected() -> Result<(), Box<dyn Error>> {
        let workspace = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = workspace.path().canonicalize()?;
        let target = outside.path().join("secret.png");
        fs::write(&target, b"media")?;
        std::os::unix::fs::symlink(&target, root.join("escape.png"))?;

        assert!(allowed_action_path(&root, "escape.png").is_none());
        Ok(())
    }
}
