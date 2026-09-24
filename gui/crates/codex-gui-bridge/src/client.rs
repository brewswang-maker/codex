//! Request/response correlation against the app-server.

use crate::error::Error;
use codex_app_server_protocol::{
    ClientRequest, JSONRPCError, JSONRPCMessage, JSONRPCResponse, RequestId, ServerRequest,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicI64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

/// Locks the pending map, recovering from poisoning: a panic in one waiter
/// must not take down every other in-flight request.
fn lock_pending(
    pending: &Mutex<HashMap<String, oneshot::Sender<Result<Value, Error>>>>,
) -> std::sync::MutexGuard<'_, HashMap<String, oneshot::Sender<Result<Value, Error>>>> {
    pending.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How long a request may stay unanswered before it fails with
/// [`Error::Timeout`].
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A handle for issuing requests to, and answering server-initiated requests
/// from, a running app-server.
///
/// Cheap to clone; all clones share one connection.
#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

struct Shared {
    next_id: AtomicI64,
    outbound: mpsc::Sender<JSONRPCMessage>,
    pending: std::sync::Mutex<HashMap<String, oneshot::Sender<Result<Value, Error>>>>,
}

impl Client {
    /// Creates a client that sends frames through `outbound`.
    pub fn new(outbound: mpsc::Sender<JSONRPCMessage>) -> Self {
        Self {
            shared: Arc::new(Shared {
                next_id: AtomicI64::new(1),
                outbound,
                pending: std::sync::Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Sends a client request built from a fresh [`RequestId`] and awaits its
    /// response payload.
    ///
    /// The builder keeps call sites self-documenting:
    /// `client.call(|id| ClientRequest::Initialize { request_id: id, params })`.
    pub async fn call(
        &self,
        build: impl FnOnce(RequestId) -> ClientRequest,
    ) -> Result<Value, Error> {
        let request = build(self.next_request_id());
        let key = request.id().to_string();

        let (resolver, awaiter) = oneshot::channel();
        lock_pending(&self.shared.pending).insert(key.clone(), resolver);

        let sent = self
            .shared
            .outbound
            .send(JSONRPCMessage::Request(wire_request_frame(&request)))
            .await;

        if let Err(_closed) = sent {
            lock_pending(&self.shared.pending).remove(&key);
            return Err(Error::Closed);
        }

        match tokio::time::timeout(REQUEST_TIMEOUT, awaiter).await {
            Ok(Ok(result)) => result,
            Ok(Err(_dropped)) => Err(Error::Closed),
            Err(_elapsed) => {
                lock_pending(&self.shared.pending).remove(&key);
                Err(Error::Timeout(REQUEST_TIMEOUT))
            }
        }
    }

    /// Sends a successful [`ServerResponse`]-shaped reply for a
    /// server-initiated request. The `result` must serialize into the response
    /// type that belongs to `request`'s method.
    pub async fn respond(&self, request: &ServerRequest, result: Value) -> Result<(), Error> {
        self.respond_for_id(request.id().clone(), result).await
    }

    /// Like [`Client::respond`], for requests tracked only by their raw id
    /// (the typed request was consumed when the dialog was queued).
    pub async fn respond_for_id(&self, id: RequestId, result: Value) -> Result<(), Error> {
        let frame = JSONRPCMessage::Response(JSONRPCResponse { id, result });
        self.shared
            .outbound
            .send(frame)
            .await
            .map_err(|_| Error::Closed)
    }

    /// Sends an error reply for a server-initiated request, identified only
    /// by its raw id. Used when the request could not be decoded into a
    /// typed [`ServerRequest`].
    pub async fn respond_error_for_id(
        &self,
        id: RequestId,
        code: i64,
        message: impl Into<String>,
    ) -> Result<(), Error> {
        let frame = JSONRPCMessage::Error(JSONRPCError {
            id,
            error: codex_app_server_protocol::JSONRPCErrorError {
                code,
                data: None,
                message: message.into(),
            },
        });
        self.shared
            .outbound
            .send(frame)
            .await
            .map_err(|_| Error::Closed)
    }

    fn next_request_id(&self) -> RequestId {
        let raw = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        RequestId::Integer(raw)
    }

    /// Resolves a pending request; returns `false` when the id was unknown.
    pub(crate) fn resolve(&self, id: &RequestId, result: Result<Value, Error>) -> bool {
        let key = id.to_string();
        match lock_pending(&self.shared.pending).remove(&key) {
            Some(resolver) => {
                let _ignored = resolver.send(result);
                true
            }
            None => false,
        }
    }

    /// Fails every pending request, e.g. when the process exits.
    pub(crate) fn fail_all(&self, error: Error) {
        let mut pending = lock_pending(&self.shared.pending);
        for (_id, resolver) in pending.drain() {
            let _ignored = resolver.send(Err(error.clone()));
        }
    }
}

/// Re-serializes a typed [`ClientRequest`] into the plain request frame that
/// the wire expects. Both hops are invariants of the protocol types: a typed
/// request always serializes and always matches the wire request shape.
#[allow(clippy::expect_used)]
fn wire_request_frame(request: &ClientRequest) -> codex_app_server_protocol::JSONRPCRequest {
    let value = serde_json::to_value(request).expect("client request serializes");
    serde_json::from_value(value).expect("client request matches the wire request shape")
}

impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::Spawn { program, source } => Error::Spawn {
                program: program.clone(),
                // `std::io::Error` is not `Clone`; the kind preserves the
                // meaningful payload.
                source: std::io::Error::from(source.kind()),
            },
            Error::Io(source) => Error::Io(std::io::Error::from(source.kind())),
            Error::Json(source) => Error::Json(serde::de::Error::custom(source.to_string())),
            Error::Closed => Error::Closed,
            Error::Timeout(duration) => Error::Timeout(*duration),
            Error::Request { code, message } => Error::Request {
                code: *code,
                message: message.clone(),
            },
            Error::OrphanResponse(id) => Error::OrphanResponse(id.clone()),
        }
    }
}
