//! Wire-shape and demux tests for the realtime wrappers.
#![allow(clippy::expect_used, clippy::panic)]

use super::realtime::{RealtimeEvent, audio_chunk, classify, start_params};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadRealtimeClosedNotification;
use codex_app_server_protocol::ThreadRealtimeErrorNotification;
use codex_app_server_protocol::ThreadRealtimeItemAddedNotification;
use codex_app_server_protocol::ThreadRealtimeStartedNotification;
use codex_app_server_protocol::ThreadRealtimeTranscriptDeltaNotification;
use codex_app_server_protocol::ThreadRealtimeTranscriptDoneNotification;
use codex_protocol::protocol::RealtimeConversationVersion;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn start_params_pin_the_dictation_wire_shape() {
    let request = ClientRequest::ThreadRealtimeStart {
        request_id: RequestId::Integer(3),
        params: start_params("thr_voice"),
    };
    let value = serde_json::to_value(&request).expect("serialize start request");

    assert_eq!(value["method"], json!("thread/realtime/start"));
    assert_eq!(value["params"]["threadId"], json!("thr_voice"));
    assert_eq!(value["params"]["outputModality"], json!("text"));
    // Text modality requires realtime v2 server-side; the params pin it
    // rather than inheriting the user's configured version.
    assert_eq!(value["params"]["version"], json!("v2"));
    assert_eq!(value["params"]["transport"], json!({ "type": "websocket" }));
    // Dictation must never auto-forward handoffs to the codex agent.
    assert_eq!(value["params"]["clientManagedHandoffs"], json!(true));
    assert_eq!(value["params"]["includeStartupContext"], json!(false));
}

#[test]
fn audio_chunk_encodes_mono_pcm16_as_base64() {
    let chunk = audio_chunk(&[1, -1, i16::MAX], 24_000);

    assert_eq!(chunk.sample_rate, 24_000);
    assert_eq!(chunk.num_channels, 1);
    assert_eq!(chunk.samples_per_channel, Some(3));
    assert_eq!(chunk.item_id, None);
    let bytes = BASE64_STANDARD
        .decode(&chunk.data)
        .expect("chunk data decodes");
    // Little-endian PCM16: 1, -1, 32767.
    assert_eq!(bytes, vec![1, 0, 0xFF, 0xFF, 0xFF, 0x7F]);
}

#[test]
fn classify_maps_the_transcript_family() {
    assert_eq!(classify(&started()), Some(RealtimeEvent::Started));
    assert_eq!(
        classify(&transcript_delta("user", "你好")),
        Some(RealtimeEvent::TranscriptDelta {
            role: "user".to_string(),
            delta: "你好".to_string(),
        })
    );
    assert_eq!(
        classify(&transcript_done("user", "你好，世界")),
        Some(RealtimeEvent::TranscriptDone {
            role: "user".to_string(),
            text: "你好，世界".to_string(),
        })
    );
    assert_eq!(
        classify(&ServerNotification::ThreadRealtimeError(
            ThreadRealtimeErrorNotification {
                thread_id: "t".to_string(),
                message: "boom".to_string(),
            }
        )),
        Some(RealtimeEvent::Failed("boom".to_string()))
    );
    assert_eq!(
        classify(&ServerNotification::ThreadRealtimeClosed(
            ThreadRealtimeClosedNotification {
                thread_id: "t".to_string(),
                reason: Some("done".to_string()),
            }
        )),
        Some(RealtimeEvent::Closed(Some("done".to_string())))
    );
}

#[test]
fn classify_ignores_other_traffic() {
    // Family members the dictation flow does not react to...
    let item_added =
        ServerNotification::ThreadRealtimeItemAdded(ThreadRealtimeItemAddedNotification {
            thread_id: "t".to_string(),
            item: json!({ "type": "input_audio_buffer.speech_started" }),
        });
    assert_eq!(classify(&item_added), None);
    // ...and notifications entirely outside the family.
    let turn_started: ServerNotification = serde_json::from_value(json!({
        "method": "turn/started",
        "params": {
            "threadId": "t",
            "turn": {
                "id": "turn-1",
                "items": [],
                "status": "inProgress",
                "error": null,
                "startedAt": null,
                "completedAt": null,
                "durationMs": null
            }
        }
    }))
    .expect("turn/started decodes");
    assert_eq!(classify(&turn_started), None);
}

fn started() -> ServerNotification {
    ServerNotification::ThreadRealtimeStarted(ThreadRealtimeStartedNotification {
        thread_id: "t".to_string(),
        realtime_session_id: Some("sess".to_string()),
        version: RealtimeConversationVersion::V2,
    })
}

fn transcript_delta(role: &str, delta: &str) -> ServerNotification {
    ServerNotification::ThreadRealtimeTranscriptDelta(ThreadRealtimeTranscriptDeltaNotification {
        thread_id: "t".to_string(),
        role: role.to_string(),
        delta: delta.to_string(),
    })
}

fn transcript_done(role: &str, text: &str) -> ServerNotification {
    ServerNotification::ThreadRealtimeTranscriptDone(ThreadRealtimeTranscriptDoneNotification {
        thread_id: "t".to_string(),
        role: role.to_string(),
        text: text.to_string(),
    })
}
