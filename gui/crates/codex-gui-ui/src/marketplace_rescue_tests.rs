use std::fs;
use std::path::Path;

use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::generated_manifest;
use super::is_manifest_missing_error;
use super::rescue_into;
use super::sanitize_name;
use super::staged_root_from_error;

fn staging_dir(base: &Path, suffix: &str) -> std::path::PathBuf {
    let staged = base
        .join(".staging")
        .join(format!("marketplace-add-{suffix}"));
    fs::create_dir_all(&staged).expect("staging dir");
    staged
}

fn error_text_for(staged: &Path) -> String {
    format!(
        "request failed (-32600): invalid marketplace file '{}': \
marketplace root does not contain a supported manifest",
        staged.display()
    )
}

fn write_skill(root: &Path, skill: &str) {
    let dir = root.join("skills").join(skill);
    fs::create_dir_all(&dir).expect("skill dir");
    fs::write(dir.join("SKILL.md"), "---\nname: x\ndescription: y\n---\n").expect("skill doc");
}

#[test]
fn manifest_missing_marker_matches_only_that_error() {
    assert!(is_manifest_missing_error(
        "invalid marketplace file '/x': marketplace root does not contain a supported manifest"
    ));
    assert!(!is_manifest_missing_error("marketplace already added"));
    assert!(!is_manifest_missing_error(""));
}

#[test]
fn staged_root_is_parsed_from_the_quoted_error_path() {
    let base = TempDir::new().expect("base");
    let staged = staging_dir(base.path(), "jGC5QU");
    let parsed = staged_root_from_error(&error_text_for(&staged)).expect("parsed");
    assert_eq!(parsed, staged);
}

#[test]
fn staged_root_rejects_paths_outside_the_staging_area() {
    let base = TempDir::new().expect("base");
    let plain = base.path().join("repo");
    fs::create_dir_all(&plain).expect("repo");
    assert_eq!(staged_root_from_error(&error_text_for(&plain)), None);
    assert_eq!(staged_root_from_error("no quoted path here"), None);
}

#[test]
fn repo_layout_yields_a_single_root_plugin() {
    let base = TempDir::new().expect("base");
    let staged = staging_dir(base.path(), "repo");
    write_skill(&staged, "life-decision-guide");

    let manifest = generated_manifest(&staged).expect("manifest");
    assert_eq!(manifest["plugins"].as_array().expect("plugins").len(), 1);
    assert_eq!(manifest["plugins"][0]["source"]["path"], "./");
    assert!(!manifest["name"].as_str().expect("name").is_empty());
}

#[test]
fn top_level_dirs_become_one_plugin_each() {
    let base = TempDir::new().expect("base");
    let staged = staging_dir(base.path(), "multi");
    for (dir, skill) in [("docs", "guide"), ("toolkit", "helper")] {
        let skills = staged.join(dir).join("skills").join(skill);
        fs::create_dir_all(&skills).expect("skill dir");
        fs::write(skills.join("SKILL.md"), "x").expect("skill doc");
    }

    let manifest = generated_manifest(&staged).expect("manifest");
    let plugins = manifest["plugins"].as_array().expect("plugins");
    assert_eq!(plugins.len(), 2);
    assert_eq!(plugins[0]["source"]["path"], "docs/");
    assert_eq!(plugins[1]["source"]["path"], "toolkit/");
}

#[test]
fn repositories_without_skill_layout_are_rejected() {
    let base = TempDir::new().expect("base");
    let staged = staging_dir(base.path(), "empty");
    assert!(generated_manifest(&staged).is_err());
}

#[test]
fn rescue_generates_the_manifest_and_persists_the_clone() {
    let staging_base = TempDir::new().expect("staging base");
    let staged = staging_dir(staging_base.path(), "AbC123");
    write_skill(&staged, "life-decision-guide");
    fs::write(staged.join("README.md"), "guide").expect("readme");

    let target_base = TempDir::new().expect("target base");
    let installed = rescue_into(&error_text_for(&staged), target_base.path()).expect("rescued");

    let installed = std::path::PathBuf::from(installed);
    assert!(installed.starts_with(target_base.path()));
    assert!(installed.join(".agents/plugins/marketplace.json").is_file());
    assert!(
        installed
            .join("skills/life-decision-guide/SKILL.md")
            .is_file()
    );
    assert!(!staged.exists(), "staging clone should have been moved");

    let manifest =
        fs::read_to_string(installed.join(".agents/plugins/marketplace.json")).expect("manifest");
    // Without git metadata the plugin falls back to the generic name and
    // the repo-as-plugin entry points at the marketplace root itself.
    assert!(manifest.contains("\"imported-skills\""));
    assert!(manifest.contains("\"path\": \"./\""));
}

#[test]
fn rescue_refuses_a_directory_that_already_has_a_manifest() {
    let staging_base = TempDir::new().expect("staging base");
    let staged = staging_dir(staging_base.path(), "hasManifest");
    fs::create_dir_all(staged.join(".agents/plugins")).expect("manifest dir");
    fs::write(staged.join(".agents/plugins/marketplace.json"), "{}").expect("manifest");

    let target_base = TempDir::new().expect("target base");
    let error = rescue_into(&error_text_for(&staged), target_base.path()).expect_err("must refuse");
    assert!(error.contains("已包含市场清单"));
}

#[test]
fn sanitize_name_collapses_to_lowercase_dashes() {
    assert_eq!(sanitize_name("HowToLiveBetter"), "howtolivebetter");
    assert_eq!(sanitize_name("  A B--c! "), "a-b-c");
    assert_eq!(sanitize_name("中文-only"), "only");
    assert_eq!(sanitize_name("///"), "imported-skills");
}
