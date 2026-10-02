//! Header and version-encoding tests for the iLink HTTP client.

use super::*;

#[test]
fn client_version_encodes_major_minor_patch() {
    // Documented example from the JS channel: "1.0.11" -> 0x0001000B.
    assert_eq!(client_version_u32("1.0.11"), 0x0001_000B);
    assert_eq!(client_version_u32("0.1.0"), 256); // 0<<16 | 1<<8
    assert_eq!(client_version_u32("2.3.4"), 0x0002_0304);
    // Partial or odd inputs degrade component-wise to zero.
    assert_eq!(client_version_u32("7"), 0x0007_0000);
    assert_eq!(client_version_u32("x.y.z"), 0);
}

#[test]
fn wechat_uin_header_is_base64_of_a_decimal_uint32() {
    use base64::Engine as _;
    let encoded = random_wechat_uin();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .expect("valid base64");
    let text = String::from_utf8(decoded).expect("decimal utf-8");
    let value: u32 = text.parse().expect("decimal uint32");
    // Random per call, but always a full-width decimal rendering of a u32.
    assert!(text.chars().all(|c| c.is_ascii_digit()));
    let _ = value;
}

#[test]
fn urls_join_without_double_slashes() {
    let base = "https://ilinkai.weixin.qq.com";
    assert_eq!(
        WeixinClient::url(base, "ilink/bot/getupdates"),
        "https://ilinkai.weixin.qq.com/ilink/bot/getupdates"
    );
    assert_eq!(
        WeixinClient::url("https://host.example/", "path"),
        "https://host.example/path"
    );
}

#[test]
fn base_info_carries_identity_fields() {
    let client = WeixinClient::new("https://ilinkai.weixin.qq.com", None)
        .expect("client builds without network");
    let info = client.base_info();
    assert_eq!(
        info.bot_agent.as_deref(),
        Some(concat!("codex-weixin-bridge/", env!("CARGO_PKG_VERSION")))
    );
    assert!(info.channel_version.is_some());
}
