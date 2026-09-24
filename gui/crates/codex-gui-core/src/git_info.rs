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

/// The branch and working-tree changes of one repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitInfo {
    /// Current branch name; `None` detached, bare, or outside a repo.
    pub branch: Option<String>,
    /// Changed paths keyed for lookup, ordered for display.
    pub changes: BTreeMap<String, GitStatus>,
}

impl GitInfo {
    /// Probes the repository rooted at (or containing) `cwd`.
    pub fn probe(cwd: &Path) -> Self {
        Self {
            branch: branch_of(cwd),
            changes: changes_of(cwd),
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

/// `git rev-parse --abbrev-ref HEAD` inside `cwd`, if that is a repo.
fn branch_of(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|branch| !branch.is_empty())
}

/// Parses `git status --porcelain` inside `cwd` into coarse per-path states.
fn changes_of(cwd: &Path) -> BTreeMap<String, GitStatus> {
    let Ok(output) = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    else {
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
        let path = path.trim_start().to_string();
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
