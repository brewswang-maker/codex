use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use serde_json::json;

/// Manifest locations the upstream marketplace loader accepts.
const SUPPORTED_MANIFESTS: &[&str] = &[
    ".agents/plugins/marketplace.json",
    ".agents/plugins/api_marketplace.json",
    ".claude-plugin/marketplace.json",
    ".cursor-plugin/marketplace.json",
];

const MANIFEST_MISSING_MARKER: &str = "does not contain a supported manifest";

/// Only staging clones from a failed add are ever rescued; this keeps a
/// retried add from looping back through the rescue path.
const STAGING_MARKER: &str = ".staging/marketplace-add-";

const FALLBACK_NAME: &str = "imported-skills";

/// True when the add failed specifically because the source repository
/// has no marketplace manifest; only then is a rescue worth attempting.
pub(crate) fn is_manifest_missing_error(error_text: &str) -> bool {
    error_text.contains(MANIFEST_MISSING_MARKER)
}

/// Rescues a failed add whose source turned out to be a skill-only
/// repository: generates a Codex manifest from its `skills/` layout,
/// moves the staged clone into a persistent GUI-owned root and returns
/// the local path a retried add should use.
pub(crate) fn rescue(error_text: &str) -> Result<String, String> {
    let base = local_marketplaces_root().ok_or("无法定位 ~/.codex 目录")?;
    rescue_into(error_text, &base)
}

fn rescue_into(error_text: &str, base: &Path) -> Result<String, String> {
    let staged = staged_root_from_error(error_text).ok_or("错误中未引用可恢复的暂存目录")?;
    if !manifests_are_absent(&staged) {
        return Err("暂存目录已包含市场清单".to_string());
    }
    let manifest = generated_manifest(&staged)?;

    fs::create_dir_all(base).map_err(|err| format!("创建 {} 失败：{err}", base.display()))?;
    let destination = unique_destination(base, manifest_marketplace_name(&manifest));

    let manifest_dir = staged.join(".agents/plugins");
    fs::create_dir_all(&manifest_dir).map_err(|err| format!("创建清单目录失败：{err}"))?;
    fs::write(
        manifest_dir.join("marketplace.json"),
        serde_json::to_string_pretty(&manifest).map_err(|err| format!("序列化清单失败：{err}"))?,
    )
    .map_err(|err| format!("写入清单失败：{err}"))?;

    fs::rename(&staged, &destination)
        .map_err(|err| format!("将暂存目录移入 {} 失败：{err}", destination.display()))?;

    Ok(destination.display().to_string())
}

/// Extracts the staged clone path from an add error, keeping only
/// directories under the marketplace staging area.
fn staged_root_from_error(error_text: &str) -> Option<PathBuf> {
    if !is_manifest_missing_error(error_text) {
        return None;
    }
    let start = error_text.find('\'')? + 1;
    let rest = &error_text[start..];
    let end = rest.find('\'')?;
    let raw = &rest[..end];
    if !raw.contains(STAGING_MARKER) {
        return None;
    }
    let path = PathBuf::from(raw);
    path.is_dir().then_some(path)
}

/// Builds the manifest that turns `root` into a local marketplace.
fn generated_manifest(root: &Path) -> Result<serde_json::Value, String> {
    let plugins = collect_plugins(root);
    if plugins.is_empty() {
        return Err("仓库中没有 skills/*/SKILL.md 结构，无法生成市场清单".to_string());
    }
    let display_name = repo_display_name(root).unwrap_or_else(|| FALLBACK_NAME.to_string());
    let name = sanitize_name(&display_name);
    let entries = plugins
        .iter()
        .map(|(plugin_name, path)| {
            json!({
                "name": plugin_name,
                "source": {"source": "local", "path": path},
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "name": name,
        "interface": {"displayName": display_name},
        "plugins": entries,
    }))
}

fn manifest_marketplace_name(manifest: &serde_json::Value) -> &str {
    manifest["name"].as_str().unwrap_or(FALLBACK_NAME)
}

/// Collects plugin entries (name, path relative to the marketplace
/// root) for the two skill layouts a plain repository may use:
/// repo-as-plugin (`skills/<skill>/SKILL.md`) or one plugin per
/// top-level directory (`<dir>/skills/<skill>/SKILL.md`).
fn collect_plugins(root: &Path) -> Vec<(String, String)> {
    if skill_dir_names(&root.join("skills")).next().is_some() {
        let name = repo_display_name(root).unwrap_or_else(|| FALLBACK_NAME.to_string());
        return vec![(sanitize_name(&name), "./".to_string())];
    }

    let mut entries = Vec::new();
    let Ok(children) = fs::read_dir(root) else {
        return entries;
    };
    let mut directories = children
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    for directory in directories {
        let Some(dir_name) = directory.file_name().and_then(|raw| raw.to_str()) else {
            continue;
        };
        let plugin_name = sanitize_name(dir_name);
        if skill_dir_names(&directory.join("skills")).next().is_some()
            && entries.iter().all(|(name, _)| name != &plugin_name)
        {
            entries.push((plugin_name, format!("{dir_name}/")));
        }
    }
    entries
}

fn skill_dir_names(skills_dir: &Path) -> impl Iterator<Item = String> {
    let discovered = fs::read_dir(skills_dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir() && path.join("SKILL.md").is_file())
                .filter_map(|path| {
                    path.file_name()
                        .and_then(|raw| raw.to_str().map(String::from))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    discovered.into_iter()
}

/// The repository name from the staged clone's origin remote, when the
/// clone still carries its git metadata.
fn repo_display_name(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout.trim_end().trim_end_matches('/');
    let last = url.rsplit('/').next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

/// Lowercase ASCII identifier with dashes; anything else collapses.
fn sanitize_name(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_destination(base: &Path, name: &str) -> PathBuf {
    let mut candidate = base.join(name);
    let mut counter = 2;
    while candidate.exists() {
        candidate = base.join(format!("{name}-{counter}"));
        counter += 1;
    }
    candidate
}

fn manifests_are_absent(root: &Path) -> bool {
    !SUPPORTED_MANIFESTS
        .iter()
        .any(|manifest| root.join(manifest).is_file())
}

fn local_marketplaces_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".codex")
            .join("marketplaces-local"),
    )
}

#[cfg(test)]
#[path = "marketplace_rescue_tests.rs"]
mod tests;
