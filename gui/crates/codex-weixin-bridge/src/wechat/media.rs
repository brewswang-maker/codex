//! WeChat CDN media protocol primitives (D3): AES-128-ECB PKCS7, CDN URL
//! construction, encrypted download and upload. Composition layers (upload
//! pipeline, outbound items, inbound persistence) live in
//! [`super::media_transfer`].
//!
//! Line-by-line port of openclaw-weixin 2.4.9: `cdn/aes-ecb.ts`,
//! `cdn/cdn-url.ts`, `cdn/pic-decrypt.ts` and `cdn/cdn-upload.ts`.

use std::time::Duration;

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use base64::Engine as _;

use crate::error::{BridgeError, Result};

/// Hard cap for a single inbound media download (mirrors media-download.ts).
pub const WEIXIN_MEDIA_MAX_BYTES: usize = 100 * 1024 * 1024;
/// CDN request timeout (mirrors the fetch defaults in pic-decrypt.ts).
const CDN_TIMEOUT: Duration = Duration::from_secs(60);
/// Upload retries on server errors; 4xx aborts immediately (cdn-upload.ts).
const UPLOAD_MAX_RETRIES: usize = 3;

fn b64() -> &'static base64::engine::GeneralPurpose {
    &base64::engine::general_purpose::STANDARD
}

// ---------------------------------------------------------------------------
// AES-128-ECB with PKCS7 padding (aes-ecb.ts)
// ---------------------------------------------------------------------------

/// PKCS7-padded AES-128-ECB ciphertext size (aesEcbPaddedSize).
pub fn padded_size(plaintext_len: usize) -> usize {
    (plaintext_len + 1).div_ceil(16) * 16
}

/// Encrypt with AES-128-ECB + PKCS7 (encryptAesEcb).
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let cipher = aes::Aes128::new(GenericArray::from_slice(key));
    let padded_len = padded_size(plaintext.len());
    let pad = (padded_len - plaintext.len()) as u8;
    let mut buf = Vec::with_capacity(padded_len);
    buf.extend_from_slice(plaintext);
    buf.resize(padded_len, pad);
    for chunk in buf.chunks_mut(16) {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }
    buf
}

/// Decrypt AES-128-ECB and strip PKCS7 (decryptAesEcb); fails on non-block
/// aligned input or malformed padding.
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>> {
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(16) {
        return Err(BridgeError::Other(format!(
            "CDN 密文长度 {0} 不是 16 字节的倍数",
            ciphertext.len()
        )));
    }
    let cipher = aes::Aes128::new(GenericArray::from_slice(key));
    let mut buf = ciphertext.to_vec();
    for chunk in buf.chunks_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
    }
    let Some(&last) = buf.last() else {
        return Err(BridgeError::Other("CDN 密文为空".to_string()));
    };
    let pad = last as usize;
    if pad == 0
        || pad > 16
        || pad > buf.len()
        || buf[buf.len() - pad..].iter().any(|&b| b as usize != pad)
    {
        return Err(BridgeError::Other(
            "CDN 媒体 PKCS7 填充无效（密钥错误？）".to_string(),
        ));
    }
    buf.truncate(buf.len() - pad);
    Ok(buf)
}

/// Parse `CDNMedia.aes_key` into a raw 16-byte key (parseAesKey). Two wire
/// encodings exist: base64(raw 16 bytes) for images and
/// base64(hex string of 16 bytes) for file/voice/video.
pub fn parse_aes_key(aes_key_base64: &str) -> Result<[u8; 16]> {
    let decoded = b64()
        .decode(aes_key_base64.trim())
        .map_err(|err| BridgeError::Other(format!("aes_key base64 解码失败: {err}")))?;
    if let Ok(key) = <[u8; 16]>::try_from(decoded.as_slice()) {
        return Ok(key);
    }
    if decoded.len() == 32 && decoded.iter().all(u8::is_ascii_hexdigit) {
        let mut key = [0u8; 16];
        for (slot, chunk) in key.iter_mut().zip(decoded.chunks(2)) {
            let hex = std::str::from_utf8(chunk)
                .map_err(|_| BridgeError::Other("aes_key hex 段非法".to_string()))?;
            *slot = u8::from_str_radix(hex, 16)
                .map_err(|_| BridgeError::Other("aes_key hex 段非法".to_string()))?;
        }
        return Ok(key);
    }
    Err(BridgeError::Other(format!(
        "aes_key 需解码为 16 字节或 32 位 hex 字符串，实际 {0} 字节",
        decoded.len()
    )))
}

// ---------------------------------------------------------------------------
// CDN URLs (cdn-url.ts)
// ---------------------------------------------------------------------------

/// JS `encodeURIComponent`: everything except A-Za-z0-9-_.!~*'() is escaped.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn build_download_url(encrypted_query_param: &str, cdn_base: &str) -> String {
    format!(
        "{}/download?encrypted_query_param={}",
        cdn_base.trim_end_matches('/'),
        encode_uri_component(encrypted_query_param)
    )
}

pub fn build_upload_url(upload_param: &str, filekey: &str, cdn_base: &str) -> String {
    format!(
        "{}/upload?encrypted_query_param={}&filekey={}",
        cdn_base.trim_end_matches('/'),
        encode_uri_component(upload_param),
        encode_uri_component(filekey)
    )
}

// ---------------------------------------------------------------------------
// Download (pic-decrypt.ts)
// ---------------------------------------------------------------------------

/// Download raw bytes from the CDN (fetchCdnBytes), capped at
/// [`WEIXIN_MEDIA_MAX_BYTES`].
pub async fn fetch_cdn_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = http
        .get(url)
        .timeout(CDN_TIMEOUT)
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?;
    if bytes.len() > WEIXIN_MEDIA_MAX_BYTES {
        return Err(BridgeError::Other(format!(
            "CDN 媒体超过上限 {0} 字节（实际 {1}）",
            WEIXIN_MEDIA_MAX_BYTES,
            bytes.len()
        )));
    }
    Ok(bytes.to_vec())
}

fn resolve_url(full_url: Option<&str>, encrypted_query_param: &str, cdn_base: &str) -> String {
    match full_url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(full) => full.to_string(),
        // ENABLE_CDN_URL_FALLBACK stays on, like the JS channel.
        None => build_download_url(encrypted_query_param, cdn_base),
    }
}

/// Download and AES-decrypt a CDN media file (downloadAndDecryptBuffer).
pub async fn download_and_decrypt(
    http: &reqwest::Client,
    full_url: Option<&str>,
    encrypted_query_param: &str,
    aes_key_base64: &str,
    cdn_base: &str,
) -> Result<Vec<u8>> {
    let key = parse_aes_key(aes_key_base64)?;
    let ciphertext = fetch_cdn_bytes(
        http,
        &resolve_url(full_url, encrypted_query_param, cdn_base),
    )
    .await?;
    decrypt_aes_ecb(&ciphertext, &key)
}

/// Download plain (unencrypted) CDN bytes (downloadPlainCdnBuffer).
pub async fn download_plain(
    http: &reqwest::Client,
    full_url: Option<&str>,
    encrypted_query_param: &str,
    cdn_base: &str,
) -> Result<Vec<u8>> {
    fetch_cdn_bytes(
        http,
        &resolve_url(full_url, encrypted_query_param, cdn_base),
    )
    .await
}

// ---------------------------------------------------------------------------
// Upload primitive (cdn-upload.ts uploadBufferToCdn)
// ---------------------------------------------------------------------------

/// Upload one buffer to the CDN (uploadBufferToCdn): AES encrypt, POST as
/// octet-stream, retry server errors up to 3 times, abort on 4xx. Returns
/// the `x-encrypted-param` download param.
pub async fn upload_buffer_to_cdn(
    http: &reqwest::Client,
    plaintext: &[u8],
    upload_full_url: Option<&str>,
    upload_param: Option<&str>,
    filekey: &str,
    cdn_base: &str,
    aes_key: &[u8; 16],
) -> Result<String> {
    let ciphertext = encrypt_aes_ecb(plaintext, aes_key);
    let url = match upload_full_url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(full) => full.to_string(),
        None => match upload_param {
            Some(param) => build_upload_url(param, filekey, cdn_base),
            None => {
                return Err(BridgeError::Other(
                    "CDN 上传地址缺失（需要 upload_full_url 或 upload_param）".to_string(),
                ));
            }
        },
    };

    let mut last_error: Option<BridgeError> = None;
    for attempt in 1..=UPLOAD_MAX_RETRIES {
        match post_ciphertext(http, &url, &ciphertext).await {
            Ok(download_param) => return Ok(download_param),
            Err(err) if matches!(&err, BridgeError::Other(msg) if msg.contains("client error")) => {
                return Err(err);
            }
            Err(err) => {
                tracing::warn!(attempt, "CDN 上传失败，准备重试: {err}");
                last_error = Some(err);
            }
        }
    }
    Err(last_error.unwrap_or(BridgeError::Other("CDN 上传在全部重试后失败".to_string())))
}

async fn post_ciphertext(http: &reqwest::Client, url: &str, ciphertext: &[u8]) -> Result<String> {
    let response = http
        .post(url)
        .timeout(CDN_TIMEOUT)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(ciphertext.to_vec())
        .send()
        .await?;
    let status = response.status();
    let error_header = response
        .headers()
        .get("x-error-message")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    if status.is_client_error() {
        return Err(BridgeError::Other(format!(
            "CDN upload client error {status}: {}",
            error_header.unwrap_or_default()
        )));
    }
    if status.as_u16() != 200 {
        return Err(BridgeError::Other(format!(
            "CDN upload server error: {}",
            error_header.unwrap_or_else(|| format!("status {status}"))
        )));
    }
    let download_param = response
        .headers()
        .get("x-encrypted-param")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if download_param.is_empty() {
        return Err(BridgeError::Other(
            "CDN 上传响应缺少 x-encrypted-param 头".to_string(),
        ));
    }
    Ok(download_param)
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
