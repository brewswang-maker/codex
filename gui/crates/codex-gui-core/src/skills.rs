//! The skills inventory: the composer mention picker and the settings
//! toggles both render from one flattened `skills/list` payload.

use codex_app_server_protocol::SkillMetadata;
use codex_app_server_protocol::SkillsListResponse;
use std::path::PathBuf;

/// One flattened skill row driving the picker and the toggles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    /// The mention name as invoked in prompts (`$name`).
    pub name: String,
    /// One-line description from the skill's SKILL.md.
    pub description: String,
    /// Where the skill was discovered.
    pub scope: codex_app_server_protocol::SkillScope,
    /// On-disk skill directory, sent back with a `UserInput::Skill`
    /// mention so the backend resolves the invocation.
    pub path: PathBuf,
    /// Whether turns may use the skill.
    pub enabled: bool,
}

/// The loaded skills plus whether the composer picker is open.
#[derive(Debug, Default, Clone)]
pub struct SkillsBoard {
    /// Every discovered skill, including disabled ones.
    pub rows: Vec<SkillRow>,
    /// Whether the composer picker is showing.
    pub picker_open: bool,
}

impl SkillsBoard {
    /// Replaces the rows with a fresh `skills/list` response; entries for
    /// several cwds are flattened into one list.
    pub fn apply_list(&mut self, response: &SkillsListResponse) {
        self.rows = response
            .data
            .iter()
            .flat_map(|entry| entry.skills.iter())
            .map(skill_row)
            .collect();
    }

    /// The composer mention for an enabled skill; `None` for unknown or
    /// disabled names.
    pub fn mention(&self, name: &str) -> Option<String> {
        self.rows
            .iter()
            .find(|row| row.enabled && row.name == name)
            .map(|row| format!("${} ", row.name))
    }

    /// Mirrors a settled `skills/config/write` onto the matching row;
    /// returns whether a row matched.
    pub fn apply_enabled(&mut self, name: &str, enabled: bool) -> bool {
        match self.rows.iter_mut().find(|row| row.name == name) {
            Some(row) => {
                row.enabled = enabled;
                true
            }
            None => false,
        }
    }
}

fn skill_row(skill: &SkillMetadata) -> SkillRow {
    SkillRow {
        name: skill.name.clone(),
        description: skill.description.clone(),
        scope: skill.scope,
        path: skill.path.clone().into(),
        enabled: skill.enabled,
    }
}
