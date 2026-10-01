//! The knowledge center: durable project/workspace notes that can be
//! searched and injected into prompts as quoted context.
//!
//! Entries persist in `~/.codex/gui-knowledge.json`. Injection is a
//! scored search over title/tags/body rendered into a bounded markdown
//! fragment the composer prepends to the next turn.

use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

/// One knowledge note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEntry {
    /// Stable unique id.
    pub id: String,
    /// The short heading.
    pub title: String,
    /// The full note body (markdown).
    pub body: String,
    /// Free-form routing tags.
    pub tags: Vec<String>,
    /// What the note belongs to: a project root path or `workspace`.
    pub scope: String,
    /// Unix seconds of creation / last edit.
    pub created_at: i64,
    pub updated_at: i64,
}

/// The persisted knowledge base.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeBase {
    /// Every entry, oldest first.
    pub entries: Vec<KnowledgeEntry>,
}

impl KnowledgeBase {
    /// Loads `~/.codex/gui-knowledge.json`; failures fall back to empty.
    pub fn load_or_default() -> Self {
        let Some(home) = home_dir() else {
            return Self::default();
        };
        let path = home.join(".codex").join("gui-knowledge.json");
        Self::load_from(&path).unwrap_or_default()
    }

    /// Loads from an explicit path (also the test seam).
    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Writes the base back; `false` when the disk write failed.
    pub fn save(&self) -> bool {
        let Some(home) = home_dir() else {
            return false;
        };
        let path = home.join(".codex").join("gui-knowledge.json");
        self.save_to(&path)
    }

    /// Saves to an explicit path (also the test seam).
    pub fn save_to(&self, path: &Path) -> bool {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(raw) => std::fs::write(path, raw).is_ok(),
            Err(_) => false,
        }
    }

    /// Adds an entry and persists; returns its id.
    pub fn add(
        &mut self,
        title: &str,
        body: &str,
        tags: &[String],
        scope: &str,
        now: i64,
    ) -> String {
        let id = format!("kb-{now}-{}", self.entries.len());
        self.entries.push(KnowledgeEntry {
            id: id.clone(),
            title: title.to_string(),
            body: body.to_string(),
            tags: tags.to_vec(),
            scope: scope.to_string(),
            created_at: now,
            updated_at: now,
        });
        self.save();
        id
    }

    /// Edits an entry in place; `true` when the id existed.
    pub fn update(&mut self, id: &str, title: &str, body: &str, tags: &[String], now: i64) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.title = title.to_string();
        entry.body = body.to_string();
        entry.tags = tags.to_vec();
        entry.updated_at = now;
        self.save();
        true
    }

    /// Removes an entry; `true` when something was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        let removed = self.entries.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// Case-insensitive scored search: each whitespace-separated term
    /// scores 3 in the title, 2 in a tag, 1 in the body; ties keep the
    /// newest. Empty queries return nothing (the panel shows all).
    pub fn search(&self, query: &str) -> Vec<&KnowledgeEntry> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|term| term.to_lowercase())
            .collect();
        if terms.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, &KnowledgeEntry)> = self
            .entries
            .iter()
            .map(|entry| (score_entry(entry, &terms), entry))
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.updated_at.cmp(&a.1.updated_at)));
        scored.into_iter().map(|(_, entry)| entry).collect()
    }

    /// Renders the top hits as a quoted-context markdown block within
    /// `char_budget`; `None` when nothing matches or the budget is tiny.
    pub fn inject_fragment(&self, query: &str, char_budget: usize) -> Option<String> {
        let hits = self.search(query);
        if hits.is_empty() || char_budget < 8 {
            return None;
        }
        let mut out = String::from("以下是相关知识库引用，供决策参考：\n");
        for entry in hits {
            let block = format!("\n## {}\n{}\n", entry.title, entry.body.trim_end());
            if out.chars().count() + block.chars().count() > char_budget {
                break;
            }
            out.push_str(&block);
        }
        if out.ends_with("供决策参考：\n") {
            // The very first hit already overflowed; still return the
            // header-less empty result rather than a bare preamble.
            return None;
        }
        Some(out)
    }
}

/// The search score of one entry against lowercased terms.
fn score_entry(entry: &KnowledgeEntry, terms: &[String]) -> usize {
    let title = entry.title.to_lowercase();
    let body = entry.body.to_lowercase();
    let tags: Vec<String> = entry.tags.iter().map(|tag| tag.to_lowercase()).collect();
    terms
        .iter()
        .map(|term| {
            let mut score = 0;
            if title.contains(term) {
                score += 3;
            }
            if tags.iter().any(|tag| tag.contains(term)) {
                score += 2;
            }
            if body.contains(term) {
                score += 1;
            }
            score
        })
        .sum()
}

/// The user's home directory from the environment; POSIX sets `HOME`, and
/// `USERPROFILE` covers Windows.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
