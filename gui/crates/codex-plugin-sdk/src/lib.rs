//! The Codex GUI plugin SDK: the trait surface a third-party plugin
//! implements and the C-ABI entry convention the loader expects.
//!
//! # Authoring a plugin
//!
//! Depend on this crate with `crate-type = ["cdylib"]` (plus `rlib` for
//! tests), implement [`Plugin`], and export it with
//! [`register_plugin!`]:
//!
//! ```ignore
//! use codex_plugin_sdk::CommandDescriptor;
//! use codex_plugin_sdk::Plugin;
//! use codex_plugin_sdk::SkillDescriptor;
//! use codex_plugin_sdk::register_plugin;
//!
//! #[derive(Default)]
//! struct Demo;
//!
//! impl Plugin for Demo {
//!     fn name(&self) -> &str { "demo" }
//!     fn version(&self) -> &str { "0.1.0" }
//!     fn skills(&self) -> Vec<SkillDescriptor> {
//!         vec![SkillDescriptor { id: "demo.hello".into(), title: "Hello".into(), description: "greets".into() }]
//!     }
//!     fn invoke_skill(&self, skill_id: &str, _args: &str) -> Result<String, String> {
//!         if skill_id == "demo.hello" { Ok("hi!".into()) } else { Err(format!("unknown skill {skill_id}")) }
//!     }
//! }
//!
//! register_plugin!(Demo);
//! ```
//!
//! Build with `cargo build --release` and import the produced
//! `plugin.so` / `plugin.dylib` / `plugin.dll` from the GUI plugin
//! market.
//!
//! # ABI note
//!
//! The `*mut dyn Plugin` hand-off is sound only between binaries built
//! by the same rustc (trait objects are not a stable ABI). Plugin
//! releases pin the SDK version; the loader surfaces load failures
//! instead of crashing.

pub mod loader;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

/// One invokable skill a plugin offers (surfaced in the plugin market).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDescriptor {
    /// Stable id, conventionally `plugin.skill`.
    pub id: String,
    /// The display title.
    pub title: String,
    /// The one-line explanation.
    pub description: String,
}

/// One runnable command a plugin offers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDescriptor {
    /// Stable id, conventionally `plugin.command`.
    pub id: String,
    /// The display title.
    pub title: String,
    /// The one-line explanation.
    pub description: String,
}

/// The plugin surface the GUI loads from a `cdylib`.
///
/// All methods take `&self` so one stateless implementation can be
/// shared; keep plugin state inside `UnsafeCell`/locks behind the impl
/// if concurrency demands it — the GUI may invoke from any thread.
pub trait Plugin {
    /// The unique plugin name (lowercase, no spaces).
    fn name(&self) -> &str;

    /// The semantic version of this build.
    fn version(&self) -> &str;

    /// The skills this plugin offers.
    fn skills(&self) -> Vec<SkillDescriptor> {
        Vec::new()
    }

    /// The commands this plugin offers.
    fn commands(&self) -> Vec<CommandDescriptor> {
        Vec::new()
    }

    /// Runs one skill; the error string is shown in the GUI.
    fn invoke_skill(&self, _skill_id: &str, _args: &str) -> Result<String, String> {
        Err("this plugin exposes no skills".to_string())
    }

    /// Runs one command; the error string is shown in the GUI.
    fn run_command(&self, _command_id: &str, _args: &str) -> Result<String, String> {
        Err("this plugin exposes no commands".to_string())
    }
}

/// Exports the plugin entry point for a [`Plugin`] implementor.
///
/// The type must implement `Default`; the loader constructs a fresh
/// instance per load.
#[macro_export]
macro_rules! register_plugin {
    ($ty:ty) => {
        // Edition 2024 requires attributes that expand to unsafe to be
        // wrapped in `unsafe()` themselves.
        #[unsafe(no_mangle)]
        pub extern "C" fn codex_plugin_entry() -> *mut dyn $crate::Plugin {
            let plugin: Box<dyn $crate::Plugin> =
                Box::new(<$ty as ::std::default::Default>::default());
            Box::into_raw(plugin)
        }
    };
}
