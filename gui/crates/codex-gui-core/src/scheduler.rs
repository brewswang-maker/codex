//! Durable scheduled prompts: one-shot and recurring tasks the GUI
//! submits into the conversation when they come due.
//!
//! Tasks persist in `~/.codex/gui-schedule.json`. `due` is the only
//! state-transition point and is driven by the UI tick, so firing is
//! deterministic under replay: a one-shot flips itself off after its
//! single run, a recurring task advances `last_run` by its interval.

use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

/// The current Unix time in whole seconds; the GUI passes this into every
/// timestamped call so tests can drive the clock.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// When a task fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScheduleKind {
    /// Fires once at the given Unix time, then disables itself.
    #[serde(rename_all = "camelCase")]
    Once {
        /// Absolute fire time in Unix seconds.
        run_at: i64,
    },
    /// Fires every `interval_secs`, counted from the previous run (or
    /// creation before the first run).
    #[serde(rename_all = "camelCase")]
    Every {
        /// The period between runs.
        interval_secs: u64,
    },
}

impl ScheduleKind {
    /// The human-readable cadence shown in the scheduler panel.
    pub fn label(&self) -> String {
        match self {
            ScheduleKind::Once { run_at } => format!("一次性（epoch {run_at}）"),
            ScheduleKind::Every { interval_secs } => {
                format!("每 {}", humanize_interval(*interval_secs))
            }
        }
    }
}

/// Renders a second count as `90 秒` / `30 分钟` / `2 小时` / `1 天`.
pub fn humanize_interval(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} 秒"),
        60..=3599 => format!("{} 分钟", secs / 60),
        3600..=86399 => format!("{} 小时", secs / 3600),
        _ => format!("{} 天", secs / 86400),
    }
}

/// Parses a shorthand delay such as `90s` / `30m` / `2h` / `1d` into
/// seconds; a bare number means minutes. `None` on garbage.
pub fn parse_delay_secs(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (digits, unit) = text.split_at(text.len().saturating_sub(1));
    let multiplier = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => {
            // Not a recognized unit suffix: the whole text is the number
            // of minutes.
            return text.parse::<u64>().ok().map(|minutes| minutes * 60);
        }
    };
    let value = match digits.parse::<u64>() {
        Ok(value) => value,
        // `30m` split fine; a pure number like `45` lands here because the
        // last char is a digit, so re-parse the whole text as minutes.
        Err(_) => return text.parse::<u64>().ok().map(|minutes| minutes * 60),
    };
    Some(value * multiplier)
}

/// One scheduled prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    /// Stable unique id (also the storage key for edits).
    pub id: String,
    /// The display name shown in the panel.
    pub name: String,
    /// When it fires.
    pub kind: ScheduleKind,
    /// The prompt submitted into the conversation when due.
    pub prompt: String,
    /// Disabled tasks never fire but stay listed.
    pub enabled: bool,
    /// The last fire time, if any.
    pub last_run: Option<i64>,
    /// Creation time (also the first-run base for recurring tasks).
    pub created_at: i64,
}

/// The persisted task list.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheduler {
    /// Every task, oldest first.
    pub tasks: Vec<ScheduledTask>,
}

impl Scheduler {
    /// Loads `~/.codex/gui-schedule.json`; failures fall back to empty.
    pub fn load_or_default() -> Self {
        let Some(home) = home_dir() else {
            return Self::default();
        };
        let path = home.join(".codex").join("gui-schedule.json");
        Self::load_from(&path).unwrap_or_default()
    }

    /// Loads from an explicit path (also the test seam).
    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Writes the list back; `false` when the disk write failed (the
    /// in-memory list stays authoritative either way).
    pub fn save(&self) -> bool {
        let Some(home) = home_dir() else {
            return false;
        };
        let path = home.join(".codex").join("gui-schedule.json");
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

    /// Adds a task and persists; returns its id.
    pub fn add(&mut self, name: &str, kind: ScheduleKind, prompt: &str, now: i64) -> String {
        let id = format!("task-{now}-{}", self.tasks.len());
        self.tasks.push(ScheduledTask {
            id: id.clone(),
            name: name.to_string(),
            kind,
            prompt: prompt.to_string(),
            enabled: true,
            last_run: None,
            created_at: now,
        });
        self.save();
        id
    }

    /// Removes a task; `true` when something was removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|task| task.id != id);
        let removed = self.tasks.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// Enables or disables a task; `true` when the id existed.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) else {
            return false;
        };
        task.enabled = enabled;
        self.save();
        true
    }

    /// Fires every due task and returns the fired snapshots. A one-shot
    /// disables itself after firing; a recurring task advances `last_run`
    /// so the next fire is a full interval away. Persists on any change.
    pub fn due(&mut self, now: i64) -> Vec<ScheduledTask> {
        let mut fired = Vec::new();
        for task in &mut self.tasks {
            if !task.enabled {
                continue;
            }
            match &task.kind {
                ScheduleKind::Once { run_at } => {
                    if now >= *run_at {
                        task.last_run = Some(now);
                        task.enabled = false;
                        fired.push(task.clone());
                    }
                }
                ScheduleKind::Every { interval_secs } => {
                    let interval = (*interval_secs).max(1) as i64;
                    let base = task.last_run.unwrap_or(task.created_at);
                    if now >= base + interval {
                        task.last_run = Some(now);
                        fired.push(task.clone());
                    }
                }
            }
        }
        if !fired.is_empty() {
            self.save();
        }
        fired
    }
}

/// The user's home directory from the environment; POSIX sets `HOME`, and
/// `USERPROFILE` covers Windows.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
