//! In-GUI WeChat binding: the QR login flow rendered inside the WeChat
//! pane instead of the terminal.
//!
//! The flow reuses the bridge crate's [`run_login`] unchanged: this module
//! only maps its [`LoginEvent`]s into the binding phase shown by the pane,
//! renders the QR matrix on a canvas, and relays the typed verify code
//! back into the polling loop. Cancelling drops the login future through
//! a oneshot stop signal, so no orphaned poller outlives the pane.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use codex_weixin_bridge::wechat::auth::LoginEvent;
use iced::Rectangle;
use iced::widget::canvas;
use iced::widget::canvas::Geometry;
use qrcode::QrCode;

/// The QR login window, matching the CLI `login` command's timeout.
pub const BINDING_TIMEOUT: Duration = Duration::from_secs(480);

/// Displayed edge length of the QR code canvas, in logical pixels.
const QR_DISPLAY_SIZE: f32 = 216.0;

/// How often the verify-code wait re-checks the shared slot; the flow
/// itself is dropped wholesale on cancel, so this is purely latency.
const VERIFY_POLL: Duration = Duration::from_millis(150);

// ---------------------------------------------------------------------------
// QR matrix and canvas
// ---------------------------------------------------------------------------

/// A decoded QR matrix: one boolean per module, including a 4-module
/// quiet zone on every side so scanners see the standard border.
#[derive(Debug)]
pub struct QrMatrix {
    /// Total module count per side (payload + quiet zones).
    pub size: usize,
    cells: Vec<bool>,
}

impl QrMatrix {
    /// Encodes `payload` into a QR matrix; `None` when the payload is not
    /// encodable (never expected for the login URLs).
    pub fn encode(payload: &str) -> Option<Self> {
        let code = QrCode::new(payload.as_bytes()).ok()?;
        let width = code.width();
        let colors = code.to_colors();
        let quiet = 4usize;
        let size = width + quiet * 2;
        let mut cells = vec![false; size * size];
        for y in 0..width {
            for x in 0..width {
                if matches!(colors[y * width + x], qrcode::Color::Dark) {
                    cells[(y + quiet) * size + (x + quiet)] = true;
                }
            }
        }
        Some(Self { size, cells })
    }

    /// Whether the module at (`x`, `y`) is dark.
    fn is_dark(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.size + x]
    }
}

/// A canvas widget painting one [`QrMatrix`] as black modules on white.
pub struct QrCanvas {
    matrix: Arc<QrMatrix>,
    size: f32,
}

impl QrCanvas {
    /// The widget at its display size, centered by the caller.
    pub fn widget(matrix: Arc<QrMatrix>) -> canvas::Canvas<Self, crate::message::Message> {
        canvas(Self {
            matrix,
            size: QR_DISPLAY_SIZE,
        })
        .width(QR_DISPLAY_SIZE)
        .height(QR_DISPLAY_SIZE)
    }
}

impl canvas::Program<crate::message::Message> for QrCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let mut frame = canvas::Frame::new(renderer, iced::Size::new(self.size, self.size));
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            iced::Size::new(self.size, self.size),
            iced::Color::WHITE,
        );
        let module = self.size / self.matrix.size as f32;
        let dark = iced::Color::from_rgb8(0x1F, 0x23, 0x28);
        for y in 0..self.matrix.size {
            for x in 0..self.matrix.size {
                if self.matrix.is_dark(x, y) {
                    frame.fill_rectangle(
                        iced::Point::new(x as f32 * module, y as f32 * module),
                        iced::Size::new(module, module),
                        dark,
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

// ---------------------------------------------------------------------------
// Binding state
// ---------------------------------------------------------------------------

/// What the binding pane is showing right now.
#[derive(Clone, Debug)]
pub enum BindingPhase {
    /// The first QR code is being fetched.
    Starting,
    /// A QR code is on screen; `note` carries refresh progress.
    QrReady { matrix: Arc<QrMatrix>, note: String },
    /// The phone scanned the code and awaits confirmation.
    Scanned,
    /// WeChat demands the numeric code shown on the phone.
    NeedVerifyCode,
    /// Credentials are saved (or the bot was already bound).
    Connected {
        /// The user-facing notice, e.g. the bound bot id.
        notice: String,
    },
    /// The flow gave up; `message` explains why.
    Failed {
        /// The user-facing reason.
        message: String,
    },
}

/// One live binding session, held by `State.weixin_binding` while the
/// pane shows the binding view; dropped on cancel/close.
pub struct WeixinBinding {
    /// The phase currently rendered.
    pub phase: BindingPhase,
    /// The verify-code input draft.
    pub verify_input: String,
    /// Where the typed verify code lands for the polling loop to pick up.
    pub verify_code: Arc<Mutex<Option<String>>>,
    /// Breaks the login future; taken on cancel.
    pub stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl WeixinBinding {
    /// A fresh session waiting for the first QR code.
    pub fn starting(stop: tokio::sync::oneshot::Sender<()>) -> Self {
        Self {
            phase: BindingPhase::Starting,
            verify_input: String::new(),
            verify_code: Arc::new(Mutex::new(None)),
            stop: Some(stop),
        }
    }

    /// A terminal session that only shows `message` (no flow is running).
    pub fn failed(message: String) -> Self {
        Self {
            phase: BindingPhase::Failed { message },
            verify_input: String::new(),
            verify_code: Arc::new(Mutex::new(None)),
            stop: None,
        }
    }

    /// Hands the typed code to the polling loop.
    pub fn submit_verify_code(&self) {
        let code = self.verify_input.trim().to_string();
        if let Ok(mut slot) = self.verify_code.lock() {
            *slot = Some(code);
        }
    }
}

// ---------------------------------------------------------------------------
// Verify-code relay
// ---------------------------------------------------------------------------

/// Waits until the user submits the verify code typed into the pane.
/// An empty code makes the server reject the attempt; cancelling drops
/// this wait together with the whole login future.
pub async fn wait_for_verify_code(slot: Arc<Mutex<Option<String>>>) -> String {
    loop {
        if let Ok(mut guard) = slot.lock()
            && let Some(code) = guard.take()
        {
            return code;
        }
        tokio::time::sleep(VERIFY_POLL).await;
    }
}

// ---------------------------------------------------------------------------
// Progress mapping and transitions
// ---------------------------------------------------------------------------

/// Progress events surfaced by the login flow, one per pane update.
#[derive(Clone, Debug)]
pub enum BindingEvent {
    /// The first QR code is ready.
    QrReady(Arc<QrMatrix>),
    /// A refreshed QR code replaced an expired one.
    QrRefreshed(u32, Arc<QrMatrix>, String),
    /// The phone scanned the code.
    Scanned,
    /// WeChat wants the numeric code typed in.
    NeedVerifyCode,
    /// A progress note (access-point switch, refresh countdown, …).
    Message(String),
    /// The flow ended: `Ok` carries the success notice.
    Finished(Result<String, String>),
}

/// Maps a raw login event; `None` for no-op ticks (Waiting).
pub fn map_login_event(event: LoginEvent) -> Option<BindingEvent> {
    match event {
        LoginEvent::QrReady { url } => {
            QrMatrix::encode(&url).map(|matrix| BindingEvent::QrReady(Arc::new(matrix)))
        }
        LoginEvent::QrRefreshed { count, url } => QrMatrix::encode(&url).map(|matrix| {
            BindingEvent::QrRefreshed(count, Arc::new(matrix), format!("二维码已刷新（{count}）"))
        }),
        LoginEvent::Scanned => Some(BindingEvent::Scanned),
        LoginEvent::NeedVerifyCode => Some(BindingEvent::NeedVerifyCode),
        LoginEvent::Message(message) => Some(BindingEvent::Message(message)),
        LoginEvent::Waiting => None,
    }
}

/// Applies one event to the binding session (pure state transition).
pub fn apply_event(binding: &mut WeixinBinding, event: BindingEvent) {
    match event {
        BindingEvent::QrReady(matrix) => {
            binding.phase = BindingPhase::QrReady {
                matrix,
                note: String::new(),
            };
        }
        BindingEvent::QrRefreshed(_count, matrix, note) => {
            binding.phase = BindingPhase::QrReady { matrix, note };
        }
        BindingEvent::Scanned => {
            binding.phase = BindingPhase::Scanned;
        }
        BindingEvent::NeedVerifyCode => {
            binding.phase = BindingPhase::NeedVerifyCode;
            binding.verify_input.clear();
        }
        BindingEvent::Message(note) => {
            // Progress notes only decorate the QR view; elsewhere (e.g.
            // while the verify-code box is up) they must not clobber the
            // current phase.
            if let BindingPhase::QrReady { note: slot, .. } = &mut binding.phase {
                *slot = note;
            }
        }
        BindingEvent::Finished(outcome) => {
            binding.phase = match outcome {
                Ok(notice) => BindingPhase::Connected { notice },
                Err(message) => BindingPhase::Failed { message },
            };
            binding.stop = None;
        }
    }
}

#[cfg(test)]
#[path = "weixin_binding_tests.rs"]
mod tests;
