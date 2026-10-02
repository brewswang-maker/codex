//! Turns app-server notifications into WeChat-sized progress lines and the
//! final reply.
//!
//! Streaming deltas are never forwarded (they would flood the chat); instead
//! each completed command/file-change item collapses into one short line,
//! and the finished turn's last agent message — or its error — becomes the
//! reply. Markdown is lightly stripped because WeChat renders plain text.

use codex_app_server_protocol::{ThreadItem, Turn};

/// One-line summary for a completed work item; `None` for items that are
/// noise on WeChat (agent messages, reasoning, user echoes, ...).
pub fn summarize_item(item: &ThreadItem) -> Option<String> {
    match item {
        ThreadItem::CommandExecution {
            command, exit_code, ..
        } => Some(match exit_code {
            Some(0) => format!("✓ {command}"),
            Some(code) => format!("✗ {command}（退出码 {code}）"),
            None => format!("· {command}"),
        }),
        ThreadItem::FileChange { changes, .. } => {
            let paths: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();
            Some(format!("✎ {}", paths.join(", ")))
        }
        ThreadItem::McpToolCall { server, tool, .. } => {
            Some(format!("· {server}:{tool}"))
        }
        _ => None,
    }
}

/// The WeChat reply for a finished turn: the turn error when present,
/// otherwise the last non-empty agent message.
pub fn final_reply(turn: &Turn) -> Option<String> {
    if let Some(error) = &turn.error {
        return Some(format!("任务出错: {}", error.message));
    }
    turn.items.iter().rev().find_map(|item| match item {
        ThreadItem::AgentMessage { text, .. } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    })
}

/// Human description for `turn.duration_ms`.
pub fn describe_duration(duration_ms: Option<i64>) -> String {
    match duration_ms {
        Some(ms) if ms < 1_000 => format!("用时 {ms}ms"),
        Some(ms) if ms < 60_000 => format!("用时 {:.1}s", ms as f64 / 1_000.0),
        Some(ms) => {
            let minutes = ms / 60_000;
            let seconds = (ms % 60_000) / 1_000;
            format!("用时 {minutes}m{seconds:02}s")
        }
        None => String::new(),
    }
}

/// Reply length cap: WeChat renders long bubbles poorly, so a truncated
/// notice is appended instead.
const MAX_REPLY_CHARS: usize = 2_800;

/// Lightweight markdown cleanup for WeChat: drop code fences/headings/emphasis,
/// collapse links to their text, squeeze blank runs, and clamp the length.
pub fn clean_for_wechat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0usize;
    for line in text.lines() {
        let Some(kept) = strip_heading_or_fence(line) else {
            continue;
        };
        let stripped = strip_inline_markup(kept);
        if stripped.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        out.push_str(stripped.trim_end());
        out.push('\n');
    }
    let mut cleaned = out.trim().to_string();
    if cleaned.chars().count() > MAX_REPLY_CHARS {
        cleaned = cleaned.chars().take(MAX_REPLY_CHARS).collect();
        cleaned.push_str("\n\n（内容过长已截断）");
    }
    cleaned
}

/// Line-level markdown pass: `None` means the line is a code-fence marker and
/// is dropped entirely; `Some` is the line with heading markers removed.
fn strip_heading_or_fence(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return None;
    }
    let after_marker = trimmed.trim_start_matches('#');
    if after_marker.len() < trimmed.len() && after_marker.starts_with(' ') {
        return Some(after_marker.trim_start());
    }
    Some(line)
}

/// Removes `[text](url)` / `![alt](url)` and emphasis/code backticks, keeping
/// the inner text. Pure string pass, no regex dependency.
pub fn strip_inline_markup(text: &str) -> String {
    let without_links = strip_links(text);
    let mut out = String::with_capacity(without_links.len());
    let mut chars = without_links.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' | '_' | '`' | '~' => {
                // Swallow runs of emphasis markers (**, __, ~~) entirely.
                while chars.peek() == Some(&ch) {
                    chars.next();
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// `[text](url)` and `![alt](url)` become `text` / `alt`.
fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            out.push('[');
            break;
        };
        let label = &after_open[..close];
        let tail = &after_open[close + 1..];
        if let Some(paren_close) = tail.strip_prefix('(').and_then(|inner| inner.find(')')) {
            // Link: keep the label only.
            out.push_str(label);
            rest = &tail[paren_close + 2..];
        } else {
            // Plain bracket text: keep as-is.
            out.push('[');
            out.push_str(label);
            out.push(']');
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
