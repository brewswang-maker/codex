//! Better Harness: managed lifecycles for external coding-agent runs.
//!
//! A harness session wraps one agent command (for example
//! `codex exec "…"`), tracking its process, its stage in the
//! plan → implement → review → repair loop, its captured log, the
//! recorded review verdict, and the derived repair checklist.
//!
//! Child processes live in [`HarnessBoard::children`], which is skipped
//! by serde: persisted sessions that claim to be running were aborted by
//! the GUI exiting and are re-marked on load.

use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

/// Where a session sits in the four-stage loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HarnessStage {
    /// Agree on the plan.
    Plan,
    /// Execute it.
    Implement,
    /// Audit the result.
    Review,
    /// Fix what review found.
    Repair,
}

impl HarnessStage {
    /// The Chinese label shown in the panel.
    pub fn label(self) -> &'static str {
        match self {
            HarnessStage::Plan => "规划",
            HarnessStage::Implement => "实现",
            HarnessStage::Review => "审查",
            HarnessStage::Repair => "修复",
        }
    }

    /// The next stage in the loop.
    pub fn next(self) -> Self {
        match self {
            HarnessStage::Plan => HarnessStage::Implement,
            HarnessStage::Implement => HarnessStage::Review,
            HarnessStage::Review => HarnessStage::Repair,
            HarnessStage::Repair => HarnessStage::Plan,
        }
    }
}

/// The process state of one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum HarnessStatus {
    /// Created but never started.
    #[serde(rename_all = "camelCase")]
    Pending,
    /// The child process is live.
    #[serde(rename_all = "camelCase")]
    Running,
    /// The child exited with code 0.
    Succeeded,
    /// The child exited with a nonzero code, or never got to run.
    #[serde(rename_all = "camelCase")]
    Failed {
        /// The exit code, or the launch/abort explanation.
        reason: String,
    },
    /// Killed by the user.
    Stopped,
}

/// One recorded review pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessReview {
    /// The verdict headline, e.g. `通过` or `存在问题`.
    pub verdict: String,
    /// Findings, one per line; feeds the repair plan.
    pub findings: Vec<String>,
    /// Unix seconds of the recording.
    pub recorded_at: i64,
}

/// One checkbox of the repair plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairStep {
    /// What to fix.
    pub text: String,
    /// Whether it is done.
    pub done: bool,
}

/// One managed external agent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessSession {
    /// Stable unique id.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The shell command of the external agent.
    pub command: String,
    /// The working directory, if any.
    pub cwd: Option<PathBuf>,
    /// The current loop stage.
    pub stage: HarnessStage,
    /// The process state.
    pub status: HarnessStatus,
    /// The log file the child's output streams into.
    pub log_path: PathBuf,
    /// The recorded review, if any.
    pub review: Option<HarnessReview>,
    /// The repair checklist derived from the review.
    pub repair_steps: Vec<RepairStep>,
    /// Unix seconds of the last start.
    pub started_at: Option<i64>,
    /// Unix seconds of the last observed exit.
    pub exited_at: Option<i64>,
}

/// The persisted harness board.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessBoard {
    /// Every session, creation order.
    pub sessions: Vec<HarnessSession>,
    /// Live child processes by session id (not persisted).
    #[serde(skip)]
    children: BTreeMap<String, Child>,
}

impl HarnessBoard {
    /// Loads `~/.codex/gui-harness.json`.
    pub fn load_or_default() -> Self {
        let Some(home) = home_dir() else {
            return Self::default();
        };
        let path = home.join(".codex").join("gui-harness.json");
        Self::load_from(&path).unwrap_or_default()
    }

    /// Loads from an explicit path (also the test seam). Sessions that
    /// still claim `Running` were aborted when the GUI died; their child
    /// processes no longer exist.
    pub fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let mut board: Self = serde_json::from_str(&raw).ok()?;
        for session in &mut board.sessions {
            if session.status == HarnessStatus::Running {
                session.status = HarnessStatus::Failed {
                    reason: "GUI 退出导致运行中断".to_string(),
                };
            }
        }
        Some(board)
    }

    /// Persists the board; `false` when the disk write failed.
    pub fn save(&self) -> bool {
        let Some(home) = home_dir() else {
            return false;
        };
        let path = home.join(".codex").join("gui-harness.json");
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

    /// Adds a session and persists; returns its id.
    pub fn add(&mut self, name: &str, command: &str, cwd: Option<PathBuf>, now: i64) -> String {
        let id = format!("harness-{now}-{}", self.sessions.len());
        self.sessions.push(HarnessSession {
            id: id.clone(),
            name: name.to_string(),
            command: command.to_string(),
            cwd,
            stage: HarnessStage::Plan,
            status: HarnessStatus::Pending,
            log_path: log_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(format!("{id}.log")),
            review: None,
            repair_steps: Vec::new(),
            started_at: None,
            exited_at: None,
        });
        self.save();
        id
    }

    /// Removes a session, killing it first when live.
    pub fn remove(&mut self, id: &str) -> bool {
        if let Some(mut child) = self.children.remove(id) {
            let _ = child.kill();
        }
        let before = self.sessions.len();
        self.sessions.retain(|session| session.id != id);
        let removed = self.sessions.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// Spawns the external agent for one session. Output appends to the
    /// session log so a rerun keeps history readable.
    pub fn start(&mut self, id: &str, now: i64) -> Result<(), String> {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return Err(format!("会话 {id} 不存在"));
        };
        if self.children.contains_key(id) {
            return Err("会话已在运行".to_string());
        }
        if let Some(parent) = session.log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&session.log_path)
            .map_err(|err| format!("无法打开日志文件: {err}"))?;
        let log_err = log
            .try_clone()
            .map_err(|err| format!("无法复用日志句柄: {err}"))?;
        let mut command = shell_command(&session.command);
        if let Some(cwd) = &session.cwd {
            command.current_dir(cwd);
        }
        command
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        let child = command.spawn().map_err(|err| format!("启动失败: {err}"))?;
        self.children.insert(id.to_string(), child);
        session.status = HarnessStatus::Running;
        session.started_at = Some(now);
        session.exited_at = None;
        self.save();
        Ok(())
    }

    /// Kills a running session.
    pub fn stop(&mut self, id: &str) -> Result<(), String> {
        let mut child = self
            .children
            .remove(id)
            .ok_or_else(|| "会话未在运行".to_string())?;
        let _ = child.kill();
        let _ = child.wait();
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return Ok(());
        };
        session.status = HarnessStatus::Stopped;
        session.exited_at = Some(crate::scheduler::now_secs());
        self.save();
        Ok(())
    }

    /// Reaps finished children and updates their statuses. Called from
    /// the UI tick; returns the ids whose status changed.
    pub fn poll(&mut self, now: i64) -> Vec<String> {
        let mut changed = Vec::new();
        let ids: Vec<String> = self.children.keys().cloned().collect();
        for id in ids {
            let Some(child) = self.children.get_mut(&id) else {
                continue;
            };
            let outcome = match child.try_wait() {
                Ok(outcome) => outcome,
                Err(_) => continue,
            };
            let Some(exit) = outcome else {
                continue;
            };
            self.children.remove(&id);
            if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                session.status = match exit.code() {
                    Some(0) => HarnessStatus::Succeeded,
                    Some(code) => HarnessStatus::Failed {
                        reason: format!("退出码 {code}"),
                    },
                    None => HarnessStatus::Failed {
                        reason: "被信号终止".to_string(),
                    },
                };
                session.exited_at = Some(now);
            }
            changed.push(id);
        }
        if !changed.is_empty() {
            self.save();
        }
        changed
    }

    /// Whether a session's process is live.
    pub fn is_running(&self, id: &str) -> bool {
        self.children.contains_key(id)
    }

    /// Advances a session to the next loop stage.
    pub fn advance_stage(&mut self, id: &str) -> bool {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return false;
        };
        session.stage = session.stage.next();
        self.save();
        true
    }

    /// Records a review verdict; findings become repair candidates.
    pub fn record_review(&mut self, id: &str, verdict: &str, findings: &str, now: i64) -> bool {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return false;
        };
        let findings: Vec<String> = findings
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();
        session.review = Some(HarnessReview {
            verdict: verdict.to_string(),
            findings,
            recorded_at: now,
        });
        self.save();
        true
    }

    /// Turns the recorded review's findings into the repair checklist.
    pub fn plan_repairs(&mut self, id: &str) -> Result<usize, String> {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return Err(format!("会话 {id} 不存在"));
        };
        let review = session
            .review
            .as_ref()
            .ok_or_else(|| "还没有审查记录".to_string())?;
        session.repair_steps = review
            .findings
            .iter()
            .map(|finding| RepairStep {
                text: finding.clone(),
                done: false,
            })
            .collect();
        let count = session.repair_steps.len();
        self.save();
        Ok(count)
    }

    /// Flips one repair checkbox.
    pub fn toggle_repair_step(&mut self, id: &str, index: usize) -> bool {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return false;
        };
        let Some(step) = session.repair_steps.get_mut(index) else {
            return false;
        };
        step.done = !step.done;
        self.save();
        true
    }

    /// The last `max_lines` lines of a session's log.
    pub fn log_tail(&self, id: &str, max_lines: usize) -> Vec<String> {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return Vec::new();
        };
        let Ok(raw) = std::fs::read_to_string(&session.log_path) else {
            return Vec::new();
        };
        let lines: Vec<&str> = raw.lines().collect();
        let start = lines.len().saturating_sub(max_lines);
        lines[start..].iter().map(|line| line.to_string()).collect()
    }
}

/// Wraps a command string in the platform shell.
fn shell_command(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut sh = Command::new("sh");
        sh.arg("-c").arg(command);
        sh
    }
}

/// The harness log directory: `~/.codex/gui-harness-logs`.
fn log_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".codex").join("gui-harness-logs"))
}

/// The user's home directory from the environment; POSIX sets `HOME`, and
/// `USERPROFILE` covers Windows.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
#[path = "harness_tests.rs"]
mod tests;
