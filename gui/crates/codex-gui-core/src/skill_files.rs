//! User-level skill files behind the Skills page: create, import, edit,
//! and delete skills under `$CODEX_HOME/skills`.
//!
//! Every write keeps the `SKILL.md` contract the backend parser enforces
//! (`codex-rs/skills/src/parser.rs`): a leading `---` frontmatter block
//! carrying a non-empty `name` (at most 64 characters) and `description`,
//! followed by the body. Edits rewrite just those two scalar lines so any
//! other frontmatter keys (metadata, ...) survive untouched.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// The file name every skill directory declares itself through.
pub const SKILL_FILE: &str = "SKILL.md";

/// The name every user-level skill directory lives under.
pub const SKILLS_DIR: &str = "skills";

/// How many characters a skill name may use, matching the backend cap.
const MAX_NAME_LEN: usize = 64;
/// The upper bound on the skills one link may import, so a monorepo's
/// entire `SKILL.md` population never lands in the skills root by accident.
pub(crate) const MAX_LINK_SKILLS: usize = 32;

/// The declared frontmatter of one `SKILL.md`, with the body verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDocument {
    /// Declared skill name (single line; empty when the file omits it).
    pub name: String,
    /// Declared one-line description.
    pub description: String,
    /// Everything after the frontmatter block.
    pub body: String,
}

/// Collapses whitespace exactly like the backend sanitizer.
pub fn sanitize_single_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Validates a skill name for both the frontmatter and the on-disk
/// directory: a non-empty single line of at most 64 characters without
/// path separators or control characters.
pub fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(format!("name exceeds the {MAX_NAME_LEN}-character limit"));
    }
    if name == "." || name == ".." {
        return Err("name must not be a directory reference".to_string());
    }
    if name != name.trim() {
        return Err("name must not start or end with whitespace".to_string());
    }
    if name
        .chars()
        .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err("name must not contain path separators or control characters".to_string());
    }
    Ok(())
}

/// Splits `SKILL.md` text into its frontmatter scalars and the body.
pub fn parse_skill_document(text: &str) -> Result<SkillDocument, String> {
    let mut lines = text.lines();
    if !matches!(lines.next(), Some(line) if line.trim() == "---") {
        return Err("SKILL.md must start with a --- frontmatter block".to_string());
    }

    let mut frontmatter = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        frontmatter.push(line);
    }
    if !closed {
        return Err("frontmatter block is not closed with ---".to_string());
    }

    Ok(SkillDocument {
        name: frontmatter_scalar(&frontmatter, "name").unwrap_or_default(),
        description: frontmatter_scalar(&frontmatter, "description").unwrap_or_default(),
        body: lines.collect::<Vec<_>>().join("\n"),
    })
}

/// Renders a complete `SKILL.md` from its frontmatter and body.
pub fn render_skill_document(name: &str, description: &str, body: &str) -> String {
    let mut text = String::from("---\n");
    text.push_str(&format!("name: {}\n", frontmatter_value(name)));
    text.push_str(&format!(
        "description: {}\n",
        frontmatter_value(description)
    ));
    text.push_str("---\n\n");
    text.push_str(body.trim_end());
    text.push('\n');
    text
}

/// Creates a new user-level skill directory with a fresh `SKILL.md`.
pub fn create_skill(root: &Path, name: &str, description: &str) -> Result<PathBuf, String> {
    let name = sanitize_single_line(name);
    validate_skill_name(&name)?;
    let description = sanitize_single_line(description);
    if description.is_empty() {
        return Err("description must not be empty".to_string());
    }

    let dir = root.join(&name);
    if dir.exists() {
        return Err(format!("a skill named `{name}` already exists"));
    }
    fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;

    let body = format!("# {name}\n\n{description}\n");
    let path = dir.join(SKILL_FILE);
    if let Err(error) = fs::write(&path, render_skill_document(&name, &description, &body)) {
        // Leave nothing half-created behind.
        let _cleanup = fs::remove_dir_all(&dir);
        return Err(format!("write {}: {error}", path.display()));
    }
    Ok(path)
}

/// Imports a skill from an existing directory holding a `SKILL.md`,
/// copying the whole tree under `root`. The frontmatter name (falling
/// back to the source directory name) picks the destination directory.
pub fn import_skill_directory(root: &Path, source: &Path) -> Result<PathBuf, String> {
    if !source.is_dir() {
        return Err(format!("{} is not a directory", source.display()));
    }
    let source_file = source.join(SKILL_FILE);
    let contents = fs::read_to_string(&source_file)
        .map_err(|error| format!("read {}: {error}", source_file.display()))?;
    let document = parse_skill_document(&contents)?;

    let fallback = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = sanitize_single_line(if document.name.trim().is_empty() {
        &fallback
    } else {
        &document.name
    });
    validate_skill_name(&name)?;

    let destination = root.join(&name);
    if destination.exists() {
        return Err(format!("a skill named `{name}` already exists"));
    }
    copy_tree(source, &destination)?;
    Ok(destination.join(SKILL_FILE))
}

/// Imports a single `SKILL.md`-shaped file into its own directory under
/// `root`; the frontmatter name (falling back to the file stem) picks the
/// directory.
pub fn import_skill_file(root: &Path, source: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    let contents = fs::read_to_string(source)
        .map_err(|error| format!("read {}: {error}", source.display()))?;
    let document = parse_skill_document(&contents)?;

    let fallback = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = sanitize_single_line(if document.name.trim().is_empty() {
        &fallback
    } else {
        &document.name
    });
    validate_skill_name(&name)?;

    let dir = root.join(&name);
    if dir.exists() {
        return Err(format!("a skill named `{name}` already exists"));
    }
    fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;

    let path = dir.join(SKILL_FILE);
    if let Err(error) = fs::write(&path, &contents) {
        let _cleanup = fs::remove_dir_all(&dir);
        return Err(format!("write {}: {error}", path.display()));
    }
    Ok(path)
}

/// One parsed skill link: a `git clone` target plus the optional branch
/// and in-repo subdirectory a skill is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillRepoLink {
    /// The clone target handed to `git clone`.
    pub(crate) repo_url: String,
    /// Branch or tag to check out; `None` keeps the default branch.
    pub(crate) reference: Option<String>,
    /// Repo-relative directory holding the skill; `None` searches the
    /// checkout for its `SKILL.md`.
    pub(crate) subpath: Option<String>,
}

/// Parses a skill link into its clone target.
///
/// GitHub links may address a skill directly: `/tree/<ref>/<path>` names a
/// branch and subdirectory, `/blob/<ref>/.../SKILL.md` a file whose parent
/// directory is the skill. Any other `git`-cloneable URL is taken as-is.
pub(crate) fn parse_skill_link(link: &str) -> Result<SkillRepoLink, String> {
    let trimmed = link.trim();
    if trimmed.is_empty() {
        return Err("请输入仓库链接".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("链接不能包含空白字符".to_string());
    }

    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if host.eq_ignore_ascii_case("github.com") {
            return parse_github_link(path);
        }
    } else if !trimmed.starts_with("git@") && !trimmed.contains("://") {
        return Err("链接需为 git 仓库地址，例如 https://github.com/<owner>/<repo>".to_string());
    }

    Ok(SkillRepoLink {
        repo_url: trimmed.trim_end_matches('/').to_string(),
        reference: None,
        subpath: None,
    })
}

/// The GitHub branch of [`parse_skill_link`]: `owner/repo`, optionally
/// followed by `/tree/<ref>/<path>` or `/blob/<ref>/<file>`.
fn parse_github_link(path: &str) -> Result<SkillRepoLink, String> {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let (Some(owner), Some(repo)) = (segments.first(), segments.get(1)) else {
        return Err("GitHub 链接缺少 <owner>/<repo>".to_string());
    };
    let repo = repo.trim_end_matches(".git");

    let mut reference = None;
    let mut subpath = None;
    match segments.get(2).copied() {
        None => {}
        Some("tree") => {
            let Some(ref_name) = segments.get(3) else {
                return Err("GitHub tree 链接缺少分支名".to_string());
            };
            reference = Some((*ref_name).to_string());
            let sub = segments[4..].join("/");
            subpath = (!sub.is_empty()).then_some(sub);
        }
        Some("blob") => {
            let Some(ref_name) = segments.get(3) else {
                return Err("GitHub blob 链接缺少分支名".to_string());
            };
            reference = Some((*ref_name).to_string());
            let file = segments[4..].join("/");
            if file == SKILL_FILE {
                // The repo root itself is the skill directory.
            } else if let Some(dir) = file.strip_suffix(&format!("/{SKILL_FILE}")) {
                subpath = Some(dir.to_string());
            } else {
                return Err("blob 链接需指向 SKILL.md".to_string());
            }
        }
        // A plain repo path (`owner/repo/sub/dir`) selects that subdirectory.
        Some(_) => subpath = Some(segments[2..].join("/")),
    }

    Ok(SkillRepoLink {
        repo_url: format!("https://github.com/{owner}/{repo}"),
        reference,
        subpath,
    })
}

/// The outcome of importing skills from one Git link.
///
/// A batch that only partially imports keeps the skills that landed and
/// reports one message per failure, so a single bad skill never discards
/// the rest of the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkImportReport {
    /// Installed `SKILL.md` paths, ordered by source directory.
    pub imported: Vec<PathBuf>,
    /// One message per skill directory that could not be imported.
    pub failed: Vec<String>,
}

/// Imports every skill a Git link resolves to.
///
/// A GitHub `/tree/` link names one branch and subtree; a plain repo link
/// imports the checkout's root `SKILL.md`, or every nested `SKILL.md` when
/// the repo nests its skills instead. Only a repo without a single
/// importable skill fails as a whole.
pub fn import_skill_from_link(root: &Path, link: &str) -> Result<LinkImportReport, String> {
    let parsed = parse_skill_link(link)?;
    let staging = std::env::temp_dir().join(format!(
        "codex-skill-link-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));

    let result = (|| {
        fs::create_dir_all(&staging)
            .map_err(|error| format!("create {}: {error}", staging.display()))?;
        let checkout = staging.join("repo");
        clone_skill_repo(&parsed, &checkout)?;

        let selected = select_skill_dirs(&parsed, &checkout)?;
        // The checkout metadata never becomes part of an installed skill.
        let _remove_git = fs::remove_dir_all(checkout.join(".git"));

        let mut report = LinkImportReport {
            imported: Vec::new(),
            failed: Vec::new(),
        };
        for dir in &selected {
            match ensure_skill_dir(dir).and_then(|()| import_skill_directory(root, dir)) {
                Ok(path) => report.imported.push(path),
                Err(error) => report.failed.push(error),
            }
        }
        if report.imported.is_empty() {
            return Err(report
                .failed
                .first()
                .cloned()
                .unwrap_or_else(|| format!("仓库中未找到 {SKILL_FILE}")));
        }
        Ok(report)
    })();
    let _cleanup = fs::remove_dir_all(&staging);
    result
}

/// Clones `parsed` into `checkout`, restricting the checkout to the skill
/// subdirectory when the link names one.
fn clone_skill_repo(parsed: &SkillRepoLink, checkout: &Path) -> Result<(), String> {
    let checkout = checkout.to_string_lossy().into_owned();
    let mut args = vec![
        "clone".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "--single-branch".to_string(),
    ];
    if parsed.subpath.is_some() {
        args.push("--filter=blob:none".to_string());
        args.push("--sparse".to_string());
    }
    if let Some(reference) = &parsed.reference {
        args.push("--branch".to_string());
        args.push(reference.clone());
    }
    args.push(parsed.repo_url.clone());
    args.push(checkout.clone());
    run_git(&args)?;

    if let Some(subpath) = &parsed.subpath {
        run_git(&[
            "-C".to_string(),
            checkout,
            "sparse-checkout".to_string(),
            "set".to_string(),
            subpath.clone(),
        ])?;
    }
    Ok(())
}

/// Runs one `git` invocation, mapping failures to its stderr tail.
fn run_git(args: &[String]) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(args)
        .output()
        .map_err(|error| format!("git 无法执行：{error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("git 以失败状态退出")
        .chars()
        .take(200)
        .collect::<String>();
    Err(format!("git 命令失败：{detail}"))
}

/// Resolves the skill directories a link imports: the `/tree/` subpath
/// (or the whole checkout when the link names none), the root `SKILL.md`
/// when one exists, else every nested `SKILL.md` up to the import cap.
pub(crate) fn select_skill_dirs(
    parsed: &SkillRepoLink,
    checkout: &Path,
) -> Result<Vec<PathBuf>, String> {
    let base = match &parsed.subpath {
        Some(subpath) => checkout.join(subpath),
        None => checkout.to_path_buf(),
    };
    if !base.is_dir() {
        return Err(match &parsed.subpath {
            Some(subpath) => format!("仓库中不存在目录 {subpath}"),
            None => format!("仓库中不存在目录 {}", checkout.display()),
        });
    }
    if base.join(SKILL_FILE).is_file() {
        return Ok(vec![base]);
    }

    let mut found = Vec::new();
    collect_skill_dirs(&base, &mut found);
    found.sort();
    match found.len() {
        0 => Err(format!("仓库中未找到 {SKILL_FILE}")),
        count if count > MAX_LINK_SKILLS => Err(format!(
            "仓库包含 {count} 个 skill（超过单次导入上限 {MAX_LINK_SKILLS}），请改用 /tree/<ref>/<子目录> 链接指定"
        )),
        _ => Ok(found),
    }
}

/// Collects every directory under `checkout` holding a `SKILL.md`, never
/// descending into `.git` or through symbolic links.
fn collect_skill_dirs(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        if path.join(SKILL_FILE).is_file() {
            found.push(path);
            continue;
        }
        collect_skill_dirs(&path, found);
    }
}

/// Validates the selected skill directory: it exists, holds a `SKILL.md`,
/// and contains no symbolic links (mirroring the installer's rule).
fn ensure_skill_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("仓库中不存在目录 {}", dir.display()));
    }
    if !dir.join(SKILL_FILE).is_file() {
        return Err(format!("目录 {} 中没有 {SKILL_FILE}", dir.display()));
    }
    reject_symlinks(dir)
}

/// Rejects a skill tree containing symbolic links.
fn reject_symlinks(dir: &Path) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", dir.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("stat {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "skill 目录包含符号链接：{}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            reject_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

/// Rewrites the `name`/`description` scalar lines of one `SKILL.md` in
/// place, keeping every other frontmatter entry and the body untouched.
pub fn update_skill(skill_path: &Path, name: &str, description: &str) -> Result<(), String> {
    let name = sanitize_single_line(name);
    validate_skill_name(&name)?;
    let description = sanitize_single_line(description);
    if description.is_empty() {
        return Err("description must not be empty".to_string());
    }

    let current = fs::read_to_string(skill_path)
        .map_err(|error| format!("read {}: {error}", skill_path.display()))?;
    let updated = rewrite_frontmatter(&current, &name, &description)?;
    fs::write(skill_path, updated)
        .map_err(|error| format!("write {}: {error}", skill_path.display()))
}

/// Removes one user-level skill: the path must be the skill's `SKILL.md`
/// and must never point at a skills root itself.
pub fn delete_skill(skill_path: &Path) -> Result<(), String> {
    if skill_path.file_name().is_none_or(|file| file != SKILL_FILE) {
        return Err("only a skill's SKILL.md can be deleted".to_string());
    }
    let Some(dir) = skill_path.parent() else {
        return Err("skill path has no parent directory".to_string());
    };
    let refuse_root = dir
        .file_name()
        .is_none_or(|name| name == SKILLS_DIR || name == ".agents");
    if refuse_root {
        return Err("refusing to delete a skills root".to_string());
    }
    if !skill_path.is_file() {
        return Err(format!("{} does not exist", skill_path.display()));
    }
    fs::remove_dir_all(dir).map_err(|error| format!("remove {}: {error}", dir.display()))
}

/// The `SKILL.md` frontmatter/body-preserving rewrite behind
/// [`update_skill`]; missing scalars are appended inside the block.
fn rewrite_frontmatter(text: &str, name: &str, description: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().is_none_or(|line| line.trim() != "---") {
        return Err("SKILL.md must start with a --- frontmatter block".to_string());
    }
    let Some(end) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim() == "---")
        .map(|(index, _)| index)
    else {
        return Err("frontmatter block is not closed with ---".to_string());
    };

    let mut kept: Vec<String> = Vec::new();
    let mut replaced_name = false;
    let mut replaced_description = false;
    let mut skip_continuation = false;
    for (index, line) in lines.iter().enumerate() {
        if index == end {
            if !replaced_name {
                kept.push(format!("name: {}", frontmatter_value(name)));
            }
            if !replaced_description {
                kept.push(format!("description: {}", frontmatter_value(description)));
            }
            kept.push((*line).to_string());
            continue;
        }
        if index == 0 || index > end {
            kept.push((*line).to_string());
            continue;
        }

        let indented = line.starts_with([' ', '\t']);
        if skip_continuation && indented && !line.trim().is_empty() {
            // Drop the block-scalar continuation the replacement supersedes.
            continue;
        }
        if indented || line.trim().is_empty() {
            kept.push((*line).to_string());
            continue;
        }
        skip_continuation = false;

        let Some((key, value)) = line.split_once(':') else {
            kept.push((*line).to_string());
            continue;
        };
        let block_value = {
            let value = value.trim();
            value.is_empty() || value.starts_with('|') || value.starts_with('>')
        };
        match key.trim() {
            "name" if !replaced_name => {
                kept.push(format!("name: {}", frontmatter_value(name)));
                replaced_name = true;
                skip_continuation = block_value;
            }
            "description" if !replaced_description => {
                kept.push(format!("description: {}", frontmatter_value(description)));
                replaced_description = true;
                skip_continuation = block_value;
            }
            _ => kept.push((*line).to_string()),
        }
    }

    let mut rendered = kept.join("\n");
    if text.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

/// Reads a top-level `key: value` scalar from frontmatter lines.
fn frontmatter_scalar(frontmatter: &[&str], key: &str) -> Option<String> {
    for line in frontmatter {
        if line.starts_with([' ', '\t']) {
            // Nested keys (metadata block, ...) never declare the scalars.
            continue;
        }
        let Some((candidate, value)) = line.split_once(':') else {
            continue;
        };
        if candidate.trim() != key {
            continue;
        }
        let value = unquote_scalar(value.trim());
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

/// Strips one layer of matching YAML quotes from a scalar.
fn unquote_scalar(raw: &str) -> String {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'' {
        return trimmed[1..trimmed.len() - 1].replace("''", "'");
    }
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

/// A YAML scalar for one frontmatter value: plain when simple, otherwise
/// single-quoted with `''` escaping (the same repair the backend applies).
fn frontmatter_value(value: &str) -> String {
    let special = value.is_empty()
        || value.contains([':', '#', '\'', '"', '\n'])
        || value.starts_with(['-', '?', '*', '&', '!', '|', '>', '%', '@', '`'])
        || value.starts_with(' ')
        || value.ends_with(' ');
    if special {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        value.to_string()
    }
}

/// Recursively copies `source` into `destination`.
fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    let entries =
        fs::read_dir(source).map_err(|error| format!("read {}: {error}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", source.display()))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let metadata =
            fs::metadata(&from).map_err(|error| format!("stat {}: {error}", from.display()))?;
        if metadata.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|error| format!("copy {}: {error}", from.display()))?;
        }
    }
    Ok(())
}
