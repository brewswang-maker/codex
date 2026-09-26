//! Headless coverage for the skills inventory.
#![allow(clippy::expect_used)]

use crate::skills::SkillNotice;
use crate::skills::SkillRow;
use crate::skills::SkillSource;
use crate::skills::SkillsBoard;
use crate::skills::SkillsTab;
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
                plugin_id: None,
            },
            SkillRow {
                name: "deploy".to_string(),
                description: "ship the build".to_string(),
                scope: SkillScope::Repo,
                path: PathBuf::from("/repo/.codex/skills/deploy/SKILL.md"),
                enabled: false,
                plugin_id: None,
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

#[test]
fn source_classifies_rows_by_plugin_and_scope() {
    let mut board = SkillsBoard::default();
    board.apply_list(&list_response());
    board.rows.push(SkillRow {
        name: "pdf-split".to_string(),
        description: "split pdfs".to_string(),
        scope: SkillScope::User,
        path: PathBuf::from("/plugins/pdf-tools/skills/pdf-split/SKILL.md"),
        enabled: true,
        plugin_id: Some("pdf-tools@local-market".to_string()),
    });
    board.rows.push(SkillRow {
        name: "guardian".to_string(),
        description: "system policy".to_string(),
        scope: SkillScope::System,
        path: PathBuf::from("/etc/codex/skills/guardian/SKILL.md"),
        enabled: true,
        plugin_id: None,
    });

    assert_eq!(board.rows[0].source(), SkillSource::Local);
    assert_eq!(board.rows[1].source(), SkillSource::Local);
    assert_eq!(board.rows[2].source(), SkillSource::Plugin);
    assert_eq!(board.rows[3].source(), SkillSource::System);

    assert!(board.rows[0].editable(), "user skills are editable");
    assert!(!board.rows[2].editable(), "plugin skills stay read-only");
    assert!(board.rows[2].deletable(), "plugin skills uninstall");
    assert!(!board.rows[3].deletable(), "system skills are protected");
}

#[test]
fn remove_drops_only_the_matching_row() {
    let mut board = SkillsBoard::default();
    board.apply_list(&list_response());

    assert!(board.remove("review"));
    assert_eq!(board.rows.len(), 1);
    assert_eq!(board.rows[0].name, "deploy");
    assert!(!board.remove("review"), "the second removal is a miss");
}

#[test]
fn remove_plugin_drops_every_bundled_row() {
    let mut board = SkillsBoard {
        rows: vec![
            plugin_row("pdf-split", "pdf-tools@local-market"),
            plugin_row("pdf-merge", "pdf-tools@local-market"),
            plugin_row("writer", "writer@official"),
        ],
        ..SkillsBoard::default()
    };

    assert_eq!(board.remove_plugin("pdf-tools@local-market"), 2);
    assert_eq!(board.rows.len(), 1);
    assert_eq!(board.rows[0].name, "writer");
    assert_eq!(board.remove_plugin("pdf-tools@local-market"), 0);
}

/// One plugin-shipped row for the removal tests.
fn plugin_row(name: &str, plugin_id: &str) -> SkillRow {
    SkillRow {
        name: name.to_string(),
        description: format!("{name} description"),
        scope: SkillScope::User,
        path: PathBuf::from(format!("/plugins/{plugin_id}/skills/{name}/SKILL.md")),
        enabled: true,
        plugin_id: Some(plugin_id.to_string()),
    }
}

#[test]
fn page_toggle_clears_the_transient_forms() {
    let mut board = SkillsBoard::default();

    assert!(board.toggle_page(), "the first flip opens");
    board.open_add();
    board.tab = SkillsTab::Market;
    board.notice = Some(SkillNotice::ok("saved"));

    assert!(!board.toggle_page(), "the second flip closes");
    assert_eq!(board.add, None, "closing drops the add form");
    assert_eq!(board.notice, None, "closing drops the notice");
    assert_eq!(board.tab, SkillsTab::Market, "the tab survives a close");

    board.rows.push(SkillRow {
        name: "review".to_string(),
        description: "review the current diff".to_string(),
        scope: SkillScope::User,
        path: PathBuf::from("/tmp/skills/review/SKILL.md"),
        enabled: true,
        plugin_id: None,
    });
    assert!(board.open_edit("review"), "known rows open the editor");
    assert_eq!(
        board
            .editor
            .as_ref()
            .map(|editor| editor.name_draft.clone()),
        Some("review".to_string())
    );
    assert_eq!(board.add, None, "opening the editor drops the add form");
    assert!(!board.open_edit("nope"), "unknown rows stay closed");
}
