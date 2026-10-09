//! Media composition layer on top of [`super::media`] primitives: the
//! upload pipeline (`cdn/upload.ts`), outbound media message items
//! (`messaging/send.ts`, `messaging/send-media.ts`) and inbound persistence
//! (`media/media-download.ts` → `inbound/<contact>/`).

use std::path::{Path, PathBuf};

use base64::Engine as _;
use md5::{Digest, Md5};

use crate::error::{BridgeError, Result};
use crate::wechat::auth::normalize_account_id;
use crate::wechat::client::WeixinClient;
use crate::wechat::media::{
    WEIXIN_MEDIA_MAX_BYTES, download_and_decrypt, download_plain, padded_size, upload_buffer_to_cdn,
};
use crate::wechat::types::{
    self, CDN_BASE_URL, CdnMedia, FileItem, GetUploadUrlResp, ImageItem, MessageItem, VideoItem,
    WeixinMessage,
};

fn b64() -> &'static base64::engine::GeneralPurpose {
    &base64::engine::general_purpose::STANDARD
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Upload pipeline (cdn/upload.ts uploadMediaToCdn)
// ---------------------------------------------------------------------------

/// Result of one successful media upload (UploadedFileInfo).
#[derive(Debug, Clone, PartialEq)]
pub struct UploadedMedia {
    pub filekey: String,
    /// CDN download `encrypted_query_param` for the receiver's CDNMedia.
    pub download_param: String,
    /// Raw AES key as hex (32 chars); goes to the wire base64-encoded.
    pub aeskey_hex: String,
    /// Plaintext size in bytes.
    pub file_size: usize,
    /// Ciphertext size (PKCS7 padded); use for mid_size / video_size.
    pub file_size_ciphertext: usize,
}

/// Common upload pipeline: read → md5 → random keys → getuploadurl → CDN
/// POST → [`UploadedMedia`].
pub async fn upload_media(
    client: &WeixinClient,
    file_path: &Path,
    to_user_id: &str,
    media_type: i64,
) -> Result<UploadedMedia> {
    let plaintext = std::fs::read(file_path)?;
    if plaintext.len() > WEIXIN_MEDIA_MAX_BYTES {
        return Err(BridgeError::Other(format!(
            "文件超过 {0} 字节上限，无法通过微信发送",
            WEIXIN_MEDIA_MAX_BYTES
        )));
    }
    let rawsize = plaintext.len();
    let mut hasher = Md5::new();
    hasher.update(&plaintext);
    let rawfilemd5 = to_hex(&hasher.finalize());
    let filesize = padded_size(rawsize);
    let filekey = to_hex(&rand::random::<[u8; 16]>());
    let aes_key = rand::random::<[u8; 16]>();

    let resp: GetUploadUrlResp = client
        .get_upload_url(crate::wechat::client::UploadUrlRequest {
            filekey: &filekey,
            media_type,
            to_user_id,
            rawsize,
            rawfilemd5: &rawfilemd5,
            filesize,
            aeskey_hex: &to_hex(&aes_key),
        })
        .await?;
    let download_param = upload_buffer_to_cdn(
        client.http(),
        &plaintext,
        resp.upload_full_url.as_deref(),
        resp.upload_param.as_deref(),
        &filekey,
        CDN_BASE_URL,
        &aes_key,
    )
    .await?;
    Ok(UploadedMedia {
        filekey,
        download_param,
        aeskey_hex: to_hex(&aes_key),
        file_size: rawsize,
        file_size_ciphertext: filesize,
    })
}

// ---------------------------------------------------------------------------
// Outbound message items (send.ts sendImage/Video/FileMessageWeixin)
// ---------------------------------------------------------------------------

fn outbound_media(aeskey_hex: &str, download_param: &str) -> CdnMedia {
    CdnMedia {
        encrypt_query_param: Some(download_param.to_string()),
        // Wire shape from send.ts: base64 of the 32-char hex string.
        aes_key: Some(b64().encode(aeskey_hex.as_bytes())),
        encrypt_type: Some(1),
        full_url: None,
    }
}

/// IMAGE message item from an upload result.
pub fn image_message_item(uploaded: &UploadedMedia) -> MessageItem {
    MessageItem {
        r#type: Some(types::item_type::IMAGE),
        image_item: Some(ImageItem {
            media: Some(outbound_media(
                &uploaded.aeskey_hex,
                &uploaded.download_param,
            )),
            mid_size: Some(uploaded.file_size_ciphertext as i64),
            ..ImageItem::default()
        }),
        ..MessageItem::default()
    }
}

/// VIDEO message item from an upload result.
pub fn video_message_item(uploaded: &UploadedMedia) -> MessageItem {
    MessageItem {
        r#type: Some(types::item_type::VIDEO),
        video_item: Some(VideoItem {
            media: Some(outbound_media(
                &uploaded.aeskey_hex,
                &uploaded.download_param,
            )),
            video_size: Some(uploaded.file_size_ciphertext as i64),
            ..VideoItem::default()
        }),
        ..MessageItem::default()
    }
}

/// FILE message item from an upload result (`len` is plaintext as string).
pub fn file_message_item(uploaded: &UploadedMedia, file_name: &str) -> MessageItem {
    MessageItem {
        r#type: Some(types::item_type::FILE),
        file_item: Some(FileItem {
            media: Some(outbound_media(
                &uploaded.aeskey_hex,
                &uploaded.download_param,
            )),
            file_name: Some(file_name.to_string()),
            len: Some(uploaded.file_size.to_string()),
            ..FileItem::default()
        }),
        ..MessageItem::default()
    }
}

/// Outbound MIME routing (sendWeixinMediaFile): image → IMAGE upload+item,
/// video → VIDEO, everything else → FILE attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    File,
}

pub fn kind_for_path(path: &Path) -> MediaKind {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => MediaKind::Image,
        Some("mp4" | "mov" | "avi" | "mkv" | "webm") => MediaKind::Video,
        _ => MediaKind::File,
    }
}

/// Upload a local file and send it as a native media message.
pub async fn upload_and_send(
    client: &WeixinClient,
    to: &str,
    context_token: Option<&str>,
    path: &Path,
) -> Result<()> {
    let kind = kind_for_path(path);
    let media_type = match kind {
        MediaKind::Image => types::upload_media_type::IMAGE,
        MediaKind::Video => types::upload_media_type::VIDEO,
        MediaKind::File => types::upload_media_type::FILE,
    };
    let uploaded = upload_media(client, path, to, media_type).await?;
    let item = match kind {
        MediaKind::Image => image_message_item(&uploaded),
        MediaKind::Video => video_message_item(&uploaded),
        MediaKind::File => file_message_item(
            &uploaded,
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file.bin"),
        ),
    };
    client
        .send_prepared(WeixinMessage::media_reply(
            to,
            item,
            context_token,
            &types::generate_client_id(),
        ))
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Inbound persistence (media-download.ts + framework saveMedia)
// ---------------------------------------------------------------------------

/// One decoded inbound media file plus a human-readable note for prompts.
#[derive(Debug, Clone, PartialEq)]
pub struct InboundAttachment {
    pub path: PathBuf,
    pub note: String,
}

fn human_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn sanitize_file_name(raw: &str) -> String {
    let name = Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file.bin")
        .trim()
        .to_string();
    if name.is_empty() {
        return "file.bin".to_string();
    }
    name.chars()
        .map(|ch| if ch == '/' || ch == '\\' { '_' } else { ch })
        .collect()
}

fn inbound_dir(codex_home: &Path, contact: &str) -> PathBuf {
    codex_home
        .join("weixin-bridge")
        .join("inbound")
        .join(normalize_account_id(contact))
}

/// Persist decoded media bytes under `inbound/<contact>/` (saveMedia).
pub fn save_inbound(
    codex_home: &Path,
    contact: &str,
    file_name: Option<&str>,
    data: &[u8],
) -> Result<PathBuf> {
    let dir = inbound_dir(codex_home, contact);
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let name = file_name
        .map(sanitize_file_name)
        .unwrap_or_else(|| format!("media-{stamp}.bin"));
    let mut path = dir.join(&name);
    if path.exists() {
        path = dir.join(format!("{stamp}-{name}"));
    }
    std::fs::write(&path, data)?;
    Ok(path)
}

/// Download + decrypt one media item; `Ok(None)` for unsupported or empty
/// items. Failures propagate so callers can warn per item.
async fn download_item(
    http: &reqwest::Client,
    codex_home: &Path,
    contact: &str,
    item: &MessageItem,
) -> Result<Option<InboundAttachment>> {
    match item.r#type {
        Some(types::item_type::IMAGE) => {
            let Some(img) = item.image_item.as_ref() else {
                return Ok(None);
            };
            let media = img.media.as_ref();
            let param = media
                .and_then(|m| m.encrypt_query_param.as_deref())
                .unwrap_or("");
            let full_url = media.and_then(|m| m.full_url.as_deref());
            if param.is_empty() && full_url.is_none() {
                return Ok(None);
            }
            let data = match img.aeskey.as_deref().filter(|k| !k.is_empty()) {
                // image_item.aeskey is hex; re-encode so parse_aes_key sees
                // base64(hex string), the format media.aes_key uses.
                Some(hex_key) => {
                    download_and_decrypt(
                        http,
                        full_url,
                        param,
                        &b64().encode(hex_key.as_bytes()),
                        CDN_BASE_URL,
                    )
                    .await?
                }
                None => match media.and_then(|m| m.aes_key.as_deref()) {
                    Some(aes_key) => {
                        download_and_decrypt(http, full_url, param, aes_key, CDN_BASE_URL).await?
                    }
                    None => download_plain(http, full_url, param, CDN_BASE_URL).await?,
                },
            };
            let path = save_inbound(codex_home, contact, None, &data)?;
            Ok(Some(InboundAttachment {
                path,
                note: format!("图片 {}", human_size(data.len())),
            }))
        }
        Some(types::item_type::VOICE) => {
            let Some(voice) = item.voice_item.as_ref() else {
                return Ok(None);
            };
            let Some(media) = voice.media.as_ref() else {
                return Ok(None);
            };
            let param = media.encrypt_query_param.as_deref().unwrap_or("");
            let full_url = media.full_url.as_deref();
            if param.is_empty() && full_url.is_none() {
                return Ok(None);
            }
            let Some(aes_key) = media.aes_key.as_deref() else {
                return Ok(None);
            };
            // SILK audio is stored as-is; no best-effort transcode here.
            let data = download_and_decrypt(http, full_url, param, aes_key, CDN_BASE_URL).await?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            let path = save_inbound(
                codex_home,
                contact,
                Some(&format!("voice-{stamp}.silk")),
                &data,
            )?;
            let transcript = voice
                .text
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| format!("，语音转写：{t}"))
                .unwrap_or_default();
            Ok(Some(InboundAttachment {
                path,
                note: format!("语音 ({}{})", human_size(data.len()), transcript),
            }))
        }
        Some(types::item_type::FILE) => {
            let Some(file) = item.file_item.as_ref() else {
                return Ok(None);
            };
            let Some(media) = file.media.as_ref() else {
                return Ok(None);
            };
            let param = media.encrypt_query_param.as_deref().unwrap_or("");
            let full_url = media.full_url.as_deref();
            if param.is_empty() && full_url.is_none() {
                return Ok(None);
            }
            let Some(aes_key) = media.aes_key.as_deref() else {
                return Ok(None);
            };
            let data = download_and_decrypt(http, full_url, param, aes_key, CDN_BASE_URL).await?;
            let path = save_inbound(codex_home, contact, file.file_name.as_deref(), &data)?;
            let name = file.file_name.as_deref().unwrap_or("file.bin");
            Ok(Some(InboundAttachment {
                path,
                note: format!("文件 {name} ({})", human_size(data.len())),
            }))
        }
        Some(types::item_type::VIDEO) => {
            let Some(video) = item.video_item.as_ref() else {
                return Ok(None);
            };
            let Some(media) = video.media.as_ref() else {
                return Ok(None);
            };
            let param = media.encrypt_query_param.as_deref().unwrap_or("");
            let full_url = media.full_url.as_deref();
            if param.is_empty() && full_url.is_none() {
                return Ok(None);
            }
            let Some(aes_key) = media.aes_key.as_deref() else {
                return Ok(None);
            };
            let data = download_and_decrypt(http, full_url, param, aes_key, CDN_BASE_URL).await?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default();
            let path = save_inbound(
                codex_home,
                contact,
                Some(&format!("video-{stamp}.mp4")),
                &data,
            )?;
            Ok(Some(InboundAttachment {
                path,
                note: format!("视频 ({})", human_size(data.len())),
            }))
        }
        _ => Ok(None),
    }
}

/// Fetch every media item of an inbound message to disk, best-effort per
/// item; failures are logged and skipped.
pub async fn fetch_inbound_attachments(
    client: &WeixinClient,
    codex_home: &Path,
    contact: &str,
    msg: &WeixinMessage,
) -> Vec<InboundAttachment> {
    let mut out = Vec::new();
    for item in msg.item_list.iter().flatten() {
        match download_item(client.http(), codex_home, contact, item).await {
            Ok(Some(attachment)) => out.push(attachment),
            Ok(None) => {}
            Err(err) => tracing::warn!(contact, "入站媒体下载失败: {err}"),
        }
    }
    out
}

/// Compose the codex task prompt: original text plus attachment list;
/// `None` when the message has neither usable text nor media.
pub fn build_attachment_prompt(text: &str, attachments: &[InboundAttachment]) -> Option<String> {
    let trimmed = text.trim();
    if attachments.is_empty() {
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    let mut out = String::new();
    if trimmed.is_empty() {
        out.push_str("请查看以下附件：\n");
    } else {
        out.push_str(trimmed);
        out.push_str("\n\n");
    }
    for attachment in attachments {
        out.push_str(&format!(
            "- [附件] {} ({})\n",
            attachment.path.display(),
            attachment.note
        ));
    }
    Some(out)
}

#[cfg(test)]
#[path = "media_transfer_tests.rs"]
mod tests;
