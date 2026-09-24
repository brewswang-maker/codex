//! Unit coverage of the porcelain-to-workbench projection.
#![allow(clippy::expect_used)]

use super::GitInfo;
use super::GitStatus;
use super::coarse_status;

#[test]
fn porcelain_codes_collapse_into_coarse_states() {
    assert_eq!(coarse_status("??"), Some(GitStatus::Untracked));
    assert_eq!(coarse_status("M "), Some(GitStatus::Modified));
    assert_eq!(coarse_status(" M"), Some(GitStatus::Modified));
    assert_eq!(coarse_status("MM"), Some(GitStatus::Modified));
    assert_eq!(coarse_status("A "), Some(GitStatus::Added));
    assert_eq!(coarse_status(" D"), Some(GitStatus::Deleted));
    assert_eq!(coarse_status("D "), Some(GitStatus::Deleted));
    // Clean lines (rename source rows and unknowns) render nothing.
    assert_eq!(coarse_status("  "), None);
}

#[test]
fn markers_match_the_reference_shorthand() {
    assert_eq!(GitStatus::Modified.marker(), "M");
    assert_eq!(GitStatus::Added.marker(), "A");
    assert_eq!(GitStatus::Deleted.marker(), "D");
    assert_eq!(GitStatus::Untracked.marker(), "U");
}

#[test]
fn changed_files_projects_the_tree_in_order() {
    let info = GitInfo {
        branch: Some(String::from("main")),
        changes: [
            (String::from("a.txt"), GitStatus::Modified),
            (String::from("b.txt"), GitStatus::Added),
        ]
        .into_iter()
        .collect(),
    };

    assert_eq!(
        info.changed_files(),
        vec![
            (String::from("a.txt"), GitStatus::Modified),
            (String::from("b.txt"), GitStatus::Added),
        ]
    );
}

/// A throwaway repo rooted at a unique temp dir; cleaned on drop.
struct ScratchRepo {
    root: std::path::PathBuf,
}

impl ScratchRepo {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or_else(|_| 0, |d| d.subsec_nanos() as u64);
        let root =
            std::env::temp_dir().join(format!("codex-gui-git-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch repo created");

        let repo = Self { root };
        repo.run(&["init", "-q"]);
        repo.run(&["config", "user.email", "test@example.com"]);
        repo.run(&["config", "user.name", "test"]);
        repo
    }

    fn run(&self, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn log_subjects(&self) -> Vec<String> {
        let output = std::process::Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(&self.root)
            .output()
            .expect("git log runs");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl Drop for ScratchRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn stage_and_commit_commits_only_the_selected_paths() {
    let repo = ScratchRepo::new();
    std::fs::write(repo.root.join("a.txt"), "alpha").expect("write a");
    std::fs::write(repo.root.join("b.txt"), "beta").expect("write b");
    assert!(repo.run(&["add", "."]));
    assert!(repo.run(&["commit", "-q", "-m", "init"]));

    std::fs::write(repo.root.join("a.txt"), "alpha changed").expect("edit a");
    std::fs::write(repo.root.join("b.txt"), "beta changed").expect("edit b");

    // Only a.txt is selected; b.txt must stay uncommitted.
    let result = GitInfo::stage_and_commit(&repo.root, &[String::from("a.txt")], "change a only");
    assert_eq!(result, Ok(()));
    assert_eq!(repo.log_subjects(), vec!["change a only", "init"]);

    // After the commit nothing is staged anymore.
    assert_eq!(GitInfo::staged_diff(&repo.root), Some(String::new()));
}

#[test]
fn staged_diff_shows_what_is_staged() {
    let repo = ScratchRepo::new();
    std::fs::write(repo.root.join("a.txt"), "alpha").expect("write a");
    assert!(repo.run(&["add", "a.txt"]));

    let diff = GitInfo::staged_diff(&repo.root).expect("diff in a repo");
    assert!(diff.contains("+++ b/a.txt"), "diff shows the staged file");
}
