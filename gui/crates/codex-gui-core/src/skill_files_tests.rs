//! Headless coverage for the on-disk skill management.
#![allow(clippy::expect_used)]

use crate::skill_files::LinkImportReport;
use crate::skill_files::MAX_LINK_SKILLS;
use crate::skill_files::SkillRepoLink;
use crate::skill_files::create_skill;
use crate::skill_files::delete_skill;
use crate::skill_files::import_skill_directory;
use crate::skill_files::import_skill_file;
use crate::skill_files::import_skill_from_link;
use crate::skill_files::parse_skill_document;
use crate::skill_files::parse_skill_link;
use crate::skill_files::render_skill_document;
use crate::skill_files::select_skill_dirs;
use crate::skill_files::update_skill;
use crate::skill_files::validate_skill_name;
use pretty_assertions::assert_eq;
use std::path::Path;

fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: The {name} skill.\n---\n\n# {name}\n")
}

#[test]
fn parse_reads_frontmatter_scalars_and_keeps_the_body() {
    let document = parse_skill_document(
        "---\nname: review\ndescription: 'Review: the current diff'\nmetadata:\n  short-description: nope\n---\n\n# Body\n",
    )
    .expect("document parses");

    assert_eq!(
        document,
        crate::skill_files::SkillDocument {
            name: "review".to_string(),
            description: "Review: the current diff".to_string(),
            body: "\n# Body".to_string(),
        }
    );
}

#[test]
fn parse_rejects_files_without_a_frontmatter_block() {
    assert!(
        parse_skill_document("# Just a body\n").is_err(),
        "a missing block is rejected"
    );
    assert!(
        parse_skill_document("---\nname: x\n").is_err(),
        "an unclosed block is rejected"
    );
}

#[test]
fn render_then_parse_round_trips_quoted_values() {
    let rendered = render_skill_document(
        "review",
        "Review: the current diff ('strict')",
        "# Review\n",
    );
    assert_eq!(
        rendered,
        "---\nname: review\ndescription: 'Review: the current diff (''strict'')'\n---\n\n# Review\n"
    );

    let document = parse_skill_document(&rendered).expect("rendered text parses");
    assert_eq!(document.name, "review");
    assert_eq!(document.description, "Review: the current diff ('strict')");
}

#[test]
fn validate_rejects_unsafe_names() {
    assert!(validate_skill_name("").is_err());
    assert!(validate_skill_name("..").is_err());
    assert!(validate_skill_name("a/b").is_err());
    assert!(validate_skill_name("a\\b").is_err());
    assert!(validate_skill_name(&"x".repeat(65)).is_err());
    assert!(validate_skill_name(" spaced").is_err());
    assert!(validate_skill_name("fine-name_1").is_ok());
    assert!(validate_skill_name(&"x".repeat(64)).is_ok());
}

#[test]
fn create_writes_a_skill_and_refuses_duplicates() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = create_skill(root.path(), "review", "review the diff").expect("skill created");
    assert_eq!(path, root.path().join("review").join("SKILL.md"));

    let document = parse_skill_document(&std::fs::read_to_string(&path).expect("file reads"))
        .expect("document parses");
    assert_eq!(document.name, "review");
    assert_eq!(document.description, "review the diff");

    let duplicate = create_skill(root.path(), "review", "again");
    assert!(duplicate.is_err(), "the duplicate is rejected");
    assert!(
        create_skill(root.path(), "empty-desc", "  ").is_err(),
        "an empty description is rejected"
    );
}

#[test]
fn import_directory_copies_the_tree_under_the_frontmatter_name() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = tempfile::tempdir().expect("source dir");
    let skill_dir = source.path().join("my-skill");
    std::fs::create_dir_all(skill_dir.join("scripts")).expect("nested dir");
    std::fs::write(skill_dir.join("SKILL.md"), skill_md("renamed-skill")).expect("fixture");
    std::fs::write(skill_dir.join("scripts/run.sh"), "echo hi\n").expect("fixture");

    let path = import_skill_directory(root.path(), &skill_dir).expect("import works");
    assert_eq!(path, root.path().join("renamed-skill").join("SKILL.md"));
    assert!(
        root.path().join("renamed-skill/scripts/run.sh").is_file(),
        "the nested tree is copied"
    );

    assert!(
        import_skill_directory(root.path(), &skill_dir).is_err(),
        "the duplicate import is rejected"
    );
}

#[test]
fn import_directory_falls_back_to_the_source_directory_name() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = tempfile::tempdir().expect("source dir");
    let skill_dir = source.path().join("plain-skill");
    std::fs::create_dir_all(&skill_dir).expect("skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\ndescription: no name here\n---\n\n# Body\n",
    )
    .expect("fixture");

    let path = import_skill_directory(root.path(), &skill_dir).expect("import works");
    assert_eq!(path, root.path().join("plain-skill").join("SKILL.md"));
}

#[test]
fn import_file_uses_the_frontmatter_name_then_the_stem() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = tempfile::tempdir().expect("source dir");
    let named = source.path().join("whatever.md");
    std::fs::write(&named, skill_md("from-frontmatter")).expect("fixture");
    let anonym = source.path().join("stem-name.md");
    std::fs::write(&anonym, "---\ndescription: unnamed skill\n---\n\n# Body\n").expect("fixture");

    let path = import_skill_file(root.path(), &named).expect("import works");
    assert_eq!(path, root.path().join("from-frontmatter").join("SKILL.md"));
    let path = import_skill_file(root.path(), &anonym).expect("import works");
    assert_eq!(path, root.path().join("stem-name").join("SKILL.md"));

    assert!(
        import_skill_file(root.path(), &named).is_err(),
        "the duplicate import is rejected"
    );
}

#[test]
fn update_rewrites_only_the_two_scalars() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = create_skill(root.path(), "review", "old description").expect("skill created");
    let original = std::fs::read_to_string(&path).expect("file reads");
    let annotated = original.replace(
        "---\n\n",
        "metadata:\n  short-description: keep me\n---\n\n",
    );
    std::fs::write(&path, &annotated).expect("fixture");

    update_skill(&path, "renamed", "New: description").expect("update works");

    let updated = std::fs::read_to_string(&path).expect("file reads");
    assert!(updated.contains("name: renamed"), "the name is rewritten");
    assert!(
        updated.contains("description: 'New: description'"),
        "the description is rewritten and quoted"
    );
    assert!(
        updated.contains("metadata:\n  short-description: keep me"),
        "other frontmatter entries survive"
    );
    let document = parse_skill_document(&updated).expect("updated text parses");
    assert_eq!(document.name, "renamed");
    assert_eq!(document.description, "New: description");
}

#[test]
fn update_appends_missing_scalars_inside_the_block() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("SKILL.md");
    std::fs::write(&path, "---\nmetadata:\n  short-description: x\n---\nbody\n").expect("fixture");

    update_skill(&path, "filled", "description added").expect("update works");

    let updated = std::fs::read_to_string(&path).expect("file reads");
    let document = parse_skill_document(&updated).expect("updated text parses");
    assert_eq!(document.name, "filled");
    assert_eq!(document.description, "description added");
    assert!(document.body.contains("body"), "the body survives");
}

#[test]
fn update_replaces_block_scalar_continuations() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("SKILL.md");
    std::fs::write(
        &path,
        "---\nname: old\ndescription: |\n  line one\n  line two\n---\nbody\n",
    )
    .expect("fixture");

    update_skill(&path, "new", "one line").expect("update works");

    let updated = std::fs::read_to_string(&path).expect("file reads");
    assert!(!updated.contains("line one"), "the block lines are gone");
    let document = parse_skill_document(&updated).expect("updated text parses");
    assert_eq!(document.description, "one line");
}

#[test]
fn delete_removes_the_skill_directory_only() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = create_skill(root.path(), "doomed", "to be removed").expect("skill created");

    delete_skill(&path).expect("delete works");

    assert!(
        !root.path().join("doomed").exists(),
        "the directory is gone"
    );
    assert!(root.path().exists(), "the root survives");
}

#[test]
fn delete_refuses_unsafe_paths() {
    let root = tempfile::tempdir().expect("tempdir");
    let skills_root = root.path().join("skills");
    std::fs::create_dir_all(&skills_root).expect("skills root");
    let root_file: &Path = &skills_root.join("SKILL.md");
    std::fs::write(root_file, skill_md("nope")).expect("fixture");

    assert!(
        delete_skill(root_file).is_err(),
        "the skills root itself is protected"
    );
    let missing = root.path().join("gone").join("SKILL.md");
    assert!(
        delete_skill(&missing).is_err(),
        "missing files are rejected"
    );
    let wrong_name = root.path().join("other.md");
    std::fs::write(&wrong_name, "x").expect("fixture");
    assert!(
        delete_skill(&wrong_name).is_err(),
        "only SKILL.md paths are accepted"
    );
}

#[test]
fn parse_skill_link_reads_github_repos_and_tree_urls() {
    let cases = [
        (
            "https://github.com/openai/skills",
            SkillRepoLink {
                repo_url: "https://github.com/openai/skills".to_string(),
                reference: None,
                subpath: None,
            },
        ),
        (
            "https://github.com/openai/skills.git/",
            SkillRepoLink {
                repo_url: "https://github.com/openai/skills".to_string(),
                reference: None,
                subpath: None,
            },
        ),
        (
            "https://github.com/openai/skills/tree/main",
            SkillRepoLink {
                repo_url: "https://github.com/openai/skills".to_string(),
                reference: Some("main".to_string()),
                subpath: None,
            },
        ),
        (
            "https://github.com/openai/skills/tree/main/skills/.curated/babysit-pr",
            SkillRepoLink {
                repo_url: "https://github.com/openai/skills".to_string(),
                reference: Some("main".to_string()),
                subpath: Some("skills/.curated/babysit-pr".to_string()),
            },
        ),
        (
            "https://github.com/obra/superpowers/skills/brainstorming",
            SkillRepoLink {
                repo_url: "https://github.com/obra/superpowers".to_string(),
                reference: None,
                subpath: Some("skills/brainstorming".to_string()),
            },
        ),
    ];

    for (link, expected) in cases {
        assert_eq!(
            parse_skill_link(link).expect("link parses"),
            expected,
            "for {link}"
        );
    }
}

#[test]
fn parse_skill_link_points_blob_urls_at_the_skill_directory() {
    assert_eq!(
        parse_skill_link(
            "https://github.com/obra/superpowers/blob/main/skills/brainstorming/SKILL.md"
        )
        .expect("blob link parses"),
        SkillRepoLink {
            repo_url: "https://github.com/obra/superpowers".to_string(),
            reference: Some("main".to_string()),
            subpath: Some("skills/brainstorming".to_string()),
        }
    );
    assert_eq!(
        parse_skill_link("https://github.com/obra/superpowers/blob/main/SKILL.md")
            .expect("a root blob keeps the whole repo"),
        SkillRepoLink {
            repo_url: "https://github.com/obra/superpowers".to_string(),
            reference: Some("main".to_string()),
            subpath: None,
        }
    );
    assert!(
        parse_skill_link("https://github.com/obra/superpowers/blob/main/README.md").is_err(),
        "blob links must point at SKILL.md"
    );
}

#[test]
fn parse_skill_link_passes_other_git_urls_through_and_rejects_the_rest() {
    assert_eq!(
        parse_skill_link("git@github.com:obra/superpowers.git").expect("ssh links pass through"),
        SkillRepoLink {
            repo_url: "git@github.com:obra/superpowers.git".to_string(),
            reference: None,
            subpath: None,
        }
    );
    assert!(
        parse_skill_link("file:///tmp/repo").is_ok(),
        "file URLs are cloneable"
    );

    for rejected in ["", "   ", "openai/skills", "has space"] {
        assert!(
            parse_skill_link(rejected).is_err(),
            "{rejected:?} is not a repository link"
        );
    }
}

#[test]
fn import_skill_from_link_clones_a_local_repo() {
    let repo = tempfile::tempdir().expect("repo dir");
    let skill = repo.path().join("skills").join("demo");
    std::fs::create_dir_all(&skill).expect("skill dir");
    std::fs::write(skill.join("SKILL.md"), skill_md("demo-skill")).expect("fixture");
    commit_repo(repo.path());

    let root = tempfile::tempdir().expect("skills root");
    let link = format!("file://{}", repo.path().display());
    let report = import_skill_from_link(root.path(), &link).expect("import succeeds");

    assert_eq!(
        report,
        LinkImportReport {
            imported: vec![root.path().join("demo-skill").join("SKILL.md")],
            failed: Vec::new(),
        }
    );
    assert!(
        root.path().join("demo-skill").join("SKILL.md").is_file(),
        "the skill lands under the skills root"
    );
}

#[test]
fn import_skill_from_link_keeps_the_checkout_metadata_out() {
    let repo = tempfile::tempdir().expect("repo dir");
    std::fs::write(repo.path().join("SKILL.md"), skill_md("root-skill")).expect("fixture");
    commit_repo(repo.path());

    let root = tempfile::tempdir().expect("skills root");
    let link = format!("file://{}", repo.path().display());
    let report = import_skill_from_link(root.path(), &link).expect("import succeeds");

    assert_eq!(
        report,
        LinkImportReport {
            imported: vec![root.path().join("root-skill").join("SKILL.md")],
            failed: Vec::new(),
        }
    );
    assert!(
        !root.path().join("root-skill").join(".git").exists(),
        "the clone metadata is stripped"
    );
}

#[test]
fn import_skill_from_link_imports_every_nested_skill() {
    let repo = tempfile::tempdir().expect("repo dir");
    for name in ["alpha", "beta"] {
        let skill = repo.path().join("skills").join(name);
        std::fs::create_dir_all(&skill).expect("skill dir");
        std::fs::write(skill.join("SKILL.md"), skill_md(name)).expect("fixture");
    }
    commit_repo(repo.path());

    let root = tempfile::tempdir().expect("skills root");
    let link = format!("file://{}", repo.path().display());
    let report = import_skill_from_link(root.path(), &link).expect("nested repos import");

    assert_eq!(
        report,
        LinkImportReport {
            imported: vec![
                root.path().join("alpha").join("SKILL.md"),
                root.path().join("beta").join("SKILL.md"),
            ],
            failed: Vec::new(),
        }
    );
}

#[cfg(unix)]
#[test]
fn import_skill_from_link_keeps_going_when_one_skill_fails() {
    // A batch where one skill is broken (it smuggles a symbolic link) still
    // installs its siblings and reports the loss.
    let repo = tempfile::tempdir().expect("repo dir");
    let good = repo.path().join("skills").join("good");
    std::fs::create_dir_all(&good).expect("good dir");
    std::fs::write(good.join("SKILL.md"), skill_md("good")).expect("fixture");
    let leaky = repo.path().join("skills").join("leaky");
    std::fs::create_dir_all(&leaky).expect("leaky dir");
    std::fs::write(leaky.join("SKILL.md"), skill_md("leaky")).expect("fixture");
    std::os::unix::fs::symlink("elsewhere", leaky.join("escape")).expect("symlink fixture");
    commit_repo(repo.path());

    let root = tempfile::tempdir().expect("skills root");
    let link = format!("file://{}", repo.path().display());
    let report = import_skill_from_link(root.path(), &link).expect("the batch imports");

    assert_eq!(
        report.imported,
        vec![root.path().join("good").join("SKILL.md")],
        "the healthy skill lands"
    );
    assert_eq!(report.failed.len(), 1, "got {:?}", report.failed);
    assert!(
        report.failed[0].contains("符号链接"),
        "the symlink is named, got {:?}",
        report.failed
    );
}

#[test]
fn select_skill_dirs_imports_a_named_subtree_or_its_single_skill() {
    let checkout = tempfile::tempdir().expect("checkout");
    for name in ["alpha", "beta"] {
        let skill = checkout.path().join("skills").join(name);
        std::fs::create_dir_all(&skill).expect("skill dir");
        std::fs::write(skill.join("SKILL.md"), skill_md(name)).expect("fixture");
    }
    let subtree = SkillRepoLink {
        repo_url: "https://example.com/repo".to_string(),
        reference: None,
        subpath: Some("skills".to_string()),
    };

    assert_eq!(
        select_skill_dirs(&subtree, checkout.path()).expect("the subtree selects"),
        vec![
            checkout.path().join("skills").join("alpha"),
            checkout.path().join("skills").join("beta"),
        ]
    );

    let single = SkillRepoLink {
        subpath: Some("skills/alpha".to_string()),
        ..subtree.clone()
    };
    assert_eq!(
        select_skill_dirs(&single, checkout.path()).expect("the skill selects"),
        vec![checkout.path().join("skills").join("alpha")]
    );

    let missing = SkillRepoLink {
        subpath: Some("nope".to_string()),
        ..subtree
    };
    let error = select_skill_dirs(&missing, checkout.path()).expect_err("missing subtrees fail");
    assert!(error.contains("不存在目录 nope"), "got {error}");
}

#[test]
fn select_skill_dirs_caps_oversized_batches() {
    let checkout = tempfile::tempdir().expect("checkout");
    for index in 0..MAX_LINK_SKILLS + 1 {
        let name = format!("skill-{index:03}");
        let skill = checkout.path().join(&name);
        std::fs::create_dir_all(&skill).expect("skill dir");
        std::fs::write(skill.join("SKILL.md"), skill_md(&name)).expect("fixture");
    }
    let parsed = SkillRepoLink {
        repo_url: "https://example.com/repo".to_string(),
        reference: None,
        subpath: None,
    };

    let error = select_skill_dirs(&parsed, checkout.path()).expect_err("oversized batches fail");

    assert!(
        error.contains("超过单次导入上限"),
        "the cap is reported, got {error}"
    );
}

#[test]
fn import_skill_from_link_requires_a_skill_in_the_repo() {
    let repo = tempfile::tempdir().expect("repo dir");
    std::fs::write(repo.path().join("README.md"), "no skill here").expect("fixture");
    commit_repo(repo.path());

    let root = tempfile::tempdir().expect("skills root");
    let link = format!("file://{}", repo.path().display());
    let error = import_skill_from_link(root.path(), &link).expect_err("empty repos fail");

    assert!(
        error.contains("未找到 SKILL.md"),
        "the missing skill is reported, got {error}"
    );
}

/// Initializes `dir` as a Git repository and commits its current tree.
fn commit_repo(dir: &Path) {
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "init"]);
}
