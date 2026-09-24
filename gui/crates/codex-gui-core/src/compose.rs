//! Assembles the `turn/start` input list from the composer buffer.
//!
//! Pure and iced-free so the wire shape is unit-testable headlessly: the
//! pending image attachments ride as `LocalImage` items, the composer text
//! follows, and every enabled `$skill` mentioned in the text is appended as
//! a `Skill` item so the backend links the invocation.

use crate::skills::SkillRow;
use crate::skills::SkillsBoard;
use codex_app_server_protocol::UserInput;
use std::path::PathBuf;

/// Composes the `turn/start` input from pending image attachments, the
/// composer text, and any `$skill` mentions in the text.
pub fn compose_inputs(text: &str, images: &[PathBuf], skills: &SkillsBoard) -> Vec<UserInput> {
    let mut inputs: Vec<UserInput> = images
        .iter()
        .map(|path| UserInput::LocalImage {
            detail: None,
            path: path.clone(),
        })
        .collect();
    inputs.push(UserInput::Text {
        text: text.to_string(),
        text_elements: Vec::new(),
    });
    inputs.extend(
        mentioned_skills(text, skills)
            .into_iter()
            .map(|row| UserInput::Skill {
                name: row.name.clone(),
                path: row.path.clone(),
            }),
    );
    inputs
}

/// Enabled skills whose `$name` appears in the text, in first-mention order
/// and deduplicated.
fn mentioned_skills<'a>(text: &str, skills: &'a SkillsBoard) -> Vec<&'a SkillRow> {
    let mut mentioned: Vec<&SkillRow> = Vec::new();
    for name in mention_names(text) {
        let known = mentioned.iter().any(|row| row.name == name);
        if known {
            continue;
        }
        if let Some(row) = skills
            .rows
            .iter()
            .find(|row| row.enabled && row.name == name)
        {
            mentioned.push(row);
        }
    }
    mentioned
}

/// The `$name` tokens in the text: `$` followed by skill-name characters.
fn mention_names(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for (index, segment) in text.split('$').enumerate() {
        // Everything before the first `$` cannot start a mention.
        if index == 0 {
            continue;
        }
        let end = segment
            .char_indices()
            .find(|(_, ch)| !is_name_char(*ch))
            .map(|(offset, _)| offset)
            .unwrap_or(segment.len());
        if end > 0 {
            names.push(&segment[..end]);
        }
    }
    names
}

fn is_name_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '-' || ch == '_'
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
