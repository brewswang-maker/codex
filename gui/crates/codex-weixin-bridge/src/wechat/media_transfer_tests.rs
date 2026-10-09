#![allow(clippy::expect_used)]

use std::path::Path;

use pretty_assertions::assert_eq;

use super::*;

fn sample_upload() -> UploadedMedia {
    UploadedMedia {
        filekey: "0123456789abcdef0123456789abcdef".to_string(),
        download_param: "dl+param/1".to_string(),
        aeskey_hex: "00112233445566778899aabbccddeeff".to_string(),
        file_size: 1000,
        file_size_ciphertext: 1008,
    }
}

#[test]
fn kind_for_path_routes_by_extension() {
    assert_eq!(kind_for_path(Path::new("shot.PNG")), MediaKind::Image);
    assert_eq!(
        kind_for_path(Path::new("/x/y/photo.jpeg")),
        MediaKind::Image
    );
    assert_eq!(kind_for_path(Path::new("clip.MP4")), MediaKind::Video);
    assert_eq!(kind_for_path(Path::new("a/b/movie.mkv")), MediaKind::Video);
    assert_eq!(kind_for_path(Path::new("report.pdf")), MediaKind::File);
    assert_eq!(kind_for_path(Path::new("noext")), MediaKind::File);
}

#[test]
fn sanitize_file_name_strips_paths_and_separators() {
    assert_eq!(sanitize_file_name("a/b/c.txt"), "c.txt");
    // On Unix a backslash is just a character inside one component; the
    // sanitizer still rewrites it so the name is portable.
    assert_eq!(sanitize_file_name("..\\evil.exe"), ".._evil.exe");
    assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
    assert_eq!(sanitize_file_name("   "), "file.bin");
    assert_eq!(sanitize_file_name(""), "file.bin");
}

#[test]
fn save_inbound_lands_under_normalized_contact_dir() {
    let home = tempfile::tempdir().expect("tmp");
    let path = save_inbound(home.path(), "hex@im.bot", Some("data.bin"), b"payload").expect("save");
    assert_eq!(
        path.parent().expect("dir"),
        home.path().join("weixin-bridge/inbound/hex-im-bot")
    );
    assert_eq!(std::fs::read(&path).expect("bytes"), b"payload");

    // Same name again: collision gets a stamp prefix instead of overwriting.
    let second = save_inbound(home.path(), "hex@im.bot", Some("data.bin"), b"v2").expect("save");
    assert_ne!(path, second);
    assert!(second.exists());
    assert_eq!(std::fs::read(&second).expect("bytes"), b"v2");

    // Anonymous media uses a generated name.
    let anon = save_inbound(home.path(), "hex@im.bot", None, b"img").expect("save");
    assert!(
        anon.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("media-"))
    );
}

#[test]
fn build_attachment_prompt_covers_all_shapes() {
    // Plain text, no media.
    assert_eq!(
        build_attachment_prompt("修复 bug", &[]),
        Some("修复 bug".to_string())
    );
    // Neither text nor media: nothing to enqueue.
    assert_eq!(build_attachment_prompt("  \n", &[]), None);

    let attachments = vec![InboundAttachment {
        path: "/tmp/inbound/report.pdf".into(),
        note: "文件 report.pdf (1.2 MB)".to_string(),
    }];
    // Text + media.
    assert_eq!(
        build_attachment_prompt("看看这个", &attachments),
        Some(
            "看看这个\n\n- [附件] /tmp/inbound/report.pdf (文件 report.pdf (1.2 MB))\n".to_string()
        )
    );
    // Media only.
    assert_eq!(
        build_attachment_prompt("", &attachments),
        Some(
            "请查看以下附件：\n- [附件] /tmp/inbound/report.pdf (文件 report.pdf (1.2 MB))\n"
                .to_string()
        )
    );
}

#[test]
fn outbound_items_carry_cdn_refs_in_wire_shape() {
    let uploaded = sample_upload();

    let image = image_message_item(&uploaded);
    assert_eq!(image.r#type, Some(crate::wechat::types::item_type::IMAGE));
    let img = image.image_item.expect("image item");
    let media = img.media.expect("cdn media");
    // send.ts: aes_key = base64 of the 32-char hex string.
    assert_eq!(
        media.aes_key.as_deref(),
        Some(
            base64::engine::general_purpose::STANDARD
                .encode(uploaded.aeskey_hex.as_bytes())
                .as_str()
        )
    );
    assert_eq!(media.encrypt_query_param.as_deref(), Some("dl+param/1"));
    assert_eq!(media.encrypt_type, Some(1));
    assert_eq!(img.mid_size, Some(1008));

    let video = video_message_item(&uploaded);
    let vid = video.video_item.expect("video item");
    assert_eq!(vid.video_size, Some(1008));
    assert_eq!(vid.media.expect("cdn media").encrypt_type, Some(1));

    let file = file_message_item(&uploaded, "report.pdf");
    assert_eq!(file.r#type, Some(crate::wechat::types::item_type::FILE));
    let file_item = file.file_item.expect("file item");
    assert_eq!(file_item.file_name.as_deref(), Some("report.pdf"));
    assert_eq!(file_item.len.as_deref(), Some("1000"));
}

#[test]
fn human_size_formats_tiers() {
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(2048), "2.0 KB");
    assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
}
