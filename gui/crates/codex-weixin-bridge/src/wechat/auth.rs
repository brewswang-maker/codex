//! QR-code login state machine and on-disk account state.
//!
//! Mirrors openclaw-weixin's `auth/login-qr.ts` + `auth/accounts.ts`: fetch a
//! QR code, long-poll its status (wait / scaned / verify-code / expired /
//! redirect / confirmed), then persist `{token, base_url, user_id}` in a
//! 0600 file under `~/.codex/weixin-bridge/credentials/`.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{BridgeError, Result};
use crate::wechat::client::WeixinClient;
use crate::wechat::types::{DEFAULT_BASE_URL, DEFAULT_ILINK_BOT_TYPE, QrStatus};

/// Maximum QR refreshes after `expired` before giving up (mirrors JS).
const MAX_QR_REFRESH_COUNT: u32 = 3;
/// Delay between QR status polls.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Persisted bot credentials (single-account bridge).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AccountData {
    /// Raw `ilink_bot_id` from login (e.g. `hex@im.bot`).
    pub account_id: String,
    pub token: String,
    /// Polling base URL from login; falls back to [`DEFAULT_BASE_URL`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Scanner's user id; candidate first allowlist entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Unix milliseconds of the last save.
    pub saved_at_ms: i64,
}

/// Filesystem layout under `~/.codex/weixin-bridge/`.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Store rooted at the given codex home (`~/.codex` by default upstream).
    pub fn new(codex_home: &Path) -> Self {
        Self {
            root: codex_home.join("weixin-bridge"),
        }
    }

    /// Default store rooted at `$CODEX_HOME` (or `~/.codex`).
    pub fn with_default_home() -> Result<Self> {
        let home = match std::env::var_os("CODEX_HOME") {
            Some(path) => PathBuf::from(path),
            None => home::home_dir()
                .ok_or_else(|| BridgeError::Other("无法定位 HOME 目录".to_string()))?
                .join(".codex"),
        };
        Ok(Self::new(&home))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn credentials_dir(&self) -> PathBuf {
        self.root.join("credentials")
    }

    fn account_path(&self) -> PathBuf {
        self.credentials_dir().join("account.json")
    }

    fn sync_buf_path(&self) -> PathBuf {
        self.credentials_dir().join("account.sync.json")
    }

    pub fn load_account(&self) -> Option<AccountData> {
        let text = std::fs::read_to_string(self.account_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Persist credentials with restrictive permissions (best-effort chmod on
    /// non-unix platforms).
    pub fn save_account(&self, account: &AccountData) -> Result<()> {
        let dir = self.credentials_dir();
        std::fs::create_dir_all(&dir).map_err(|source| BridgeError::StateDir {
            path: dir.clone(),
            source,
        })?;
        let payload = serde_json::to_string_pretty(account)?;
        std::fs::write(self.account_path(), payload)?;
        set_private_permissions(&dir, 0o700);
        set_private_permissions(&self.account_path(), 0o600);
        Ok(())
    }

    pub fn clear_account(&self) {
        let _ = std::fs::remove_file(self.account_path());
        let _ = std::fs::remove_file(self.sync_buf_path());
    }

    /// Load the long-poll cursor; missing file means "start fresh".
    pub fn load_sync_buf(&self) -> String {
        let text = std::fs::read_to_string(self.sync_buf_path()).unwrap_or_default();
        serde_json::from_str::<SyncBufFile>(&text)
            .map(|file| file.get_updates_buf)
            .unwrap_or_default()
    }

    pub fn save_sync_buf(&self, get_updates_buf: &str) -> Result<()> {
        let dir = self.credentials_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|source| BridgeError::StateDir { path: dir, source })?;
        let payload = serde_json::to_string(&SyncBufFile {
            get_updates_buf: get_updates_buf.to_string(),
        })?;
        std::fs::write(self.sync_buf_path(), payload)?;
        Ok(())
    }

    /// Client bound to the stored credentials; errors when not logged in.
    pub fn load_client(&self) -> Result<WeixinClient> {
        let account = self.load_account().ok_or(BridgeError::NotLoggedIn)?;
        WeixinClient::new(
            account.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL),
            Some(account.token),
        )
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SyncBufFile {
    get_updates_buf: String,
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path, _mode: u32) {}

/// Normalize a raw `ilink_bot_id` into a filesystem-safe key. Mirrors the
/// JS channel's `normalizeAccountId`: `@` and other punctuation collapse to
/// `-`, so `hex@im.bot` becomes `hex-im-bot`.
pub fn normalize_account_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => out.push(ch),
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            _ => out.push('-'),
        }
    }
    if out.is_empty() || out.chars().all(|c| c == '-') {
        out.clear();
        out.push_str("unknown-bot");
    }
    out
}

/// Progress events emitted while the login flow runs.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginEvent {
    QrReady { url: String },
    QrRefreshed { count: u32, url: String },
    Scanned,
    NeedVerifyCode,
    Waiting,
    Message(String),
}

/// Terminal state of a login attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginOutcome {
    Connected(Box<AccountData>),
    AlreadyConnected,
    Failed(String),
}

/// Run the interactive QR login until confirmed, expired out, or timed out.
pub async fn run_login(
    client: &WeixinClient,
    timeout: Duration,
    mut on_event: impl FnMut(LoginEvent),
    mut read_verify_code: impl AsyncFnMut() -> String,
) -> LoginOutcome {
    let local_tokens: Vec<String> = Vec::new();
    let mut qr = match client
        .fetch_qr_code(DEFAULT_ILINK_BOT_TYPE, local_tokens)
        .await
    {
        Ok(qr) => qr,
        Err(err) => return LoginOutcome::Failed(format!("获取二维码失败: {err}")),
    };
    on_event(LoginEvent::QrReady {
        url: qr.qrcode_img_content.clone(),
    });

    let deadline = tokio::time::Instant::now() + timeout;
    let mut polling_base = client.base_url().to_string();
    let mut pending_verify_code: Option<String> = None;
    let mut scanned_announced = false;
    let mut refresh_count: u32 = 1;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return LoginOutcome::Failed("登录超时，请重试。".to_string());
        }

        let status = match client
            .poll_qr_status(&polling_base, &qr.qrcode, pending_verify_code.as_deref())
            .await
        {
            Ok(status) => status,
            Err(err) => return LoginOutcome::Failed(format!("查询二维码状态失败: {err}")),
        };

        match status.status.unwrap_or(QrStatus::Waiting) {
            QrStatus::Waiting => on_event(LoginEvent::Waiting),
            QrStatus::Scaned => {
                if pending_verify_code.take().is_some() {
                    on_event(LoginEvent::Message(
                        "验证码已接受，继续等待确认…".to_string(),
                    ));
                }
                if !scanned_announced {
                    scanned_announced = true;
                    on_event(LoginEvent::Scanned);
                }
            }
            QrStatus::NeedVerifycode => {
                on_event(if pending_verify_code.is_some() {
                    LoginEvent::Message("输入的数字不匹配，请重新输入：".to_string())
                } else {
                    LoginEvent::NeedVerifyCode
                });
                pending_verify_code = Some(read_verify_code().await);
                continue;
            }
            QrStatus::Expired | QrStatus::VerifyCodeBlocked => {
                pending_verify_code = None;
                scanned_announced = false;
                refresh_count += 1;
                if refresh_count > MAX_QR_REFRESH_COUNT {
                    return LoginOutcome::Failed(
                        "二维码多次失效，连接流程已停止。请稍后再试。".to_string(),
                    );
                }
                on_event(LoginEvent::Message(format!(
                    "二维码已失效，正在刷新…({refresh_count}/{MAX_QR_REFRESH_COUNT})"
                )));
                match client
                    .fetch_qr_code(DEFAULT_ILINK_BOT_TYPE, Vec::new())
                    .await
                {
                    Ok(new_qr) => {
                        qr = new_qr;
                        on_event(LoginEvent::QrRefreshed {
                            count: refresh_count,
                            url: qr.qrcode_img_content.clone(),
                        });
                    }
                    Err(err) => return LoginOutcome::Failed(format!("刷新二维码失败: {err}")),
                }
            }
            QrStatus::ScanedButRedirect => {
                if let Some(host) = status.redirect_host.filter(|h| !h.is_empty()) {
                    polling_base = format!("https://{host}");
                    on_event(LoginEvent::Message(format!("切换到就近接入点: {host}")));
                }
            }
            QrStatus::BindedRedirect => {
                return LoginOutcome::AlreadyConnected;
            }
            QrStatus::Confirmed => {
                let Some(token) = status.bot_token.filter(|t| !t.trim().is_empty()) else {
                    return LoginOutcome::Failed("登录失败：服务器未返回 bot_token。".to_string());
                };
                let Some(bot_id) = status.ilink_bot_id.filter(|id| !id.trim().is_empty()) else {
                    return LoginOutcome::Failed(
                        "登录失败：服务器未返回 ilink_bot_id。".to_string(),
                    );
                };
                let account = AccountData {
                    account_id: bot_id,
                    token,
                    base_url: status.baseurl.filter(|url| !url.trim().is_empty()),
                    user_id: status.ilink_user_id.filter(|id| !id.trim().is_empty()),
                    saved_at_ms: unix_now_ms(),
                };
                return LoginOutcome::Connected(Box::new(account));
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Render a URL as a terminal QR code plus a plain-text fallback link.
pub fn render_qr_to_terminal(url: &str) {
    let mut stdout = std::io::stdout();
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            // Light-on-dark half blocks: correct for dark-background terminals
            // (the common case); the URL below is the accessible fallback.
            let rendered = code
                .render::<qrcode::render::unicode::Dense1x2>()
                .dark_color(qrcode::render::unicode::Dense1x2::Light)
                .light_color(qrcode::render::unicode::Dense1x2::Dark)
                .quiet_zone(true)
                .build();
            let _ = writeln!(stdout, "{rendered}");
        }
        Err(err) => {
            let _ = writeln!(stdout, "二维码渲染失败: {err}");
        }
    }
    let _ = writeln!(
        stdout,
        "若二维码未能显示或无法使用，可以访问以下链接以继续：\n{url}"
    );
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
