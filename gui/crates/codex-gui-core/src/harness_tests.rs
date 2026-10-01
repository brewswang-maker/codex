//! Tests for the harness board, including real child processes via sh.
#![allow(clippy::expect_used, clippy::panic)]

use super::HarnessBoard;
use super::HarnessStage;
use super::HarnessStatus;
use crate::scheduler::now_secs;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gui-harness-test-{}-{tag}.json",
        std::process::id()
    ))
}

#[test]
fn quick_command_transitions_to_succeeded() {
    let mut board = HarnessBoard::default();
    let id = board.add("echo", "echo harness-alive", None, 10);
    assert_eq!(board.sessions[0].status, HarnessStatus::Pending);
    assert_eq!(board.sessions[0].stage, HarnessStage::Plan);

    board.start(&id, 20).expect("spawns");
    assert_eq!(board.sessions[0].status, HarnessStatus::Running);
    assert_eq!(board.sessions[0].started_at, Some(20));

    // Give the child a moment to exit before reaping.
    std::thread::sleep(std::time::Duration::from_millis(150));
    let changed = board.poll(30);
    assert_eq!(changed, vec![id.clone()]);
    assert_eq!(board.sessions[0].status, HarnessStatus::Succeeded);
    assert_eq!(board.sessions[0].exited_at, Some(30));
    assert!(!board.is_running(&id));

    let tail = board.log_tail(&id, 10);
    assert!(tail.iter().any(|line| line.contains("harness-alive")));
}

#[test]
fn failing_command_reports_the_exit_code() {
    let mut board = HarnessBoard::default();
    let id = board.add("fail", "exit 3", None, 10);
    board.start(&id, 20).expect("spawns");
    std::thread::sleep(std::time::Duration::from_millis(150));
    board.poll(30);
    assert_eq!(
        board.sessions[0].status,
        HarnessStatus::Failed {
            reason: "退出码 3".to_string()
        }
    );
}

#[test]
fn stop_kills_a_live_session() {
    let mut board = HarnessBoard::default();
    let id = board.add("long", "sleep 30", None, 10);
    board.start(&id, 20).expect("spawns");
    assert!(board.is_running(&id));

    board.stop(&id).expect("kills");
    assert_eq!(board.sessions[0].status, HarnessStatus::Stopped);
    assert!(!board.is_running(&id));
    assert!(board.stop(&id).is_err(), "already stopped");

    // The killed child must not linger.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(board.poll(40).is_empty());
}

#[test]
fn remove_kills_a_live_session_first() {
    let mut board = HarnessBoard::default();
    let id = board.add("victim", "sleep 30", None, 10);
    board.start(&id, 20).expect("spawns");
    assert!(board.remove(&id));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(board.poll(30).is_empty(), "child was killed, not reaped");
}

#[test]
fn stage_cycles_through_the_loop() {
    let mut board = HarnessBoard::default();
    let id = board.add("cycle", "true", None, 10);
    assert!(board.advance_stage(&id));
    assert_eq!(board.sessions[0].stage, HarnessStage::Implement);
    board.advance_stage(&id);
    board.advance_stage(&id);
    assert_eq!(board.sessions[0].stage, HarnessStage::Repair);
    board.advance_stage(&id);
    assert_eq!(board.sessions[0].stage, HarnessStage::Plan);
}

#[test]
fn review_findings_become_a_repair_checklist() {
    let mut board = HarnessBoard::default();
    let id = board.add("review", "true", None, 10);
    assert!(board.record_review(&id, "存在问题", "缺少测试\n\n日志未脱敏", 30));

    let count = board.plan_repairs(&id).expect("plan builds");
    assert_eq!(count, 2, "blank lines are dropped");
    assert_eq!(board.sessions[0].repair_steps[0].text, "缺少测试");
    assert!(!board.sessions[0].repair_steps[0].done);

    assert!(board.toggle_repair_step(&id, 1));
    assert!(board.sessions[0].repair_steps[1].done);
    // Toggling again flips it back off; the bool reports the mutation,
    // not the resulting state.
    assert!(board.toggle_repair_step(&id, 1));
    assert!(!board.sessions[0].repair_steps[1].done);

    // Planning without a review is a clean error.
    let other = board.add("no-review", "true", None, 11);
    assert!(board.plan_repairs(&other).is_err());
}

#[test]
fn board_round_trips_and_demotes_stale_runs() {
    let path = temp_path("roundtrip");
    let mut board = HarnessBoard::default();
    let id = board.add("persist", "echo hi", None, 10);
    board.start(&id, 20).expect("spawns");
    board.record_review(&id, "通过", "", 30);
    assert!(board.save_to(&path));
    // Kill the child so the test process does not leak it.
    let _ = board.stop(&id);

    let loaded = HarnessBoard::load_from(&path).expect("parses back");
    assert_eq!(loaded.sessions.len(), 1);
    assert_eq!(loaded.sessions[0].name, "persist");
    assert_eq!(
        loaded.sessions[0].review,
        Some(crate::harness::HarnessReview {
            verdict: "通过".to_string(),
            findings: Vec::new(),
            recorded_at: 30,
        })
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn loading_demotes_sessions_that_were_running_when_the_gui_died() {
    let path = temp_path("stale");
    let mut board = HarnessBoard::default();
    board.add("stale", "sleep 1", None, 10);
    board.sessions[0].status = HarnessStatus::Running;
    assert!(board.save_to(&path));

    let loaded = HarnessBoard::load_from(&path).expect("parses back");
    assert_eq!(
        loaded.sessions[0].status,
        HarnessStatus::Failed {
            reason: "GUI 退出导致运行中断".to_string()
        }
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn start_errors_are_actionable() {
    let mut board = HarnessBoard::default();
    let dup = board.add("dup", "sleep 30", None, 10);
    let bad_cwd = board.add(
        "bad-cwd",
        "true",
        Some(PathBuf::from("/nonexistent-cwd-xyz")),
        11,
    );

    board.start("missing", 20).expect_err("unknown id");
    board.start(&dup, 20).expect("first start");
    board.start(&dup, 21).expect_err("already running");
    assert!(board.start(&bad_cwd, 20).is_err(), "cwd does not exist");
    let _ = board.stop(&dup);
}

#[test]
fn timestamps_use_the_injected_clock() {
    assert!(now_secs() > 1_700_000_000);
}
