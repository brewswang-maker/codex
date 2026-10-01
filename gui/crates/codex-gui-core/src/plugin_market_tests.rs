//! Tests for the plugin market registry (no real dynamic libraries).
#![allow(clippy::expect_used, clippy::panic)]

use super::PluginMarket;
use super::PluginRecord;
use super::find_plugin_binary;
use super::plugin_binary_name;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("gui-plugin-test-{}-{tag}", std::process::id()))
}

#[test]
fn installing_a_non_library_fails_with_a_load_error() {
    let dir = temp_path("notalib");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fake = dir.join("fake.so");
    std::fs::write(&fake, b"definitely not an elf").expect("write fake");

    let mut market = PluginMarket::default();
    let outcome = market.install_from_path(&fake, 10);
    assert!(outcome.is_err(), "load must fail: {outcome:?}");
    assert!(market.plugins.is_empty());

    let missing = dir.join("missing.so");
    assert!(market.install_from_path(&missing, 10).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn registry_round_trips_through_disk() {
    let path = temp_path("registry").with_extension("json");
    let mut market = PluginMarket::default();
    market.plugins.push(PluginRecord {
        id: "demo".to_string(),
        name: "demo".to_string(),
        version: "0.1.0".to_string(),
        source: "/tmp/demo/plugin.so".to_string(),
        dir: PathBuf::from("/tmp/demo"),
        enabled: true,
        skills: Vec::new(),
        commands: Vec::new(),
        last_error: None,
        installed_at: 10,
    });
    market.save_to(&path);

    let loaded = PluginMarket::load_from(&path).expect("parses back");
    assert_eq!(loaded.plugins.len(), 1);
    assert_eq!(loaded.plugins[0].id, "demo");
    assert_eq!(loaded.plugins[0].version, "0.1.0");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn enable_uninstall_and_rollback_validate_ids() {
    let mut market = PluginMarket::default();
    assert!(!market.set_enabled("ghost", true));
    assert!(market.uninstall("ghost").is_err());
    assert!(market.rollback("ghost").is_err());

    market.plugins.push(PluginRecord {
        id: "plain".to_string(),
        name: "plain".to_string(),
        version: "1.0.0".to_string(),
        source: String::new(),
        dir: temp_path("plain-empty"),
        enabled: true,
        skills: Vec::new(),
        commands: Vec::new(),
        last_error: None,
        installed_at: 1,
    });
    assert!(market.rollback("plain").is_err(), "no backup kept");
    assert!(market.set_enabled("plain", false));
    assert!(!market.plugins[0].enabled);
    assert!(market.uninstall("plain").is_ok());
    assert!(market.plugins.is_empty());
}

#[test]
fn invoking_an_unknown_or_disabled_plugin_fails_cleanly() {
    let mut market = PluginMarket::default();
    market.plugins.push(PluginRecord {
        id: "off".to_string(),
        name: "off".to_string(),
        version: "1.0.0".to_string(),
        source: String::new(),
        dir: temp_path("off-empty"),
        enabled: false,
        skills: Vec::new(),
        commands: Vec::new(),
        last_error: None,
        installed_at: 1,
    });
    assert!(market.invoke_skill("missing", "s", "").is_err());
    let err = market
        .invoke_skill("off", "s", "")
        .expect_err("disabled refuses");
    assert!(err.contains("已禁用"), "{err}");
}

#[test]
fn binary_finder_locates_the_plugin_file() {
    let dir = temp_path("finder");
    let nested = dir.join("target").join("release");
    std::fs::create_dir_all(&nested).expect("temp dirs");
    let binary = nested.join(plugin_binary_name());
    std::fs::write(&binary, b"elf-ish").expect("write binary");

    assert_eq!(find_plugin_binary(&dir), Some(binary));

    let empty = temp_path("finder-empty");
    std::fs::create_dir_all(&empty).expect("temp dir");
    assert_eq!(find_plugin_binary(&empty), None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&empty);
}
