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

/// A scheduler whose mutations persist to a temp file, never to the
/// user's real `~/.codex/gui-schedule.json`.
fn isolated_scheduler(tag: &str) -> Scheduler {
    let mut scheduler = Scheduler::default();
    scheduler.set_save_path(temp_path(tag));
    scheduler
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
    let mut scheduler = isolated_scheduler("once");
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
    let mut scheduler = isolated_scheduler("recurring");
    scheduler.add_in(
        "巡检",
        ScheduleKind::Every { interval_secs: 300 },
        "patrol",
        Some("/tmp/project-a".into()),
        0,
    );

    assert!(scheduler.due(299).is_empty());
    assert_eq!(scheduler.due(300).len(), 1);
    assert_eq!(scheduler.due(599).is_empty().then_some(true), Some(true));
    assert_eq!(scheduler.due(600).len(), 1);
    let _ = std::fs::remove_file(temp_path("recurring"));
}

#[test]
fn project_binding_round_trips_and_old_files_default_none() {
    let path = temp_path("project");
    let mut scheduler = isolated_scheduler("project");
    scheduler.add_in(
        "绑定",
        ScheduleKind::Once { run_at: 10 },
        "echo",
        Some("/work/demo".into()),
        0,
    );
    let bound = scheduler.tasks[0].clone();
    assert_eq!(bound.project.as_deref(), Some("/work/demo"));

    // A legacy file without the project field still parses (None).
    let legacy = r#"{"tasks":[{"id":"t","name":"n","kind":{"type":"once","runAt":5},"prompt":"p","enabled":true,"lastRun":null,"createdAt":0}]}"#;
    std::fs::write(&path, legacy).expect("write legacy");
    let loaded = Scheduler::load_from(&path).expect("legacy parses");
    assert_eq!(loaded.tasks[0].project, None);

    // The bound task survives a save/load round trip.
    assert!(scheduler.save_to(&path));
    let reloaded = Scheduler::load_from(&path).expect("round trip");
    assert_eq!(reloaded.tasks, scheduler.tasks);
    let _ = std::fs::remove_file(&path);
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().expect("temp parent"))
        .expect("read temp parent")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("project.json.tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left behind: {leftovers:?}"
    );
}

#[test]
fn ids_remain_unique_after_a_same_second_removal() {
    let mut scheduler = isolated_scheduler("unique-ids");
    let first = scheduler.add("first", ScheduleKind::Once { run_at: 10 }, "x", 7);
    assert!(scheduler.remove(&first));
    let second = scheduler.add("second", ScheduleKind::Once { run_at: 10 }, "x", 7);

    assert_ne!(first, second);
    assert_eq!(scheduler.tasks.len(), 1);
    assert_eq!(scheduler.tasks[0].id, second);
    let reloaded = Scheduler::load_from(&temp_path("unique-ids")).expect("reload");
    let third = scheduler.add("third", ScheduleKind::Once { run_at: 10 }, "x", 7);
    assert_eq!(reloaded.next_id, 2);
    assert_eq!(third, "task-7-3");
    let _ = std::fs::remove_file(temp_path("unique-ids"));
}

#[test]
fn disabled_tasks_never_fire() {
    let mut scheduler = isolated_scheduler("disabled");
    let id = scheduler.add("暂停", ScheduleKind::Once { run_at: 100 }, "x", 0);
    assert!(scheduler.set_enabled(&id, false));
    assert!(scheduler.due(1_000).is_empty());
    assert!(scheduler.set_enabled(&id, true));
    assert_eq!(scheduler.due(1_000).len(), 1);
}

#[test]
fn registry_round_trips_through_disk() {
    let path = temp_path("roundtrip");
    let mut scheduler = isolated_scheduler("roundtrip");
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
    let mut scheduler = isolated_scheduler("remove");
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
