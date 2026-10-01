//! Tests for the scheduler engine.
#![allow(clippy::expect_used, clippy::panic)]

use super::ScheduleKind;
use super::Scheduler;
use super::humanize_interval;
use super::parse_delay_secs;
use crate::scheduler::now_secs;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "gui-scheduler-test-{}-{tag}.json",
        std::process::id()
    ))
}

#[test]
fn delay_shorthand_parses_all_units() {
    assert_eq!(parse_delay_secs("90s"), Some(90));
    assert_eq!(parse_delay_secs("30m"), Some(1_800));
    assert_eq!(parse_delay_secs("2h"), Some(7_200));
    assert_eq!(parse_delay_secs("1d"), Some(86_400));
    assert_eq!(parse_delay_secs("15"), Some(900));
    assert_eq!(parse_delay_secs(" 10m "), Some(600));
    assert_eq!(parse_delay_secs("abc"), None);
    assert_eq!(parse_delay_secs(""), None);
}

#[test]
fn interval_labels_humanize() {
    assert_eq!(humanize_interval(45), "45 秒");
    assert_eq!(humanize_interval(3_600), "1 小时");
    assert_eq!(humanize_interval(172_800), "2 天");
}

#[test]
fn one_shot_fires_once_then_disables() {
    let path = temp_path("once");
    let mut scheduler = Scheduler::default();
    let id = scheduler.add(
        "备份",
        ScheduleKind::Once { run_at: 1_000 },
        "run backup",
        0,
    );

    assert!(scheduler.due(999).is_empty(), "not yet due");
    let fired = scheduler.due(1_000);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].id, id);
    assert_eq!(fired[0].prompt, "run backup");
    assert!(scheduler.due(2_000).is_empty(), "never fires twice");
    assert!(!scheduler.tasks[0].enabled);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn recurring_fires_on_interval_pace() {
    let mut scheduler = Scheduler::default();
    scheduler.add(
        "巡检",
        ScheduleKind::Every { interval_secs: 300 },
        "patrol",
        0,
    );

    assert!(scheduler.due(299).is_empty());
    assert_eq!(scheduler.due(300).len(), 1);
    assert_eq!(scheduler.due(599).is_empty().then_some(true), Some(true));
    assert_eq!(scheduler.due(600).len(), 1);
}

#[test]
fn disabled_tasks_never_fire() {
    let mut scheduler = Scheduler::default();
    let id = scheduler.add("暂停", ScheduleKind::Once { run_at: 100 }, "x", 0);
    assert!(scheduler.set_enabled(&id, false));
    assert!(scheduler.due(1_000).is_empty());
    assert!(scheduler.set_enabled(&id, true));
    assert_eq!(scheduler.due(1_000).len(), 1);
}

#[test]
fn registry_round_trips_through_disk() {
    let path = temp_path("roundtrip");
    let mut scheduler = Scheduler::default();
    scheduler.add(
        "每日报表",
        ScheduleKind::Every {
            interval_secs: 86_400,
        },
        "generate report",
        50,
    );
    assert!(scheduler.save_to(&path));

    let loaded = Scheduler::load_from(&path).expect("parses back");
    assert_eq!(loaded.tasks.len(), 1);
    assert_eq!(loaded.tasks[0].name, "每日报表");
    assert_eq!(
        loaded.tasks[0].kind,
        ScheduleKind::Every {
            interval_secs: 86_400
        }
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn remove_drops_only_the_target() {
    let mut scheduler = Scheduler::default();
    let first = scheduler.add("a", ScheduleKind::Once { run_at: 10 }, "a", 0);
    let _second = scheduler.add("b", ScheduleKind::Once { run_at: 20 }, "b", 0);
    assert!(scheduler.remove(&first));
    assert_eq!(scheduler.tasks.len(), 1);
    assert_eq!(scheduler.tasks[0].name, "b");
    assert!(!scheduler.remove(&first), "already gone");
}

#[test]
fn now_secs_is_sane() {
    assert!(now_secs() > 1_700_000_000);
}
