//! WeChat iLink protocol wire types.
//!
//! Mirrors the TypeScript contract in openclaw-weixin's `api/types.ts`: JSON
//! over HTTPS, proto-style snake_case field names, base64 strings for bytes
//! fields, and identifiers accepted in both uint64 and versioned-string wire
//! shapes (normalized to `String`).

use serde::{Deserialize, Serialize};

/// Server IDs arrived as proto uint64 numbers historically, but the live
/// iLink channel now emits versioned strings (`"v1:<snowflake>"`) for the
/// same fields. Accept both wire shapes and normalize to the string form;
/// a non-string non-number value deserializes as absent.
fn deserialize_optional_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        Some(serde_json::Value::String(text)) => Some(text),
        _ => None,
    })
}

/// Default API host for every iLink bot request.
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
/// CDN host used for encrypted media upload/download (D3).
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
/// Default `bot_type` query parameter for QR login (`get_bot_qrcode`).
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";

/// Common request metadata attached to every CGI request (`base_info`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BaseInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
    /// Self-declared bot identity, UA-style `name/version`; observability only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_agent: Option<String>,
}

/// `message_type` discriminates user-authored from bot-authored messages.
pub mod message_type {
    pub const NONE: i64 = 0;
    pub const USER: i64 = 1;
    pub const BOT: i64 = 2;
}

/// `item.type` selects which `*_item` union member is populated.
pub mod item_type {
    pub const NONE: i64 = 0;
    pub const TEXT: i64 = 1;
    pub const IMAGE: i64 = 2;
    pub const VOICE: i64 = 3;
    pub const FILE: i64 = 4;
    pub const VIDEO: i64 = 5;
    pub const TOOL_CALL_START: i64 = 11;
    pub const TOOL_CALL_RESULT: i64 = 12;
}

/// Delivery state of a message; outbound replies are always `FINISH`.
pub mod message_state {
    pub const NEW: i64 = 0;
    pub const GENERATING: i64 = 1;
    pub const FINISH: i64 = 2;
}

/// Typing indicator status for `sendtyping`.
pub mod typing_status {
    pub const TYPING: i64 = 1;
    pub const CANCEL: i64 = 2;
}

/// proto: UploadMediaType values for `ilink/bot/getuploadurl`.
pub mod upload_media_type {
    pub const IMAGE: i64 = 1;
    pub const VIDEO: i64 = 2;
    pub const FILE: i64 = 3;
    pub const VOICE: i64 = 4;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TextItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// CDN media reference; `aes_key` is a base64 string on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CdnMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ImageItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES-128 key as hex (16 bytes); preferred for inbound decryption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hd_size: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VoiceItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// 1=pcm 2=adpcm 3=feature 4=speex 5=amr 6=silk 7=mp3 8=ogg-speex
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
    /// Voice length in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playtime: Option<i64>,
    /// Server-provided speech-to-text transcript, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FileItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Plaintext byte count; arrives as a string on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VideoItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_length: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RefMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_item: Option<Box<MessageItem>>,
    /// Quoted-content summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svr_id: Option<String>,
}

/// One content item; exactly one `*_item` member matches `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MessageItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_completed: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub msg_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<RefMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
}

/// Unified message (proto: WeixinMessage). `seq`/`message_id` accept both
/// the proto uint64 wire shape and the live channel's versioned strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WeixinMessage {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub seq: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_state: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    /// Per-conversation session token; must be echoed on replies (D2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

impl WeixinMessage {
    /// Build an outbound bot text reply (mirrors `buildTextMessageReq`).
    /// The wire requires empty `from_user_id`, `message_type: BOT`,
    /// `message_state: FINISH`, and a fresh `client_id` per message.
    pub fn text_reply(to: &str, text: &str, context_token: Option<&str>, client_id: &str) -> Self {
        Self {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_string()),
            client_id: Some(client_id.to_string()),
            message_type: Some(message_type::BOT),
            message_state: Some(message_state::FINISH),
            item_list: if text.is_empty() {
                None
            } else {
                Some(vec![MessageItem {
                    r#type: Some(item_type::TEXT),
                    text_item: Some(TextItem {
                        text: Some(text.to_string()),
                    }),
                    ..MessageItem::default()
                }])
            },
            context_token: context_token.map(str::to_string),
            ..WeixinMessage::default()
        }
    }

    /// Build an outbound bot message carrying one pre-built media item
    /// (image/video/file); mirrors the per-item sends in `send.ts`.
    pub fn media_reply(
        to: &str,
        item: MessageItem,
        context_token: Option<&str>,
        client_id: &str,
    ) -> Self {
        Self {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_string()),
            client_id: Some(client_id.to_string()),
            message_type: Some(message_type::BOT),
            message_state: Some(message_state::FINISH),
            item_list: Some(vec![item]),
            context_token: context_token.map(str::to_string),
            ..WeixinMessage::default()
        }
    }

    /// Convenience: concatenated text of every TEXT item, in order.
    pub fn text(&self) -> String {
        self.item_list
            .iter()
            .flatten()
            .filter_map(|item| {
                if item.r#type == Some(item_type::TEXT) {
                    item.text_item.as_ref().and_then(|t| t.text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Fresh `client_id` for an outbound message.
pub fn generate_client_id() -> String {
    let value: u128 = rand::random();
    format!("codex-weixin-bridge-{value:032x}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GetUpdatesResp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ret: Option<i64>,
    /// Server error code (e.g. -14 = stale token / session timeout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errcode: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msgs: Option<Vec<WeixinMessage>>,
    /// Opaque long-poll cursor; cache and echo on the next request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    /// Server-suggested timeout (ms) for the next long-poll.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longpolling_timeout_ms: Option<i64>,
}

impl GetUpdatesResp {
    /// Empty success response used when a client-side long-poll timeout fires.
    pub fn empty(cursor: &str) -> Self {
        Self {
            ret: Some(0),
            msgs: Some(Vec::new()),
            get_updates_buf: if cursor.is_empty() {
                None
            } else {
                Some(cursor.to_string())
            },
            ..Self::default()
        }
    }

    /// True when the server reported an API-level error (`ret`/`errcode` != 0).
    pub fn is_api_error(&self) -> bool {
        self.ret.is_some_and(|ret| ret != 0) || self.errcode.is_some_and(|code| code != 0)
    }
}

/// proto: GetUploadUrlResp — pre-signed CDN upload targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GetUploadUrlResp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_param: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_upload_param: Option<String>,
    /// Full upload URL straight from the server; takes precedence over
    /// `upload_param` when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_full_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SendMessageResp {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ret: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct GetConfigResp {
    #[serde(default)]
    pub ret: Option<i64>,
    #[serde(default)]
    pub errmsg: Option<String>,
    /// Base64 typing ticket for `sendtyping`.
    #[serde(default)]
    pub typing_ticket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct NotifyStopResp {
    #[serde(default)]
    pub ret: Option<i64>,
    #[serde(default)]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct NotifyStartResp {
    #[serde(default)]
    pub ret: Option<i64>,
    #[serde(default)]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct QrCodeResponse {
    /// Poll handle echoed to `get_qrcode_status`.
    #[serde(default)]
    pub qrcode: String,
    /// URL encoded in the QR image; render this as the scannable code.
    #[serde(default)]
    pub qrcode_img_content: String,
}

/// QR login poll status values (`get_qrcode_status.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum QrStatus {
    #[serde(rename = "wait")]
    Waiting,
    #[serde(rename = "scaned")]
    Scaned,
    #[serde(rename = "confirmed")]
    Confirmed,
    #[serde(rename = "expired")]
    Expired,
    #[serde(rename = "scaned_but_redirect")]
    ScanedButRedirect,
    #[serde(rename = "need_verifycode")]
    NeedVerifycode,
    #[serde(rename = "verify_code_blocked")]
    VerifyCodeBlocked,
    #[serde(rename = "binded_redirect")]
    BindedRedirect,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct QrStatusResponse {
    #[serde(default)]
    pub status: Option<QrStatus>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub ilink_bot_id: Option<String>,
    #[serde(default)]
    pub baseurl: Option<String>,
    /// User ID of the person who scanned; candidate first allowlist entry.
    #[serde(default)]
    pub ilink_user_id: Option<String>,
    /// New polling host when status is `scaned_but_redirect` (IDC redirect).
    #[serde(default)]
    pub redirect_host: Option<String>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
