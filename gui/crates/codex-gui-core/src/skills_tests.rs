//! Headless coverage for the skills inventory.
#![allow(clippy::expect_used)]

use crate::skills::SkillRow;
use crate::skills::SkillsBoard;
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::SkillsListResponse;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::PathBuf;

/// A wire-shaped `skills/list` payload: one enabled user skill and one
/// disabled repo skill (scope rides the snake_case wire form).
fn list_response() -> SkillsListResponse {
    serde_json::from_value(json!({
        "data": [
            {
                "cwd": "/tmp",
                "errors": [],
                "skills": [
                    {
                        "name": "review",
                        "description": "review the current diff",
                        "path": "/tmp/skills/review/SKILL.md",
                        "scope": "user",
                        "enabled": true,
                        "pluginId": null
                    },
                    {
                        "name": "deploy",
                        "description": "ship the build",
                        "path": "/repo/.codex/skills/deploy/SKILL.md",
                        "scope": "repo",
                        "enabled": false,
                        "pluginId": null
                    }
                ]
            }
        ]
    }))
    .expect("the wire payload decodes")
}

#[test]
fn apply_list_flattens_entries_and_keeps_disabled_rows() {
    let mut board = SkillsBoard::default();

    board.apply_list(&list_response());

    assert_eq!(
        board.rows,
        vec![
            SkillRow {
                name: "review".to_string(),
                description: "review the current diff".to_string(),
                scope: SkillScope::User,
                path: PathBuf::from("/tmp/skills/review/SKILL.md"),
                enabled: true,
            },
            SkillRow {
                name: "deploy".to_string(),
                description: "ship the build".to_string(),
                scope: SkillScope::Repo,
                path: PathBuf::from("/repo/.codex/skills/deploy/SKILL.md"),
                enabled: false,
            },
        ]
    );
}

#[test]
fn mention_only_matches_enabled_skills() {
    let mut board = SkillsBoard::default();
    board.apply_list(&list_response());

    assert_eq!(
        board.mention("review"),
        Some("$review ".to_string()),
        "enabled skills mention by name"
    );
    assert_eq!(board.mention("deploy"), None, "disabled skills stay out");
    assert_eq!(board.mention("nope"), None, "unknown names stay out");
}

#[test]
fn apply_enabled_flips_only_the_matching_row() {
    let mut board = SkillsBoard::default();
    board.apply_list(&list_response());

    assert!(board.apply_enabled("deploy", true));
    assert_eq!(board.rows[1].enabled, true);
    assert_eq!(board.rows[0].enabled, true, "the other row is untouched");

    assert!(!board.apply_enabled("nope", true));
}
