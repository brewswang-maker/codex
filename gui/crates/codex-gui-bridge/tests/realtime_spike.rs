//! Manual protocol spike against a real `codex app-server`.
//!
//! Verifies the EXPERIMENTAL `thread/realtime/*` wire shape end to end:
//! our `start_params` / `audio_chunk` serialization must be accepted by
//! the real server (a structured refusal for a provider without a
//! realtime channel is a pass - the connection stays alive and later
//! calls still answer), any notifications the session emits must
//! classify cleanly, and the connection must survive the attempt
//! (`config/read` still answers afterwards).
//!
//! Ignored by default: it needs a built `codex` binary. Run it as
//!
//! ```text
//! CODEX_GUI_SPIKE_BINARY=/path/to/codex \
//!     cargo test -p codex-gui-bridge --test realtime_spike -- --ignored --nocapture
//! ```
//!
//! The dictation path only needs the plain websocket transport, so no
//! audio hardware is involved here; the microphone smoke lives in
//! `codex-gui-core` (`capture_smoke_records_the_default_device`).
#![allow(clippy::expect_used, clippy::panic)]

use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_gui_bridge::Flags;
use codex_gui_bridge::GuiEvent;
use std::path::PathBuf;
use tokio::time::Duration;
use tokio::time::timeout;

/// The sibling checkout's debug binary, used when the env var is unset.
const DEFAULT_BINARY: &str = "/home/brewswang/workshop/rust/codex/codex-rs/target/debug/codex";

#[tokio::test]
#[ignore = "spike: needs a real codex app-server binary and provider credentials"]
async fn realtime_wire_shape_against_a_real_app_server() {
    let binary = PathBuf::from(
        std::env::var("CODEX_GUI_SPIKE_BINARY").unwrap_or_else(|_| DEFAULT_BINARY.to_string()),
    );
    if !binary.is_file() {
        eprintln!("skip: codex binary not found at {}", binary.display());
        return;
    }

    let flags = Flags {
        program: binary,
        args: Vec::new(),
    };
    let (client, mut events) = codex_gui_bridge::start(flags)
        .await
        .expect("real app-server starts");

    let user_agent = client
        .call(|id| ClientRequest::Initialize {
            request_id: id,
            params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex-gui-spike".to_string(),
                    title: Some("Codex GUI realtime spike".to_string()),
                    version: "0.1.0".to_string(),
                },
                // The realtime RPCs live behind the experimental gate.
                capabilities: Some(InitializeCapabilities {
                    experimental_api: true,
                    ..InitializeCapabilities::default()
                }),
            },
        })
        .await
        .expect("initialize succeeds");
    eprintln!("initialize userAgent: {user_agent}");

    let cwd = std::env::current_dir().expect("cwd").display().to_string();
    let started = client
        .call(|id| ClientRequest::ThreadStart {
            request_id: id,
            params: ThreadStartParams {
                cwd: Some(cwd),
                ..ThreadStartParams::default()
            },
        })
        .await
        .expect("thread/start succeeds");
    let thread_id = started["thread"]["id"]
        .as_str()
        .expect("thread id present")
        .to_string();
    eprintln!("thread/start id: {thread_id}");

    // The heart of the spike: our exact start params against the real
    // server. A structured refusal (e.g. the configured provider has no
    // realtime channel) is a pass; a transport death or an unparseable
    // params complaint is a failure.
    match codex_gui_bridge::realtime::start(&client, &thread_id).await {
        Ok(()) => {
            eprintln!("thread/realtime/start: session opened");
            // One second of silence in 100 ms batches at the protocol's
            // 24 kHz mono PCM16, mimicking the UI's capture pump.
            let silence = vec![0_i16; 2_400];
            for batch in 1..=10 {
                match codex_gui_bridge::realtime::append_audio(
                    &client, &thread_id, &silence, 24_000,
                )
                .await
                {
                    Ok(()) => {}
                    Err(error) => {
                        eprintln!("thread/realtime/appendAudio batch {batch} error: {error}");
                        break;
                    }
                }
            }
            eprintln!("appendAudio: 10 batches of 100 ms silence done");
        }
        Err(error) => {
            eprintln!("thread/realtime/start refused: {error}");
            let message = error.to_string();
            assert!(
                message.contains("request failed"),
                "refusal must be a structured JSON-RPC error, got: {message}"
            );
        }
    }

    // While the session is live the server may report asynchronous
    // failures (a provider without a realtime websocket channel surfaces
    // here, not necessarily at start). Watch the same way the UI routes.
    drain_notifications(&mut events, Duration::from_millis(1_500)).await;

    // Whatever the start outcome, the connection must still be healthy:
    // stop answers (session or not) and a follow-up RPC still succeeds.
    let stop = codex_gui_bridge::realtime::stop(&client, &thread_id).await;
    eprintln!("thread/realtime/stop: {stop:?}");

    // The stop boundary is where farewell events (closed/error) land;
    // the UI drains here too before archiving.
    drain_notifications(&mut events, Duration::from_millis(1_500)).await;

    let config_read = client
        .call(|id| ClientRequest::ConfigRead {
            request_id: id,
            params: ConfigReadParams {
                include_layers: false,
                cwd: None,
            },
        })
        .await;
    assert!(
        config_read.is_ok(),
        "connection must survive the realtime attempt: {config_read:?}"
    );
    eprintln!("config/read: ok (connection survived)");

    // Archiving a thread that never ran a turn has no rollout to move,
    // so a `no rollout found` refusal is expected; best effort only.
    let archived = client
        .call(|id| ClientRequest::ThreadArchive {
            request_id: id,
            params: ThreadArchiveParams {
                thread_id: thread_id.clone(),
            },
        })
        .await;
    eprintln!("thread/archive (best effort): {archived:?}");

    eprintln!("spike complete");
}

/// Drains notifications for `window`, classifying anything realtime
/// through the same demux the UI uses.
async fn drain_notifications(events: &mut tokio::sync::mpsc::Receiver<GuiEvent>, window: Duration) {
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Some(event)) = timeout(
        deadline.saturating_duration_since(tokio::time::Instant::now()),
        events.recv(),
    )
    .await
    {
        if let GuiEvent::Notification(notification) = event
            && let Some(realtime_event) = codex_gui_bridge::realtime::classify(&notification)
        {
            eprintln!("realtime notification classified: {realtime_event:?}");
        }
    }
}
