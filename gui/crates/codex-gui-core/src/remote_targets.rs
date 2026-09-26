//! Remote-target facts for the remote-explorer pane.
//!
//! Two read-only probes back the sidebar: the host aliases declared in
//! `~/.ssh/config` become the SSH target list, and a `docker info`
//! shell-out decides whether the Dev Containers section can do anything
//! on this machine.

use std::path::PathBuf;
use std::process::Command;

/// One SSH target as the remote-explorer lists it: the first alias of a
/// `Host` stanza, wildcard patterns excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    /// The alias users type after `ssh`, e.g. `github.com`.
    pub host: String,
}

/// Reads the host aliases from the user's `~/.ssh/config`; an unreadable
/// or absent file yields an empty list.
pub fn ssh_targets() -> Vec<SshTarget> {
    config_path().map_or_else(Vec::new, |path| {
        std::fs::read_to_string(path).map_or_else(|_| Vec::new(), |text| parse_ssh_config(&text))
    })
}

/// The ssh config location: `$CODEX_GUI_SSH_CONFIG` override for tests,
/// else `$HOME/.ssh/config`.
fn config_path() -> Option<PathBuf> {
    std::env::var_os("CODEX_GUI_SSH_CONFIG")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh/config")))
}

/// Extracts the SSH target aliases from one ssh-config text. Every alias
/// of a `Host` stanza becomes a target except wildcard patterns; the
/// keyword is case-insensitive, aliases are not, and quoted aliases may
/// hold spaces.
pub fn parse_ssh_config(text: &str) -> Vec<SshTarget> {
    let mut targets = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((keyword, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for alias in split_aliases(rest) {
            if alias.contains(['*', '?']) {
                continue;
            }
            targets.push(SshTarget { host: alias });
        }
    }
    targets
}

/// Splits a `Host` argument list into aliases: whitespace separated
/// outside quotes, preserved inside them.
fn split_aliases(rest: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in rest.chars() {
        match ch {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    aliases.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        aliases.push(current);
    }
    aliases
}

/// Whether a working `docker` daemon answers on this host; `None` when
/// the probe could not run at all.
pub fn docker_available() -> Option<bool> {
    let output = Command::new("docker")
        .args(["info", "--format", "ok"])
        .output()
        .ok()?;
    Some(output.status.success())
}

#[cfg(test)]
#[path = "remote_targets_tests.rs"]
mod tests;
