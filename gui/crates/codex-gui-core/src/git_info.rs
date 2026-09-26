//! Git working-tree facts for the workbench surfaces.
//!
//! A single synchronous probe per refresh shells out to the system `git`
//! (no new dependencies): `rev-parse` for the branch, `status --porcelain`
//! for the change list. Outside a repository — or without `git` on PATH —
//! the probe degrades to an empty [`GitInfo`].

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// One modified path plus its coarse state, as the workbench paints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
}

impl GitStatus {
    /// The single-character marker the workbench shows next to the path.
    pub fn marker(self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Untracked => "U",
        }
    }
}

/// One entry of the recent-commit history feeding the source-control
/// graph: abbreviated hash plus the subject line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    /// Abbreviated commit hash (`%h`), e.g. `e797cbb`.
    pub hash: String,
    /// The commit subject (`%s`), verbatim UTF-8.
    pub subject: String,
}

/// The branch and working-tree changes of one repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitInfo {
    /// Current branch name; `None` detached, bare, or outside a repo.
    pub branch: Option<String>,
    /// Changed paths keyed for lookup, ordered for display.
    pub changes: BTreeMap<String, GitStatus>,
    /// Recent commits, newest first; empty outside a repository.
    pub commits: Vec<GitCommit>,
}

impl GitInfo {
    /// Probes the repository rooted at (or containing) `cwd`.
    pub fn probe(cwd: &Path) -> Self {
        Self {
            branch: branch_of(cwd),
            changes: changes_of(cwd),
            commits: recent_commits(cwd),
        }
    }

    /// Whether there is anything to show: no repo, no changes.
    pub fn is_empty(&self) -> bool {
        self.branch.is_none() && self.changes.is_empty()
    }

    /// Number of changed paths.
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// The changed files as an ordered (path, status) list, in the same
    /// order the sidebar paints them.
    pub fn changed_files(&self) -> Vec<(String, GitStatus)> {
        self.changes
            .iter()
            .map(|(path, status)| (path.clone(), *status))
            .collect()
    }

    /// Stages the given paths (`git add --`); the stderr comes back on
    /// failure.
    pub fn stage(root: &Path, paths: &[String]) -> Result<(), String> {
        let add = Command::new("git")
            .arg("add")
            .arg("--")
            .args(paths)
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if !add.status.success() {
            return Err(String::from_utf8_lossy(&add.stderr).trim().to_string());
        }
        Ok(())
    }

    /// Stages the given paths and commits them with `message`; the
    /// combined git stderr comes back on failure.
    pub fn stage_and_commit(root: &Path, paths: &[String], message: &str) -> Result<(), String> {
        Self::stage(root, paths)?;

        let commit = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(root)
            .output()
            .map_err(|error| error.to_string())?;
        if !commit.status.success() {
            return Err(String::from_utf8_lossy(&commit.stderr).trim().to_string());
        }
        Ok(())
    }

    /// The staged unified diff (`git diff --cached`), or `None` outside a
    /// repository or without `git` on PATH.
    pub fn staged_diff(root: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["diff", "--cached"])
            .current_dir(root)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Shells out to `git` inside `cwd`. Forcing `core.quotepath=false` is
/// what keeps non-ASCII paths readable: the git default re-encodes them
/// as quoted octal escapes (`"\346\226\207"`) on `status --porcelain`.
fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.quotepath=false")
        .current_dir(cwd);
    command
}

/// `git rev-parse --abbrev-ref HEAD` inside `cwd`, if that is a repo.
fn branch_of(cwd: &Path) -> Option<String> {
    let output = git_command(cwd)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|branch| !branch.is_empty())
}

/// The newest commits as `(hash, subject)` rows, newest first.
fn recent_commits(cwd: &Path) -> Vec<GitCommit> {
    let Ok(output) = git_command(cwd)
        .args(["log", "-n", "20", "--pretty=format:%h\u{1f}%s"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\u{1f}'))
        .map(|(hash, subject)| GitCommit {
            hash: hash.to_string(),
            subject: subject.to_string(),
        })
        .collect()
}

/// Parses `git status --porcelain` inside `cwd` into coarse per-path states.
fn changes_of(cwd: &Path) -> BTreeMap<String, GitStatus> {
    let Ok(output) = git_command(cwd).args(["status", "--porcelain"]).output() else {
        return BTreeMap::new();
    };
    if !output.status.success() {
        return BTreeMap::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut changes = BTreeMap::new();
    for line in stdout.lines() {
        // Porcelain v1: two status columns, a space, then the path.
        if line.len() < 4 {
            continue;
        }
        let (codes, path) = line.split_at(2);
        let path = decode_path(path.trim_start());
        if path.is_empty() {
            continue;
        }
        let status = coarse_status(codes);
        if let Some(status) = status {
            changes.insert(path, status);
        }
    }
    changes
}

/// Turns one porcelain path field into a display path. Rename rows carry
/// `old -> new` and keep the destination; quoted paths hold C-style
/// escapes (the `core.quotepath` fallback for special characters) that
/// decode back into raw UTF-8.
fn decode_path(field: &str) -> String {
    // Rename rows split first, then each half sheds its quotes.
    let field = field.rsplit_once(" -> ").map_or(field, |(_, new)| new);
    let field = field.strip_prefix('"').unwrap_or(field);
    let field = field.strip_suffix('"').unwrap_or(field);
    unquote_escapes(field)
}

/// Decodes the C-style escapes git uses inside quoted paths: octal
/// `\ooo` byte sequences plus the single-character escapes. Bytes are
/// reassembled losslessly, so multi-byte UTF-8 survives intact.
fn unquote_escapes(path: &str) -> String {
    if !path.contains('\\') {
        return path.to_string();
    }

    let mut bytes = Vec::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            bytes.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('r') => bytes.push(b'\r'),
            Some(escape @ ('a' | 'b' | 'f' | 'v' | '\\' | '"')) => {
                bytes.push(match escape {
                    'a' => 0x07,
                    'b' => 0x08,
                    'f' => 0x0c,
                    'v' => 0x0b,
                    '\\' => b'\\',
                    _ => b'"',
                });
            }
            Some(first @ ('0'..='7')) => {
                let mut value = first.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.clone().next() {
                        Some(digit @ ('0'..='7')) => {
                            value = value * 8 + digit.to_digit(8).unwrap_or(0);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => {
                bytes.extend_from_slice(other.encode_utf8(&mut [0; 4]).as_bytes());
            }
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Collapses the two porcelain columns into the coarse display state.
fn coarse_status(codes: &str) -> Option<GitStatus> {
    let staged = codes.as_bytes().first().copied().unwrap_or(b' ');
    let unstaged = codes.as_bytes().get(1).copied().unwrap_or(b' ');

    if codes.starts_with("??") {
        return Some(GitStatus::Untracked);
    }
    if staged == b'D' || unstaged == b'D' {
        return Some(GitStatus::Deleted);
    }
    if staged == b'A' {
        return Some(GitStatus::Added);
    }
    if staged != b' ' || unstaged != b' ' {
        return Some(GitStatus::Modified);
    }
    None
}

#[cfg(test)]
#[path = "git_info_tests.rs"]
mod tests;
