//! Unit coverage of the chat-pane helpers (relative timestamps, durations).
#![allow(clippy::expect_used, clippy::panic)]

use super::fmt_tokens;
use super::format_duration;
use super::relative_time;

#[test]
fn relative_time_buckets_by_magnitude() {
    let now = 1_000_000;

    assert_eq!(relative_time(now, now), "just now");
    assert_eq!(relative_time(now - 30, now), "just now");
    assert_eq!(relative_time(now - 60, now), "1m ago");
    assert_eq!(relative_time(now - 45 * 60, now), "45m ago");
    assert_eq!(relative_time(now - 3 * 3600, now), "3h ago");
    assert_eq!(relative_time(now - 2 * 86_400, now), "2d ago");
}

#[test]
fn clock_skew_still_reads_as_just_now() {
    // A timestamp slightly in the future must not render a negative age.
    assert_eq!(relative_time(1_000_050, 1_000_000), "just now");
}

#[test]
fn token_counts_use_compact_units() {
    assert_eq!(fmt_tokens(940), "940");
    assert_eq!(fmt_tokens(1_000), "1.0k");
    assert_eq!(fmt_tokens(12_400), "12.4k");
    assert_eq!(fmt_tokens(1_200_000), "1.2M");
}

#[test]
fn durations_flip_from_millis_to_seconds() {
    assert_eq!(format_duration(5), "5ms");
    assert_eq!(format_duration(940), "940ms");
    assert_eq!(format_duration(1_200), "1.2s");
    assert_eq!(format_duration(14_000), "14.0s");
}

#[test]
fn transcript_rows_collapses_consecutive_finished_commands() {
    use super::TranscriptRow;
    use super::transcript_rows;
    use codex_gui_core::Entry;

    let command = |id: &str, exit_code: Option<i32>| Entry::CommandExecution {
        id: String::from(id),
        command: String::from("ls"),
        output: String::new(),
        exit_code,
        duration_ms: Some(1),
    };
    let user = Entry::UserMessage {
        id: String::from("u1"),
        text: String::from("hi"),
        images: Vec::new(),
    };

    // A run of three finished commands plus a running tail stays separate.
    let entries = vec![
        user.clone(),
        command("c1", Some(0)),
        command("c2", Some(0)),
        command("c3", Some(0)),
        command("c4", None),
        user.clone(),
        command("c5", Some(0)),
    ];
    let rows = transcript_rows(&entries);
    assert_eq!(rows.len(), 5);
    assert!(matches!(rows[0], TranscriptRow::Entry(_)));
    assert!(
        matches!(rows[1], TranscriptRow::Group(1, 4)),
        "run of c1..c3"
    );
    assert!(
        matches!(rows[2], TranscriptRow::Entry(_)),
        "running c4 alone"
    );
    assert!(matches!(rows[3], TranscriptRow::Entry(_)));
    assert!(
        matches!(rows[4], TranscriptRow::Entry(_)),
        "single c5 never grouped"
    );
}

#[test]
fn step_search_hit_wraps_in_both_directions() {
    use crate::chat::step_search_hit;
    use crate::message::Message;
    use crate::state::State;
    use codex_gui_bridge::Flags;

    let mut state = State::new(Flags::default_app_server());
    // Seed three matching entries through the public apply path.
    let notification = |id: &str| {
        codex_app_server_protocol::ServerNotification::ItemStarted(
            serde_json::from_value(serde_json::json!({
                "item": {"type": "agentMessage", "id": id, "text": "target needle"},
                "threadId": "t",
                "turnId": "r",
                "startedAtMs": 1
            }))
            .expect("notification"),
        )
    };
    state.transcript.apply(&notification("a"));
    state.transcript.apply(&notification("b"));
    state.transcript.apply(&notification("c"));

    state.chat_search.query = String::from("needle");
    assert_eq!(state.chat_search.hit, 0);

    step_search_hit(&mut state, 1);
    assert_eq!(state.chat_search.hit, 1);
    step_search_hit(&mut state, 1);
    assert_eq!(state.chat_search.hit, 2);
    step_search_hit(&mut state, 1);
    assert_eq!(state.chat_search.hit, 0, "forward wraps");

    step_search_hit(&mut state, -1);
    assert_eq!(state.chat_search.hit, 2, "backward wraps");

    // The update path also routes through the same helper.
    let _ = crate::update::update(
        &mut state,
        Message::ChatSearchQueryChanged(String::from("nomatch")),
    );
    assert_eq!(state.chat_search.hit, 0);
    let _ = crate::update::update(&mut state, Message::ChatSearchNext);
    assert_eq!(state.chat_search.hit, 0, "no hits keeps the cursor reset");
}

#[test]
fn only_the_latest_agent_message_renders_the_action_bar() {
    use super::agent_message_is_latest;
    use codex_gui_core::Entry;

    // Turn 1 streams two segments around a command run, then the user
    // steers mid-task (u2) and the model replies once more. Only the
    // final reply renders the action bar: neither the earlier streaming
    // segments nor the pre-steer reply may become end-of-task markers.
    let entries = vec![
        Entry::UserMessage {
            id: String::from("u1"),
            text: String::new(),
            images: Vec::new(),
        },
        Entry::AgentMessage {
            id: String::from("a1"),
            text: String::new(),
        },
        Entry::CommandExecution {
            id: String::from("c1"),
            command: String::new(),
            output: String::new(),
            exit_code: Some(0),
            duration_ms: Some(5),
        },
        Entry::AgentMessage {
            id: String::from("a2"),
            text: String::new(),
        },
        Entry::UserMessage {
            id: String::from("u2"),
            text: String::new(),
            images: Vec::new(),
        },
        Entry::AgentMessage {
            id: String::from("a3"),
            text: String::new(),
        },
        Entry::AgentMessage {
            id: String::from("a4"),
            text: String::new(),
        },
    ];

    assert!(!agent_message_is_latest(&entries, 1), "pre-command segment");
    assert!(!agent_message_is_latest(&entries, 3), "pre-steer reply");
    assert!(!agent_message_is_latest(&entries, 5), "steered segment");
    assert!(agent_message_is_latest(&entries, 6), "final reply");
}

#[test]
fn agent_message_is_latest_on_the_live_streaming_tail() {
    use super::agent_message_is_latest;
    use codex_gui_core::Entry;

    // The latest turn is still streaming: its in-flight message is the
    // tail, so it renders the action bar while its predecessors do not.
    let entries = vec![
        Entry::UserMessage {
            id: String::from("u1"),
            text: String::new(),
            images: Vec::new(),
        },
        Entry::AgentMessage {
            id: String::from("a1"),
            text: String::new(),
        },
        Entry::AgentMessage {
            id: String::from("a2"),
            text: String::new(),
        },
    ];

    assert!(!agent_message_is_latest(&entries, 1));
    assert!(agent_message_is_latest(&entries, 2));
}
