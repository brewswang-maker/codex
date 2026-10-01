//! The FFI boundary of the plugin system, isolated in this crate so the
//! rest of the GUI can keep the workspace-wide `unsafe_code = "forbid"`.
//!
//! Every function here opens the plugin binary fresh and drops it before
//! returning: no long-lived library handles, so upgrading or removing a
//! plugin file never fights an open mmap.

use crate::CommandDescriptor;
use crate::Plugin;
use crate::SkillDescriptor;
use libloading::Library;
use libloading::Symbol;
use std::path::Path;

/// The capabilities a plugin binary declares at its entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCapabilities {
    /// The plugin name (becomes the install id).
    pub name: String,
    /// The plugin version.
    pub version: String,
    /// Offered skills.
    pub skills: Vec<SkillDescriptor>,
    /// Offered commands.
    pub commands: Vec<CommandDescriptor>,
}

/// Opens a plugin binary and reads what it offers. Load errors come back
/// as human-readable strings for the market panel.
pub fn load_capabilities(so_path: &Path) -> Result<PluginCapabilities, String> {
    with_instance(so_path, |plugin| PluginCapabilities {
        name: plugin.name().to_string(),
        version: plugin.version().to_string(),
        skills: plugin.skills(),
        commands: plugin.commands(),
    })
}

/// Invokes one skill on a freshly loaded plugin instance.
pub fn invoke_skill(so_path: &Path, skill_id: &str, args: &str) -> Result<String, String> {
    with_instance(so_path, |plugin| plugin.invoke_skill(skill_id, args))?
}

/// Invokes one command on a freshly loaded plugin instance.
pub fn run_command(so_path: &Path, command_id: &str, args: &str) -> Result<String, String> {
    with_instance(so_path, |plugin| plugin.run_command(command_id, args))?
}

/// Runs one closure against a freshly constructed plugin, then unloads.
fn with_instance<T>(so_path: &Path, call: impl FnOnce(&dyn Plugin) -> T) -> Result<T, String> {
    // SAFETY: loading a third-party cdylib executes its init code and
    // hands back a `Plugin` trait object built by the same rustc as this
    // SDK (releases pin the SDK version; see the crate docs). Errors are
    // surfaced, never panicked across the boundary.
    unsafe {
        let library = Library::new(so_path)
            .map_err(|err| format!("无法加载动态库 {}: {err}", so_path.display()))?;
        let entry: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = library
            .get(b"codex_plugin_entry")
            .map_err(|err| format!("缺少插件入口符号 codex_plugin_entry: {err}"))?;
        let plugin = Box::from_raw((entry)());
        let outcome = call(plugin.as_ref());
        drop(plugin);
        drop(library);
        Ok(outcome)
    }
}
