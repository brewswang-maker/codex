//! The skills inventory: the composer mention picker, the settings toggles,
//! and the full-screen Skills page all render from one flattened
//! `skills/list` payload, plus the market board behind the page's second
//! tab.

use crate::skill_market::SkillMarket;
use codex_app_server_protocol::SkillMetadata;
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::SkillsListResponse;
use std::path::PathBuf;

/// Where a skill comes from, derived from the wire metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// A user- or repo-level SKILL.md managed on this machine.
    Local,
    /// Shipped by an installed plugin.
    Plugin,
    /// Installed system-wide or by the admin.
    System,
}

/// One page notice: the success text or the failure reason of the last
/// mutation, plus which of the two it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillNotice {
    /// The message rendered under the page header.
    pub text: String,
    /// Whether the message reports a failure.
    pub error: bool,
}

impl SkillNotice {
    /// A success notice.
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }

    /// A failure notice.
    pub fn failed(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }
}

impl SkillSource {
    /// The badge shown on the Skills page.
    pub fn label(self) -> &'static str {
        match self {
            SkillSource::Local => "本地",
            SkillSource::Plugin => "插件",
            SkillSource::System => "系统",
        }
    }
}

/// One flattened skill row driving the picker, the toggles, and the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    /// The mention name as invoked in prompts (`$name`).
    pub name: String,
    /// One-line description from the skill's SKILL.md.
    pub description: String,
    /// Where the skill was discovered.
    pub scope: SkillScope,
    /// On-disk skill directory, sent back with a `UserInput::Skill`
    /// mention so the backend resolves the invocation.
    pub path: PathBuf,
    /// Whether turns may use the skill.
    pub enabled: bool,
    /// Owning plugin id when the skill ships inside a plugin.
    pub plugin_id: Option<String>,
}

impl SkillRow {
    /// Where this skill comes from.
    pub fn source(&self) -> SkillSource {
        if self.plugin_id.is_some() {
            return SkillSource::Plugin;
        }
        match self.scope {
            SkillScope::User | SkillScope::Repo => SkillSource::Local,
            SkillScope::System | SkillScope::Admin => SkillSource::System,
        }
    }

    /// Whether the page may rewrite this skill's `SKILL.md`.
    pub fn editable(&self) -> bool {
        self.source() == SkillSource::Local
    }

    /// Whether the page may remove this skill (local directory removal or
    /// plugin uninstall).
    pub fn deletable(&self) -> bool {
        self.source() != SkillSource::System
    }
}

/// Which half of the Skills page is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillsTab {
    /// The managed inventory.
    #[default]
    Manage,
    /// The installable plugin marketplace.
    Market,
}

/// The add-skill form: create fresh, import from a picked path, or clone a
/// Git link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillAdd {
    /// Name draft for a fresh skill; ignored while a source is picked or a
    /// link is set.
    pub name: String,
    /// Description draft for a fresh skill; ignored while a source is
    /// picked or a link is set.
    pub description: String,
    /// The picked import source (a skill directory or a SKILL.md file);
    /// `None` creates a fresh skill from the drafts.
    pub source: Option<PathBuf>,
    /// Git link draft; when non-empty it wins over the drafts and the
    /// picked source.
    pub link: String,
    /// Whether a submit is still running (link imports clone over the
    /// network).
    pub busy: bool,
}

/// The edit form of one local skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEdit {
    /// The row this form edits, as listed.
    pub name: String,
    /// `SKILL.md` path being rewritten.
    pub path: PathBuf,
    /// Draft name.
    pub name_draft: String,
    /// Draft description.
    pub description_draft: String,
}

/// The loaded skills plus the composer picker and the page's admin state.
#[derive(Debug, Default, Clone)]
pub struct SkillsBoard {
    /// Every discovered skill, including disabled ones.
    pub rows: Vec<SkillRow>,
    /// Whether the composer picker is showing.
    pub picker_open: bool,
    /// Whether the full-screen Skills page is up.
    pub page_open: bool,
    /// Which half of the page is on screen.
    pub tab: SkillsTab,
    /// The add form, while open.
    pub add: Option<SkillAdd>,
    /// The edit form, while open.
    pub editor: Option<SkillEdit>,
    /// The skill name whose removal awaits confirmation.
    pub confirm_delete: Option<String>,
    /// The page's last notice (success or failure text).
    pub notice: Option<SkillNotice>,
    /// The market tab's state.
    pub market: SkillMarket,
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

    /// Drops one removed row; returns whether a row matched.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.rows.len();
        self.rows.retain(|row| row.name != name);
        self.rows.len() != before
    }

    /// Drops every row shipped by one uninstalled plugin; returns how many
    /// rows matched.
    pub fn remove_plugin(&mut self, plugin_id: &str) -> usize {
        let before = self.rows.len();
        self.rows
            .retain(|row| row.plugin_id.as_deref() != Some(plugin_id));
        before - self.rows.len()
    }

    /// Closes the page, dropping its transient forms and the notice.
    pub fn close_page(&mut self) {
        self.page_open = false;
        self.add = None;
        self.editor = None;
        self.confirm_delete = None;
        self.notice = None;
    }

    /// Flips the page; closing drops the transient forms and the notice.
    pub fn toggle_page(&mut self) -> bool {
        if self.page_open {
            self.close_page();
        } else {
            self.page_open = true;
        }
        self.page_open
    }

    /// Opens the add form with empty drafts.
    pub fn open_add(&mut self) {
        self.editor = None;
        self.confirm_delete = None;
        self.add = Some(SkillAdd::default());
    }

    /// Opens the edit form for one row; `false` for unknown names.
    pub fn open_edit(&mut self, name: &str) -> bool {
        let Some(row) = self.rows.iter().find(|row| row.name == name) else {
            return false;
        };
        self.add = None;
        self.confirm_delete = None;
        self.editor = Some(SkillEdit {
            name: row.name.clone(),
            path: row.path.clone(),
            name_draft: row.name.clone(),
            description_draft: row.description.clone(),
        });
        true
    }
}

fn skill_row(skill: &SkillMetadata) -> SkillRow {
    SkillRow {
        name: skill.name.clone(),
        description: skill.description.clone(),
        scope: skill.scope,
        path: skill.path.clone().into(),
        enabled: skill.enabled,
        plugin_id: skill.plugin_id.clone(),
    }
}
