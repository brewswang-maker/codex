//! Tests for the request-payload constructors in [`crate::commands`].

#![allow(clippy::expect_used, clippy::panic)]

use super::preview_body;
use super::thread_start_params;
use crate::state::FileBody;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::SandboxMode;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn thread_start_params_pins_the_working_directory_when_given() {
    let params = thread_start_params(Some("/tmp/proj".to_string()));

    assert_eq!(params.cwd, Some("/tmp/proj".to_string()));
}

#[test]
fn thread_start_params_defaults_to_the_server_working_directory() {
    let params = thread_start_params(None);

    assert_eq!(params.cwd, None);
}

#[test]
fn thread_start_params_auto_approves_workspace_edits() {
    // The server default (`UnlessTrusted` for untrusted projects) prompts on
    // every patch, and "always allow" only remembers the same file, so the
    // GUI pins a policy that auto-approves edits inside the writable sandbox.
    let params = thread_start_params(None);

    assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
    assert_eq!(params.sandbox, Some(SandboxMode::WorkspaceWrite));
}

#[test]
fn preview_body_renders_utf8_files_as_text() {
    let path = temp_file("preview-text", b"hello preview");

    assert_eq!(
        preview_body(&path),
        FileBody::Text(Arc::from("hello preview"))
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn preview_body_reports_non_utf8_files_as_binary() {
    // 0xFF and 0xFE are never valid in UTF-8.
    let path = temp_file("preview-binary", b"\xFF\xFEbytes");

    assert_eq!(preview_body(&path), FileBody::Binary);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn preview_body_flags_oversized_files_without_reading_them() {
    let path = std::env::temp_dir().join(format!("preview-large-{}.bin", std::process::id()));
    // A sparse file crosses the cap without touching the disk blocks.
    File::create(&path)
        .and_then(|file| file.set_len(super::PREVIEW_LIMIT + 1))
        .expect("sparse fixture creates");

    assert_eq!(preview_body(&path), FileBody::TooLarge);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn preview_body_carries_the_io_failure_reason() {
    let path = PathBuf::from(format!(
        "/nonexistent/codex-gui-preview-{}",
        std::process::id()
    ));

    assert!(matches!(preview_body(&path), FileBody::Failed(_)));
}

/// A unique temporary file with the given byte contents.
fn temp_file(tag: &str, contents: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{tag}-{}.txt", std::process::id()));
    std::fs::write(&path, contents).expect("temp fixture writes");
    path
}
