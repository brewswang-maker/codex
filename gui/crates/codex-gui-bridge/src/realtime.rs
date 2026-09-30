//! EXPERIMENTAL `thread/realtime/*` surface backing the composer's voice
//! input.
//!
//! Wraps the three calls the dictation flow needs - `start`, `appendAudio`,
//! `stop` - and demultiplexes the `thread/realtime/*` notification family
//! into [`RealtimeEvent`] so the UI matches one small enum instead of the
//! protocol variants. Everything here lives behind upstream's EXPERIMENTAL
//! marker (`codex-rs/app-server-protocol` v2 realtime), so the wrapping is
//! deliberately thin and all wire-shape decisions stay in one place:
//!
//! - text output only (`output_modality = Text`), which the server
//!   accepts on realtime v2 sessions,
//! - the websocket transport (no WebRTC negotiation from the GUI),
//! - client-managed handoffs: the realtime model never auto-forwards to the
//!   codex agent, so dictation cannot start background turns,
//! - no startup context: the throwaway dictation thread carries no project
//!   preamble.

use crate::client::Client;
use crate::error::Error;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadRealtimeAppendAudioParams;
use codex_app_server_protocol::ThreadRealtimeAudioChunk;
use codex_app_server_protocol::ThreadRealtimeStartParams;
use codex_app_server_protocol::ThreadRealtimeStartTransport;
use codex_app_server_protocol::ThreadRealtimeStopParams;
use codex_protocol::protocol::RealtimeConversationVersion;
use codex_protocol::protocol::RealtimeOutputModality;

/// The wire `role` value for user transcription.
pub const USER_ROLE: &str = "user";

/// The `thread/realtime/start` params for a dictation session.
pub fn start_params(thread_id: &str) -> ThreadRealtimeStartParams {
    ThreadRealtimeStartParams {
        thread_id: thread_id.to_string(),
        client_managed_handoffs: Some(true),
        // Text output requires realtime v2 ("text realtime output modality
        // requires realtime v2" server-side); pin it instead of inheriting
        // the user's configured realtime version.
        version: Some(RealtimeConversationVersion::V2),
        output_modality: RealtimeOutputModality::Text,
        transport: Some(ThreadRealtimeStartTransport::Websocket),
        include_startup_context: Some(false),
        delegation_ack_filler: None,
        flush_transcript_tail_on_session_end: None,
        codex_responses_as_items: None,
        codex_response_item_prefix: None,
        codex_response_handoff_mode: None,
        backend_reasoning_status: false,
        codex_response_handoff_channel_prefixes: None,
        model: None,
        initial_items: None,
        realtime_start_instructions: None,
        realtime_end_instructions: None,
        prompt: None,
        realtime_session_id: None,
        voice: None,
    }
}

/// The `thread/realtime/appendAudio` chunk for one PCM16 slice.
pub fn audio_chunk(pcm16: &[i16], sample_rate: u32) -> ThreadRealtimeAudioChunk {
    let mut bytes = Vec::with_capacity(pcm16.len() * 2);
    for sample in pcm16 {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    ThreadRealtimeAudioChunk {
        data: BASE64_STANDARD.encode(bytes),
        sample_rate,
        num_channels: 1,
        samples_per_channel: Some(pcm16.len() as u32),
        item_id: None,
    }
}

/// Opens the realtime session on `thread_id`.
pub async fn start(client: &Client, thread_id: &str) -> Result<(), Error> {
    client
        .call(|request_id| ClientRequest::ThreadRealtimeStart {
            request_id,
            params: start_params(thread_id),
        })
        .await
        .map(|_response| ())
}

/// Appends one mono PCM16 slice to the live session.
pub async fn append_audio(
    client: &Client,
    thread_id: &str,
    pcm16: &[i16],
    sample_rate: u32,
) -> Result<(), Error> {
    let audio = audio_chunk(pcm16, sample_rate);
    client
        .call(|request_id| ClientRequest::ThreadRealtimeAppendAudio {
            request_id,
            params: ThreadRealtimeAppendAudioParams {
                thread_id: thread_id.to_string(),
                audio,
            },
        })
        .await
        .map(|_response| ())
}

/// Ends the realtime session on `thread_id`.
pub async fn stop(client: &Client, thread_id: &str) -> Result<(), Error> {
    client
        .call(|request_id| ClientRequest::ThreadRealtimeStop {
            request_id,
            params: ThreadRealtimeStopParams {
                thread_id: thread_id.to_string(),
            },
        })
        .await
        .map(|_response| ())
}

/// The subset of `thread/realtime/*` notifications the dictation UI reacts
/// to, demultiplexed from [`ServerNotification`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeEvent {
    /// The session was accepted and is running.
    Started,
    /// Live transcription text changed (`role` is `user` or `assistant`).
    TranscriptDelta { role: String, delta: String },
    /// A transcript part was finalized with its complete text.
    TranscriptDone { role: String, text: String },
    /// The session hit an error; the payload is the server message.
    Failed(String),
    /// The transport closed; the payload is the optional reason.
    Closed(Option<String>),
}

/// Demultiplexes one notification; `None` for anything outside the realtime
/// family (or family members the dictation flow ignores, like output audio).
pub fn classify(notification: &ServerNotification) -> Option<RealtimeEvent> {
    match notification {
        ServerNotification::ThreadRealtimeStarted(_) => Some(RealtimeEvent::Started),
        ServerNotification::ThreadRealtimeTranscriptDelta(delta) => {
            Some(RealtimeEvent::TranscriptDelta {
                role: delta.role.clone(),
                delta: delta.delta.clone(),
            })
        }
        ServerNotification::ThreadRealtimeTranscriptDone(done) => {
            Some(RealtimeEvent::TranscriptDone {
                role: done.role.clone(),
                text: done.text.clone(),
            })
        }
        ServerNotification::ThreadRealtimeError(error) => {
            Some(RealtimeEvent::Failed(error.message.clone()))
        }
        ServerNotification::ThreadRealtimeClosed(closed) => {
            Some(RealtimeEvent::Closed(closed.reason.clone()))
        }
        _ => None,
    }
}
