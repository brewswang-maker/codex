//! Transcript full-text search, borrowed from ZCode's ModelTrajectory pane.
//!
//! The search walks the transcript entries once and returns at most one hit
//! per entry: the first entry whose text fields contain the query (ASCII
//! case-insensitive). Each hit carries a display snippet with the match
//! position inside it, so the chat pane can render a filtered view where the
//! user steps between matching entries without any scroll-to-widget support.

use crate::transcript::Entry;

/// Which text field of an entry matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchField {
    /// The user message body.
    UserText,
    /// The assistant message body.
    AgentText,
    /// The command line of an execution card.
    Command,
    /// The captured output of an execution card.
    Output,
    /// The changed-file paths of a file-change card.
    FilePath,
    /// The visible reasoning body of a reasoning card.
    ReasoningText,
    /// The `server/tool` label of an MCP call card.
    McpTool,
    /// The text of a system note divider.
    NoticeText,
}

/// One match: which entry matched, in which field, shown as a snippet with
/// the byte offset of the match inside the snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Position of the matching entry in the transcript.
    pub entry_index: usize,
    /// Wire id of the matching entry.
    pub entry_id: String,
    /// The field that contained the match.
    pub field: SearchField,
    /// A short excerpt around the match.
    pub snippet: String,
    /// Byte offset of the match inside `snippet`.
    pub match_start: usize,
}

/// Characters of context shown before the match inside a snippet.
const SNIPPET_BEFORE: usize = 40;
/// Characters of context shown after the match inside a snippet.
const SNIPPET_AFTER: usize = 80;

/// Finds the first matching entry per transcript row, in order.
///
/// An empty (or blank) query yields no hits, which the chat pane reads as
/// "show everything".
pub fn search(entries: &[Entry], query: &str) -> Vec<SearchHit> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    entries
        .iter()
        .enumerate()
        .filter_map(|(entry_index, entry)| first_hit(entry_index, entry, &needle))
        .collect()
}

/// Returns the first field hit of `needle` in `entry`, if any.
fn first_hit(entry_index: usize, entry: &Entry, needle: &str) -> Option<SearchHit> {
    let (entry_id, fields): (&str, Vec<(SearchField, String)>) = match entry {
        Entry::UserMessage { id, text, .. } => (id, vec![(SearchField::UserText, text.clone())]),
        Entry::AgentMessage { id, text } => (id, vec![(SearchField::AgentText, text.clone())]),
        Entry::CommandExecution {
            id,
            command,
            output,
            ..
        } => (
            id,
            vec![
                (SearchField::Command, command.clone()),
                (SearchField::Output, output.clone()),
            ],
        ),
        Entry::FileChange { id, changes } => {
            let paths = changes
                .iter()
                .map(|record| record.path.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (id, vec![(SearchField::FilePath, paths)])
        }
        Entry::Reasoning { id, .. } => (
            id,
            vec![(SearchField::ReasoningText, entry.reasoning_text())],
        ),
        Entry::McpToolCall {
            id, server, tool, ..
        } => (id, vec![(SearchField::McpTool, format!("{server}/{tool}"))]),
        Entry::SystemNote { id, text } => (id, vec![(SearchField::NoticeText, text.clone())]),
    };

    fields
        .into_iter()
        .filter_map(|(field, text)| find_snippet(field, &text, needle))
        .next()
        .map(|(field, snippet, match_start)| SearchHit {
            entry_index,
            entry_id: String::from(entry_id),
            field,
            snippet,
            match_start,
        })
}

/// Locates `needle` in `text` (both already lowercase where it matters) and
/// cuts a char-boundary-safe snippet around the match.
fn find_snippet(
    field: SearchField,
    text: &str,
    needle: &str,
) -> Option<(SearchField, String, usize)> {
    let haystack = text.to_lowercase();
    let byte_pos = haystack.find(needle)?;
    let start = char_boundary_snippet(text, byte_pos);
    let end = text[start..]
        .char_indices()
        .nth(SNIPPET_BEFORE + SNIPPET_AFTER)
        .map_or(text.len(), |(idx, _)| start + idx);

    let mut prefix = 0;
    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
        prefix = snippet.len();
    }
    snippet.push_str(text.get(start..end).unwrap_or_default());
    if end < text.len() {
        snippet.push('…');
    }
    // The match offset is relative to the snippet, past the ellipsis.
    Some((field, snippet, byte_pos - start + prefix))
}

/// Backs the raw byte position up to a char boundary at most
/// [`SNIPPET_BEFORE`] chars earlier, returning the snippet start.
fn char_boundary_snippet(text: &str, byte_pos: usize) -> usize {
    text[..byte_pos]
        .char_indices()
        .rev()
        .nth(SNIPPET_BEFORE - 1)
        .map_or(0, |(idx, _)| idx)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
