//! Tests for the SDK surface: the entry macro and trait defaults.
#![allow(clippy::expect_used)]

use crate::Plugin;
use crate::SkillDescriptor;
use crate::register_plugin;
use pretty_assertions::assert_eq;

/// A demo plugin exercising the whole trait surface.
#[derive(Default)]
struct DemoTest;

impl Plugin for DemoTest {
    fn name(&self) -> &str {
        "demo-test"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn skills(&self) -> Vec<SkillDescriptor> {
        vec![SkillDescriptor {
            id: "demo.hello".to_string(),
            title: "Hello".to_string(),
            description: "greets".to_string(),
        }]
    }

    fn invoke_skill(&self, skill_id: &str, _args: &str) -> Result<String, String> {
        if skill_id == "demo.hello" {
            Ok("hi!".to_string())
        } else {
            Err(format!("unknown skill {skill_id}"))
        }
    }
}

register_plugin!(DemoTest);

#[test]
fn entry_hands_back_a_working_plugin() {
    let ptr = codex_plugin_entry();
    let plugin = unsafe { Box::from_raw(ptr) };
    assert_eq!(plugin.name(), "demo-test");
    assert_eq!(plugin.version(), "0.1.0");
    assert_eq!(plugin.skills().len(), 1);
    assert_eq!(plugin.skills()[0].id, "demo.hello");
    assert!(plugin.commands().is_empty());
    assert_eq!(plugin.invoke_skill("demo.hello", ""), Ok("hi!".to_string()));
    assert!(plugin.invoke_skill("nope", "").is_err());
    assert_eq!(
        plugin.run_command("any", ""),
        Err("this plugin exposes no commands".to_string())
    );
}

#[test]
fn defaults_are_inert_but_safe() {
    #[derive(Default)]
    struct Bare;

    impl Plugin for Bare {
        fn name(&self) -> &str {
            "bare"
        }

        fn version(&self) -> &str {
            "0.0.1"
        }
    }

    let bare = Bare;
    assert!(bare.skills().is_empty());
    assert!(bare.commands().is_empty());
    assert!(bare.invoke_skill("x", "").is_err());
    assert!(bare.run_command("x", "").is_err());
}
