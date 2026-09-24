//! `compose_inputs` wire-shape coverage.

use super::*;
use codex_app_server_protocol::SkillScope;
use pretty_assertions::assert_eq;

fn board(rows: &[(&str, bool)]) -> SkillsBoard {
    SkillsBoard {
        rows: rows
            .iter()
            .map(|(name, enabled)| SkillRow {
                name: (*name).to_string(),
                description: String::new(),
                scope: SkillScope::User,
                path: std::path::PathBuf::from(format!("/skills/{name}")),
                enabled: *enabled,
            })
            .collect(),
        picker_open: false,
    }
}

#[test]
fn composes_images_text_and_mentioned_skills() {
    let skills = board(&[("figma", true), ("review", false)]);

    let inputs = compose_inputs(
        "check $figma and $review",
        &["/tmp/shot.png".to_string().into()],
        &skills,
    );

    assert_eq!(
        inputs,
        vec![
            UserInput::LocalImage {
                detail: None,
                path: "/tmp/shot.png".to_string().into(),
            },
            UserInput::Text {
                text: "check $figma and $review".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Skill {
                name: "figma".to_string(),
                path: "/skills/figma".to_string().into(),
            },
        ],
    );
}

#[test]
fn plain_text_yields_a_single_text_item() {
    let skills = board(&[]);

    let inputs = compose_inputs("fix the bug", &[], &skills);

    assert_eq!(
        inputs,
        vec![UserInput::Text {
            text: "fix the bug".to_string(),
            text_elements: Vec::new(),
        }],
    );
}

#[test]
fn disabled_or_unknown_mentions_stay_text_only() {
    let skills = board(&[("review", false), ("figma", true)]);

    let inputs = compose_inputs("$nope then $review then $figma", &[], &skills);

    assert_eq!(
        inputs,
        vec![
            UserInput::Text {
                text: "$nope then $review then $figma".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Skill {
                name: "figma".to_string(),
                path: "/skills/figma".to_string().into(),
            },
        ],
    );
}

#[test]
fn repeated_mentions_deduplicate_in_first_mention_order() {
    let skills = board(&[("figma", true), ("sample", true)]);

    let inputs = compose_inputs("$figma then $sample then $figma", &[], &skills);

    let mentioned: Vec<_> = inputs
        .iter()
        .filter_map(|input| match input {
            UserInput::Skill { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(mentioned, vec!["figma", "sample"]);
}

#[test]
fn mention_names_stop_at_non_name_characters() {
    let skills = board(&[("code-review", true)]);

    let inputs = compose_inputs("run $code-review, twice ($code-review)", &[], &skills);

    let mentioned: Vec<_> = inputs
        .iter()
        .filter_map(|input| match input {
            UserInput::Skill { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(mentioned, vec!["code-review"]);
}

#[test]
fn dollar_signs_without_names_are_ignored() {
    let skills = board(&[("figma", true)]);

    let inputs = compose_inputs("pay 5$ and $ figma", &[], &skills);

    assert_eq!(
        inputs,
        vec![UserInput::Text {
            text: "pay 5$ and $ figma".to_string(),
            text_elements: Vec::new(),
        }],
    );
}
