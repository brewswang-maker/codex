#![allow(clippy::expect_used)]

use pretty_assertions::assert_eq;

use super::*;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("ascii"), 16).expect("hex"))
        .collect()
}

#[test]
fn padded_size_matches_js_aesecb() {
    // Math.ceil((n + 1) / 16) * 16: PKCS7 always appends a full block.
    assert_eq!(padded_size(0), 16);
    assert_eq!(padded_size(15), 16);
    assert_eq!(padded_size(16), 32);
    assert_eq!(padded_size(32), 48);
}

#[test]
fn aes_ecb_matches_nist_sp800_38a_vector() {
    // ECB-AES128.Encrypt (F.1.1) / Decrypt (F.1.2).
    let key: [u8; 16] = hex_to_bytes("000102030405060708090a0b0c0d0e0f")
        .try_into()
        .expect("16 bytes");
    let plaintext = hex_to_bytes("00112233445566778899aabbccddeeff");
    let expected = hex_to_bytes("69c4e0d86a7b0430d8cdb78070b4c55a");

    let ciphertext = encrypt_aes_ecb(&plaintext, &key);
    // The NIST vector covers the first block; PKCS7 adds a full pad block.
    assert_eq!(&ciphertext[..16], &expected[..]);

    let recovered = decrypt_aes_ecb(&ciphertext, &key).expect("valid padding");
    assert_eq!(recovered, plaintext);
}

#[test]
fn aes_ecb_roundtrip_and_full_block_padding() {
    let key = [7u8; 16];
    for len in [0usize, 1, 15, 16, 17, 255] {
        let plaintext: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let ciphertext = encrypt_aes_ecb(&plaintext, &key);
        assert_eq!(ciphertext.len(), padded_size(len));
        let recovered = decrypt_aes_ecb(&ciphertext, &key).expect("roundtrip");
        assert_eq!(recovered, plaintext, "len={len}");
    }
    // A 16-byte plaintext pads to 32; the pad block round-trips back.
    let block_ciphertext = encrypt_aes_ecb(&[0u8; 16], &key);
    assert_eq!(block_ciphertext.len(), 32);
    let recovered_block = decrypt_aes_ecb(&block_ciphertext, &key).expect("roundtrip");
    assert_eq!(recovered_block, vec![0u8; 16]);
}

#[test]
fn decrypt_rejects_bad_alignment_and_padding() {
    let key = [1u8; 16];
    assert!(decrypt_aes_ecb(&[], &key).is_err());
    assert!(decrypt_aes_ecb(&[0u8; 15], &key).is_err());
    // Decrypting with the wrong key almost surely breaks PKCS7.
    let wrong_key = [2u8; 16];
    let ciphertext = encrypt_aes_ecb(b"hello", &key);
    assert!(decrypt_aes_ecb(&ciphertext, &wrong_key).is_err());
}

#[test]
fn parse_aes_key_accepts_both_wire_encodings() {
    // base64(raw 16 bytes) — images.
    let raw = [0xaau8; 16];
    let parsed = parse_aes_key(&base64::engine::general_purpose::STANDARD.encode(raw))
        .expect("raw encoding");
    assert_eq!(parsed, raw);

    // base64(hex string of 16 bytes) — file/voice/video.
    let hex_key = "0123456789abcdef0123456789abcdef";
    let parsed = parse_aes_key(&base64::engine::general_purpose::STANDARD.encode(hex_key))
        .expect("hex encoding");
    assert_eq!(parsed.to_vec(), hex_to_bytes(hex_key));

    // Wrong lengths and non-hex content fail.
    let short = base64::engine::general_purpose::STANDARD.encode([0u8; 8]);
    assert!(parse_aes_key(&short).is_err());
    let not_hex =
        base64::engine::general_purpose::STANDARD.encode("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    assert!(parse_aes_key(&not_hex).is_err());
}

#[test]
fn encode_uri_component_matches_js_escaping() {
    assert_eq!(
        encode_uri_component("a+b/c d=e&f?g#h"),
        "a%2Bb%2Fc%20d%3De%26f%3Fg%23h"
    );
    // encodeURIComponent leaves -_.!~*'() untouched.
    assert_eq!(encode_uri_component("-_.!~*'()"), "-_.!~*'()");
    assert_eq!(encode_uri_component("AZaz09"), "AZaz09");
}

#[test]
fn cdn_urls_match_js_builders() {
    assert_eq!(
        build_download_url("abc+123/", "https://novac2c.cdn.weixin.qq.com/c2c/"),
        "https://novac2c.cdn.weixin.qq.com/c2c/download?encrypted_query_param=abc%2B123%2F"
    );
    assert_eq!(
        build_upload_url("up?param", "key1", "https://novac2c.cdn.weixin.qq.com/c2c"),
        "https://novac2c.cdn.weixin.qq.com/c2c/upload?encrypted_query_param=up%3Fparam&filekey=key1"
    );
}
