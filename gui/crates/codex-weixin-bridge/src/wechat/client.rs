//! HTTP client for the WeChat iLink bot API.
//!
//! Mirrors openclaw-weixin's `api/api.ts`: JSON over HTTPS, `base_info` on
//! every POST, random `X-WECHAT-UIN`, `AuthorizationType: ilink_bot_token`,
//! and per-request timeouts including 35s long-polls.

use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;

use crate::error::{BridgeError, Result};
use crate::wechat::types::{
    BaseInfo, GetConfigResp, GetUpdatesResp, NotifyStartResp, NotifyStopResp, QrCodeResponse,
    QrStatus, QrStatusResponse, SendMessageResp, WeixinMessage,
};

const ILINK_APP_ID: &str = "bot";
const DEFAULT_LONG_POLL_TIMEOUT: Duration = Duration::from_secs(35);
const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_CONFIG_TIMEOUT: Duration = Duration::from_secs(10);

/// Encode a semver string as the wire's uint32 `0x00MMNNPP` layout.
pub fn client_version_u32(version: &str) -> u32 {
    let mut parts = version
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0) & 0xff);
    let mut next = || parts.next().unwrap_or(0);
    let (major, minor, patch) = (next(), next(), next());
    (major << 16) | (minor << 8) | patch
}

/// Random `X-WECHAT-UIN` header: random uint32 -> decimal string -> base64.
fn random_wechat_uin() -> String {
    let value: u32 = rand::random();
    base64::engine::general_purpose::STANDARD.encode(value.to_string())
}

/// Typed HTTP client bound to one bot account (token optional pre-login).
#[derive(Debug)]
pub struct WeixinClient {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
    channel_version: String,
    bot_agent: String,
}

#[derive(Serialize)]
struct GetUpdatesBody<'a> {
    get_updates_buf: &'a str,
    base_info: BaseInfo,
}

#[derive(Serialize)]
struct SendMessageBody {
    msg: WeixinMessage,
    base_info: BaseInfo,
}

#[derive(Serialize)]
struct GetConfigBody<'a> {
    ilink_user_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_token: Option<&'a str>,
    base_info: BaseInfo,
}

#[derive(Serialize)]
struct NotifyBody {
    base_info: BaseInfo,
}

#[derive(Serialize)]
struct SendTypingBody<'a> {
    ilink_user_id: &'a str,
    typing_ticket: &'a str,
    status: i64,
    base_info: BaseInfo,
}

#[derive(Serialize)]
struct QrRequestBody {
    local_token_list: Vec<String>,
}

impl WeixinClient {
    pub fn new(base_url: &str, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder().build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            channel_version: env!("CARGO_PKG_VERSION").to_string(),
            bot_agent: format!("codex-weixin-bridge/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    pub fn base_info(&self) -> BaseInfo {
        BaseInfo {
            channel_version: Some(self.channel_version.clone()),
            bot_agent: Some(self.bot_agent.clone()),
        }
    }

    fn common_headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("ilink-app-id"),
            HeaderValue::from_static(ILINK_APP_ID),
        );
        if let Ok(version) =
            HeaderValue::from_str(&client_version_u32(&self.channel_version).to_string())
        {
            headers.insert(HeaderName::from_static("ilink-app-clientversion"), version);
        }
        headers
    }

    fn post_headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderName, HeaderValue};
        let mut headers = self.common_headers();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("authorizationtype"),
            HeaderValue::from_static("ilink_bot_token"),
        );
        if let Ok(uin) = HeaderValue::from_str(&random_wechat_uin()) {
            headers.insert(HeaderName::from_static("x-wechat-uin"), uin);
        }
        if let Some(token) = self
            .token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .and_then(|token| HeaderValue::from_str(&format!("Bearer {token}")).ok())
        {
            headers.insert(reqwest::header::AUTHORIZATION, token);
        }
        headers
    }

    fn url(base_url: &str, endpoint: &str) -> String {
        format!("{}/{}", base_url.trim_end_matches('/'), endpoint)
    }

    async fn post_raw(
        &self,
        base_url: &str,
        endpoint: &str,
        body: impl Serialize,
        timeout: Duration,
    ) -> Result<String> {
        let payload = serde_json::to_string(&body)?;
        let response = self
            .http
            .post(Self::url(base_url, endpoint))
            .headers(self.post_headers())
            .timeout(timeout)
            .body(payload)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.text().await?)
    }

    async fn get_raw(
        &self,
        base_url: &str,
        endpoint: &str,
        query: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<String> {
        let mut request = self
            .http
            .get(Self::url(base_url, endpoint))
            .headers(self.common_headers())
            .timeout(timeout);
        if !query.is_empty() {
            request = request.query(query);
        }
        let response = request.send().await?.error_for_status()?;
        Ok(response.text().await?)
    }

    /// Long-poll inbound messages. A client-side timeout yields an empty
    /// success response carrying the previous cursor, so callers simply retry.
    pub async fn get_updates(&self, cursor: &str) -> Result<GetUpdatesResp> {
        let body = GetUpdatesBody {
            get_updates_buf: cursor,
            base_info: self.base_info(),
        };
        match self
            .post_raw(
                &self.base_url,
                "ilink/bot/getupdates",
                body,
                DEFAULT_LONG_POLL_TIMEOUT,
            )
            .await
        {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(BridgeError::Http(err)) if err.is_timeout() => Ok(GetUpdatesResp::empty(cursor)),
            Err(other) => Err(other),
        }
    }

    /// Send one message and return the server-assigned id.
    pub async fn send_message(&self, msg: WeixinMessage) -> Result<SendMessageResp> {
        let body = SendMessageBody {
            msg,
            base_info: self.base_info(),
        };
        let text = self
            .post_raw(
                &self.base_url,
                "ilink/bot/sendmessage",
                body,
                DEFAULT_API_TIMEOUT,
            )
            .await?;
        let resp: SendMessageResp = serde_json::from_str(&text)?;
        if resp.ret.is_some_and(|ret| ret != 0) {
            return Err(BridgeError::Api {
                ret: resp.ret.unwrap_or_default(),
                errcode: None,
                errmsg: resp.errmsg,
            });
        }
        Ok(resp)
    }

    /// Fetch bot config (contains the typing ticket) for one conversation.
    pub async fn get_config(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResp> {
        let body = GetConfigBody {
            ilink_user_id,
            context_token,
            base_info: self.base_info(),
        };
        let text = self
            .post_raw(
                &self.base_url,
                "ilink/bot/getconfig",
                body,
                DEFAULT_CONFIG_TIMEOUT,
            )
            .await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Show or cancel the typing indicator in one conversation.
    pub async fn send_typing(
        &self,
        ilink_user_id: &str,
        typing_ticket: &str,
        status: i64,
    ) -> Result<()> {
        let body = SendTypingBody {
            ilink_user_id,
            typing_ticket,
            status,
            base_info: self.base_info(),
        };
        self.post_raw(
            &self.base_url,
            "ilink/bot/sendtyping",
            body,
            DEFAULT_CONFIG_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    /// Announce this channel client to the server on startup.
    pub async fn notify_start(&self) -> Result<NotifyStartResp> {
        let body = NotifyBody {
            base_info: self.base_info(),
        };
        let text = self
            .post_raw(
                &self.base_url,
                "ilink/bot/msg/notifystart",
                body,
                DEFAULT_CONFIG_TIMEOUT,
            )
            .await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Announce shutdown so the server stops queueing messages for us.
    pub async fn notify_stop(&self) -> Result<NotifyStopResp> {
        let body = NotifyBody {
            base_info: self.base_info(),
        };
        let text = self
            .post_raw(
                &self.base_url,
                "ilink/bot/msg/notifystop",
                body,
                DEFAULT_CONFIG_TIMEOUT,
            )
            .await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Request a login QR code. Pass locally stored tokens so the server can
    /// answer `binded_redirect` for already-linked bots.
    pub async fn fetch_qr_code(
        &self,
        bot_type: &str,
        local_token_list: Vec<String>,
    ) -> Result<QrCodeResponse> {
        let body = QrRequestBody { local_token_list };
        let text = self
            .post_raw(
                &self.base_url,
                &format!("ilink/bot/get_bot_qrcode?bot_type={bot_type}"),
                body,
                DEFAULT_API_TIMEOUT,
            )
            .await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Long-poll the QR status. Network errors degrade to `Waiting` (the JS
    /// channel treats gateway hiccups as retryable waits).
    pub async fn poll_qr_status(
        &self,
        base_url: &str,
        qrcode: &str,
        verify_code: Option<&str>,
    ) -> Result<QrStatusResponse> {
        let mut query: Vec<(&str, &str)> = vec![("qrcode", qrcode)];
        if let Some(code) = verify_code {
            query.push(("verify_code", code));
        }
        match self
            .get_raw(
                base_url,
                "ilink/bot/get_qrcode_status",
                &query,
                DEFAULT_LONG_POLL_TIMEOUT,
            )
            .await
        {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(BridgeError::Http(err)) if err.is_timeout() => Ok(QrStatusResponse {
                status: Some(QrStatus::Waiting),
                ..QrStatusResponse::default()
            }),
            Err(err) => {
                tracing::warn!("poll_qr_status 网络错误，按等待处理并将重试: {err}");
                Ok(QrStatusResponse {
                    status: Some(QrStatus::Waiting),
                    ..QrStatusResponse::default()
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
