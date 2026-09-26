//! Tests for the request-payload constructors in [`crate::commands`].

#![allow(clippy::expect_used, clippy::panic)]

use super::SLASH_COMMANDS;
use super::SlashAction;
use super::link_import_notice;
use super::preview_body;
use super::slash_action;
use super::slash_options;
use super::thread_list_params;
use super::thread_resume_params;
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
fn thread_resume_params_keeps_the_workspace_approval_posture() {
    // Resuming an older thread must not resurrect the server-default
    // policy that prompts on every patch.
    let params = thread_resume_params(String::from("thr-1"));

    assert_eq!(params.thread_id, String::from("thr-1"));
    assert_eq!(params.approval_policy, Some(AskForApproval::OnRequest));
    assert_eq!(params.sandbox, Some(SandboxMode::WorkspaceWrite));
}

#[test]
fn thread_list_params_asks_for_every_model_provider() {
    // The server reads an omitted provider filter as "the current config
    // provider", so the sidebar inventory would drop every thread recorded
    // under another vendor right after a model switch; the constructor pins
    // the explicit empty list that means "all providers".
    let params = thread_list_params(None);

    assert_eq!(params.model_providers, Some(Vec::new()));
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

#[test]
fn link_import_notice_names_a_single_skill_by_path() {
    let imported = [PathBuf::from("/skills/short-drama/SKILL.md")];

    assert_eq!(
        link_import_notice(&imported),
        "已从链接导入 skill（/skills/short-drama/SKILL.md）"
    );
}

#[test]
fn link_import_notice_summarizes_batches_with_the_first_names() {
    let batch = [
        "/skills/short-drama/SKILL.md",
        "/skills/short-drama-write/SKILL.md",
        "/skills/short-drama-storyboard/SKILL.md",
    ]
    .map(PathBuf::from);

    assert_eq!(
        link_import_notice(&batch),
        "已从链接导入 3 个 skill：short-drama、short-drama-write、short-drama-storyboard"
    );

    let many: Vec<PathBuf> = (0..12)
        .map(|index| PathBuf::from(format!("/skills/skill-{index:02}/SKILL.md")))
        .collect();

    assert_eq!(
        link_import_notice(&many),
        "已从链接导入 12 个 skill：skill-00、skill-01、skill-02 等"
    );
}

/// A unique temporary file with the given byte contents.
fn temp_file(tag: &str, contents: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{tag}-{}.txt", std::process::id()));
    std::fs::write(&path, contents).expect("temp fixture writes");
    path
}

#[test]
fn slash_action_resolves_the_exact_commands() {
    assert_eq!(slash_action("/review"), Some(SlashAction::Review));
    assert_eq!(slash_action("  /compact  "), Some(SlashAction::Compact));
    // An argument or an ordinary draft is not a command.
    assert_eq!(slash_action("/review now"), None);
    assert_eq!(slash_action("hello"), None);
}

#[test]
fn slash_options_filter_by_prefix_while_typing() {
    // A bare slash offers every command; letters narrow the list.
    assert_eq!(slash_options("/").len(), SLASH_COMMANDS.len());

    let narrowed = slash_options("/r");
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].0, "/review");
}

#[test]
fn slash_options_hide_once_the_word_is_committed() {
    // Text after the command word, or a non-slash draft, hides the picker.
    assert!(slash_options("/review ").is_empty());
    assert!(slash_options("hello").is_empty());
}
