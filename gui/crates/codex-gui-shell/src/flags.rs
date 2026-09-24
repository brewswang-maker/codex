//! Launch flags for the GUI binary.
//!
//! Kept dependency-free (no `clap`) on purpose: the surface is three flags.

use codex_gui_bridge::Flags;
use std::path::PathBuf;

/// Parsed launch configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuiArgs {
    /// How to spawn the app-server backend.
    pub bridge: Flags,
    /// Skips GPU renderer initialization and forces the tiny-skia software
    /// compositor for the GUI itself.
    pub software_rendering: bool,
}

/// Parses `--app-server <path>` (repeatable last-one-wins),
/// `--app-server-arg <arg>` (appended to the backend command line), and
/// `--software-rendering` (forces the tiny-skia renderer via the
/// `ICED_BACKEND` preference that `iced/renderer/src/fallback.rs` consults).
///
/// Unknown flags are reported as errors so typos never launch silently
/// wrong backends.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<GuiArgs, String> {
    let mut parsed = GuiArgs {
        bridge: Flags::default_app_server(),
        software_rendering: false,
    };
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--app-server" => {
                let program = iter
                    .next()
                    .ok_or_else(|| "--app-server requires a path".to_string())?;
                parsed.bridge.program = PathBuf::from(program);
            }
            "--app-server-arg" => {
                let extra = iter
                    .next()
                    .ok_or_else(|| "--app-server-arg requires a value".to_string())?;
                parsed.bridge.args.push(extra);
            }
            "--software-rendering" => {
                parsed.software_rendering = true;
            }
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument {other:?}\n\n{}", usage())),
        }
    }

    Ok(parsed)
}

fn usage() -> &'static str {
    "Usage: codex-gui [--app-server <path>] [--app-server-arg <arg>] \
     [--software-rendering]"
}

#[cfg(test)]
#[path = "flags_tests.rs"]
mod tests;
