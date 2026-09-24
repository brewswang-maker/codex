//! Connection assembly: spawn the backend and pump frames in both directions.

use crate::client::Client;
use crate::codec::decode_line;
use crate::error::Error;
use crate::events::Flags;
use crate::events::GuiEvent;
use crate::process::spawn;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;

/// Outbound frame queue depth; requests rarely exceed a handful in flight.
const OUTBOUND_CAPACITY: usize = 64;
/// Event queue depth; streaming deltas can burst well past 100 per second.
const EVENT_CAPACITY: usize = 1024;

/// Spawns the backend and returns its client handle plus the event stream.
///
/// The event stream ends only after [`GuiEvent::Disconnected`] has been
/// delivered; no automatic reconnect is attempted by design (a fresh GUI
/// session must not silently fork conversation state).
pub async fn start(flags: Flags) -> Result<(Client, mpsc::Receiver<GuiEvent>), Error> {
    let (mut child, stdin, stdout, stderr) = spawn(&flags).await?;

    let (outbound_tx, mut outbound_rx) = mpsc::channel::<JSONRPCMessage>(OUTBOUND_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel::<GuiEvent>(EVENT_CAPACITY);

    let client = Client::new(outbound_tx);

    // Writer: outbound queue -> child stdin, one JSON frame per line.
    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(message) = outbound_rx.recv().await {
            match crate::codec::encode_line(&message) {
                Ok(line) => {
                    if stdin.write_all(line.as_bytes()).await.is_err()
                        || stdin.write_all(b"\n").await.is_err()
                        || stdin.flush().await.is_err()
                    {
                        break;
                    }
                }
                Err(error) => tracing::warn!(?error, "dropping unencodable outbound frame"),
            }
        }
    });

    // Stderr: the backend's own logs, surfaced through tracing only.
    tokio::spawn(async move {
        let stderr = BufReader::new(stderr);
        let mut lines = stderr.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "codex_app_server", "{line}");
        }
    });

    // Reader: child stdout -> typed events and pending-request resolution.
    let reader_client = client.clone();
    tokio::spawn(async move {
        let stdout = BufReader::new(stdout);
        let mut lines = stdout.lines();
        let pump = event_tx.clone();

        while let Ok(Some(line)) = lines.next_line().await {
            if dispatch_line(&line, &reader_client, &pump).await.is_err() {
                break;
            }
        }

        // The pipe is gone: fail every outstanding request.
        reader_client.fail_all(Error::Closed);
        let status = child.wait().await;
        let _ignored = pump
            .send(GuiEvent::Disconnected(status.map_or_else(
                |_| "unknown".to_string(),
                |status| status.to_string(),
            )))
            .await;
    });

    Ok((client, event_rx))
}

/// Classifies one inbound wire line and routes it. Returns `Err` when the
/// event channel closed, meaning nobody is listening anymore.
///
/// The `to_value` hops are invariants of the wire codec: a frame that already
/// decoded as `JSONRPCMessage` always re-serializes.
#[allow(clippy::expect_used)]
async fn dispatch_line(
    line: &str,
    client: &Client,
    events: &mpsc::Sender<GuiEvent>,
) -> Result<(), Error> {
    let message = match decode_line(line) {
        Ok(message) => message,
        Err(error) => {
            tracing::warn!(%error, line, "undecodable frame from app-server");
            return send_event(events, GuiEvent::Undecodable(line.to_string())).await;
        }
    };

    match message {
        JSONRPCMessage::Request(request) => {
            // Server-initiated request (approval, elicitation...): retype it
            // through the tagged protocol enum for safe handling upstream.
            match serde_json::from_value::<ServerRequest>(
                serde_json::to_value(&request).expect("request frame serializes"),
            ) {
                Ok(server_request) => {
                    send_event(events, GuiEvent::ServerRequest(server_request)).await
                }
                Err(error) => {
                    tracing::warn!(%error, "unknown server request; replying with an error");
                    let _ignored = client
                        .respond_error_for_id(
                            request.id,
                            -32601,
                            format!("unsupported method {}", request.method),
                        )
                        .await;
                    Ok(())
                }
            }
        }
        JSONRPCMessage::Notification(notification) => {
            match serde_json::from_value::<ServerNotification>(
                serde_json::to_value(&notification).expect("notification frame serializes"),
            ) {
                Ok(server_notification) => {
                    send_event(events, GuiEvent::Notification(server_notification)).await
                }
                Err(error) => {
                    tracing::warn!(%error, method = %notification.method, "unknown notification");
                    Ok(())
                }
            }
        }
        JSONRPCMessage::Response(response) => {
            if !client.resolve(&response.id, Ok(response.result)) {
                tracing::warn!(id = %response.id, "response with no pending request");
            }
            Ok(())
        }
        JSONRPCMessage::Error(error) => {
            let resolved = client.resolve(&error.id, Err(Error::from(&error.error)));
            if !resolved {
                tracing::warn!(id = %error.id, "error frame with no pending request");
            }
            Ok(())
        }
    }
}

async fn send_event(events: &mpsc::Sender<GuiEvent>, event: GuiEvent) -> Result<(), Error> {
    events.send(event).await.map_err(|_| Error::Closed)
}
