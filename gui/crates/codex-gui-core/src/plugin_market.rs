//! The plugin market: discovery, install, upgrade, rollback, and
//! invocation of third-party `cdylib` plugins built on
//! [`codex_plugin_sdk`].
//!
//! Install layout under `~/.codex/gui-plugins/`:
//!
//! ```text
//! <id>/plugin.so        the active binary
//! <id>/plugin.so.bak    the previous binary (rollback source)
//! ```
//!
//! The registry itself persists to `~/.codex/gui-plugins.json` and
//! caches each plugin's declared capabilities (skills/commands) so the
//! panel renders without touching the dynamic library. Libraries are
//! loaded ephemerally — per invocation — so upgrading or removing a file
//! never fights an open handle.

use codex_plugin_sdk::CommandDescriptor;
use codex_plugin_sdk::SkillDescriptor;
use codex_plugin_sdk::loader::invoke_skill as sdk_invoke_skill;
use codex_plugin_sdk::loader::load_capabilities;
use codex_plugin_sdk::loader::run_command as sdk_run_command;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;

/// One installed plugin as persisted in the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    /// Stable id, derived from the plugin's declared name.
    pub id: String,
    /// The display name (from the plugin itself).
    pub name: String,
    /// The declared version at install/upgrade time.
    pub version: String,
    /// Where it came from: a local path or the git URL.
    pub source: String,
    /// The install directory holding `plugin.so`.
    pub dir: PathBuf,
    /// Disabled plugins stay installed but cannot be invoked.
    pub enabled: bool,
    /// Cached capability listing refreshed on install/upgrade.
    pub skills: Vec<SkillDescriptor>,
    pub commands: Vec<CommandDescriptor>,
    /// The last load/upgrade failure, if any.
    pub last_error: Option<String>,
    /// Unix seconds of the initial install.
    pub installed_at: i64,
}

/// The in-memory registry behind the plugin market panel.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarket {
    /// Every installed plugin.
    pub plugins: Vec<PluginRecord>,
    /// The import input's current draft (a local path or a git URL).
    pub source_draft: String,
    /// The last operation feedback shown in the panel.
    pub status: Option<String>,
}

impl PluginMarket {
    /// The plugins root: `~/.codex/gui-plugins`.
    pub fn root() -> Option<PathBuf> {
        home_dir().map(|home| home.join(".codex").join("gui-plugins"))
    }

    /// Loads `~/.codex/gui-plugins.json`; failures fall back to empty.
    pub fn load_or_default() -> Self {
        let Some(home) = home_dir() else {
            return Self::default();
        };
        let path = home.join(".codex").join("gui-plugins.json");
        Self::load_from(&path).unwrap_or_default()
    }

    /// Loads from an explicit path (also the test seam).
    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Persists the registry; `false` when the disk write failed.
    pub fn save(&self) -> bool {
        let Some(home) = home_dir() else {
            return false;
        };
        let path = home.join(".codex").join("gui-plugins.json");
        self.save_to(&path)
    }

    /// Saves to an explicit path (also the test seam).
    pub fn save_to(&self, path: &Path) -> bool {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(raw) => std::fs::write(path, raw).is_ok(),
            Err(_) => false,
        }
    }

    /// Installs (or upgrades) from a built plugin binary. The old binary
    /// is kept as `plugin.so.bak` so rollback stays one click away.
    /// Loads the binary to read its declared identity and capabilities.
    pub fn install_from_path(&mut self, source: &Path, now: i64) -> Result<String, String> {
        let capabilities = load_capabilities(source)?;
        let Some(root) = Self::root() else {
            return Err("无法定位用户目录".to_string());
        };
        let dir = root.join(&capabilities.name);
        std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建插件目录: {err}"))?;
        let target = dir.join(plugin_binary_name());
        // Stage the copy first: a failed copy must not clobber the old
        // binary nor leave a stale backup.
        let binary_name = plugin_binary_name();
        let staged = dir.join("plugin.so.incoming");
        std::fs::copy(source, &staged).map_err(|err| format!("复制插件文件失败: {err}"))?;
        if target.exists() {
            let backup = dir.join(format!("{binary_name}.bak"));
            let _ = std::fs::rename(&target, &backup);
        }
        std::fs::rename(&staged, &target).map_err(|err| format!("启用新插件文件失败: {err}"))?;

        let existing = self
            .plugins
            .iter_mut()
            .find(|record| record.id == capabilities.name);
        let upgraded = existing.is_some();
        match existing {
            Some(record) => {
                record.version = capabilities.version.clone();
                record.source = source.display().to_string();
                record.enabled = true;
                record.skills = capabilities.skills.clone();
                record.commands = capabilities.commands.clone();
                record.last_error = None;
            }
            None => self.plugins.push(PluginRecord {
                id: capabilities.name.clone(),
                name: capabilities.name.clone(),
                version: capabilities.version.clone(),
                source: source.display().to_string(),
                dir: dir.clone(),
                enabled: true,
                skills: capabilities.skills.clone(),
                commands: capabilities.commands.clone(),
                last_error: None,
                installed_at: now,
            }),
        }
        self.status = Some(if upgraded {
            format!("已升级 {} → v{}", capabilities.name, capabilities.version)
        } else {
            format!("已安装 {} v{}", capabilities.name, capabilities.version)
        });
        self.save();
        Ok(capabilities.name)
    }

    /// Installs from a git URL: shallow-clones the repository and looks
    /// for a built plugin binary (`plugin.so`-style, or any `.so` under
    /// `target/release`). Source-only repositories must be built first.
    pub fn install_from_git(&mut self, url: &str, now: i64) -> Result<String, String> {
        let Some(root) = Self::root() else {
            return Err("无法定位用户目录".to_string());
        };
        let checkout = root.join("checkout");
        let _ = std::fs::remove_dir_all(&checkout);
        std::fs::create_dir_all(&checkout).map_err(|err| format!("无法创建临时目录: {err}"))?;
        let outcome = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                url,
                checkout.display().to_string().as_str(),
            ])
            .output()
            .map_err(|err| format!("无法运行 git: {err}"))?;
        if !outcome.status.success() {
            let stderr = String::from_utf8_lossy(&outcome.stderr);
            return Err(format!("git clone 失败: {}", stderr.trim()));
        }
        let Some(binary) = find_plugin_binary(&checkout) else {
            return Err(
                "仓库中没有已构建的插件二进制（需要 plugin.so / plugin.dylib / plugin.dll）"
                    .to_string(),
            );
        };
        self.install_from_path(&binary, now)
    }

    /// Restores the previous binary kept by the last upgrade.
    pub fn rollback(&mut self, id: &str) -> Result<(), String> {
        let record = self
            .plugins
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| format!("插件 {id} 未安装"))?;
        let binary = record.dir.join(plugin_binary_name());
        let binary_name = plugin_binary_name();
        let backup = record.dir.join(format!("{binary_name}.bak"));
        if !backup.exists() {
            return Err("没有可回滚的旧版本".to_string());
        }
        std::fs::rename(&backup, &binary).map_err(|err| format!("回滚失败: {err}"))?;
        if let Ok(capabilities) = load_capabilities(&binary) {
            record.version = capabilities.version;
            record.skills = capabilities.skills;
            record.commands = capabilities.commands;
        }
        record.last_error = None;
        self.status = Some(format!("已回滚 {id} → v{}", record.version));
        self.save();
        Ok(())
    }

    /// Uninstalls a plugin, removing its directory.
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        let Some(record) = self.plugins.iter().find(|record| record.id == id) else {
            return Err(format!("插件 {id} 未安装"));
        };
        let _ = std::fs::remove_dir_all(&record.dir);
        self.plugins.retain(|record| record.id != id);
        self.status = Some(format!("已卸载 {id}"));
        self.save();
        Ok(())
    }

    /// Enables or disables a plugin; disabled ones refuse invocation.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let Some(record) = self.plugins.iter_mut().find(|record| record.id == id) else {
            return false;
        };
        record.enabled = enabled;
        self.save();
        true
    }

    /// Runs one skill of an installed, enabled plugin.
    pub fn invoke_skill(&self, id: &str, skill_id: &str, args: &str) -> Result<String, String> {
        self.invoke(id, |so| sdk_invoke_skill(so, skill_id, args))
    }

    /// Runs one command of an installed, enabled plugin.
    pub fn run_command(&self, id: &str, command_id: &str, args: &str) -> Result<String, String> {
        self.invoke(id, |so| sdk_run_command(so, command_id, args))
    }

    /// Shared invocation path: locate, check, load, run.
    fn invoke(
        &self,
        id: &str,
        call: impl FnOnce(&Path) -> Result<String, String>,
    ) -> Result<String, String> {
        let record = self
            .plugins
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| format!("插件 {id} 未安装"))?;
        if !record.enabled {
            return Err(format!("插件 {id} 已禁用"));
        }
        call(&record.dir.join(plugin_binary_name()))
    }
}

/// The platform plugin binary file name.
pub fn plugin_binary_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "plugin.dylib"
    }
    #[cfg(target_os = "windows")]
    {
        "plugin.dll"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "plugin.so"
    }
}

/// Depth-first search for the first plugin-shaped binary in a checkout.
fn find_plugin_binary(dir: &Path) -> Option<PathBuf> {
    let wanted = plugin_binary_name();
    let mut queue = vec![dir.to_path_buf()];
    while let Some(current) = queue.pop() {
        let entries = std::fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
                continue;
            }
            let name = path.file_name().and_then(|name| name.to_str());
            if name == Some(wanted) {
                return Some(path);
            }
            // Any built .so under target/release is plugin-shaped enough
            // for a one-binary repository.
            if name.is_some_and(|name| name.ends_with(".so"))
                && path
                    .components()
                    .any(|part| part.as_os_str().to_str() == Some("release"))
            {
                return Some(path);
            }
        }
    }
    None
}

/// The user's home directory from the environment; POSIX sets `HOME`, and
/// `USERPROFILE` covers Windows.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
#[path = "plugin_market_tests.rs"]
mod tests;
