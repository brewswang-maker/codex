#![allow(clippy::expect_used, clippy::panic)]

use super::parse;
use codex_gui_bridge::Flags;
use pretty_assertions::assert_eq;

#[test]
fn parse_defaults_to_gpu_rendering() {
    let parsed = parse(std::iter::empty()).expect("empty argv parses");

    assert_eq!(
        parsed,
        super::GuiArgs {
            bridge: Flags::default_app_server(),
            software_rendering: false,
        }
    );
}

#[test]
fn parse_software_rendering_flag() {
    let parsed = parse(["--software-rendering".to_string()]).expect("flag parses");

    assert!(parsed.software_rendering);
}

#[test]
fn parse_combines_with_backend_flags() {
    let parsed = parse([
        "--app-server".to_string(),
        "/tmp/mock-server".to_string(),
        "--app-server-arg".to_string(),
        "--verbose".to_string(),
        "--software-rendering".to_string(),
    ])
    .expect("combined argv parses");

    assert_eq!(
        parsed.bridge.program,
        std::path::PathBuf::from("/tmp/mock-server")
    );
    assert_eq!(parsed.bridge.args, vec!["--verbose".to_string()]);
    assert!(parsed.software_rendering);
}

#[test]
fn parse_rejects_typoed_software_rendering() {
    let error = parse(["--software-render".to_string()]).expect_err("typo is rejected");

    assert!(error.contains("unknown argument"), "{error}");
}
