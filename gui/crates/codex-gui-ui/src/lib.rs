//! iced layer of the Codex GUI: messages, state, update logic, and views.
//!
//! Everything iced-specific lives here. [`codex_gui_bridge`] and
//! [`codex_gui_core`] stay UI-agnostic so they can be tested headlessly.

mod activity_bar;
mod approvals_view;
mod attachments;
mod chat;
mod command_palette;
mod commands;
mod diff_view;
mod editor_view;
mod git_view;
mod icons;
mod menu_bar;
mod message;
mod plan_view;
mod quest_board;
mod quest_launch;
mod quest_overlays;
mod quest_rows;
mod quest_view;
mod sessions_view;
mod settings_view;
mod state;
mod status_notifications;
mod theme;
mod update;
mod view;
mod welcome_view;

pub use message::Bootstrap;
pub use message::Message;
pub use state::State;
pub use state::Status;
pub use theme::application_theme;
pub use update::update;
pub use view::view;

use codex_gui_bridge::Flags;
use codex_gui_bridge::GuiEvent;
use iced::Event;
use iced::Subscription;
use iced::futures::SinkExt;
use iced::futures::Stream;
use iced::futures::channel::mpsc::Sender;
use iced::window;

/// How often buffered streaming deltas are flushed into the rendered
/// markdown; spec section 7 fixes the throttle at 50 ms.
const RENDER_TICK_MS: u64 = 50;

/// The single passive subscription: the app-server connection itself.
///
/// `run_with` keys the subscription identity on the launch flags plus the
/// connection epoch, so the connection survives every re-render while a
/// relaunch (`Message::Reconnect`) spawns a fresh one.
pub fn subscription(state: &State) -> Subscription<Message> {
    let identity = (state.flags.clone(), state.connection_epoch);
    let connection = Subscription::run_with(identity, connection_stream).map(Message::Event);

    // Native window drag-and-drop: iced forwards the winit file events, and
    // only the three this UI consumes produce messages.
    let drops = iced::event::listen_with(|event, _status, _window| match event {
        Event::Window(window::Event::FileHovered(_)) => Some(Message::DragHovered),
        Event::Window(window::Event::FilesHoveredLeft) => Some(Message::DragLeft),
        Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        _ => None,
    });

    // The render tick only lives while deltas are buffered; once the stream
    // goes idle the timer subscription is dropped entirely.
    let tick = state.stream.is_active().then(|| {
        iced::time::every(std::time::Duration::from_millis(RENDER_TICK_MS))
            .map(|_instant| Message::Tick)
    });

    // Ctrl+Shift+P raises the palette from anywhere; while it is up the
    // arrow keys and Esc drive the overlay instead.
    let palette_toggle = iced::event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.control() && modifiers.shift() =>
        {
            let is_p = matches!(&key, iced::keyboard::Key::Character(c)
                if c.eq_ignore_ascii_case("p"));
            is_p.then_some(Message::PaletteToggled)
        }
        _ => None,
    });
    let new_quest_keys = iced::event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.control() && !modifiers.shift() && !modifiers.alt() =>
        {
            // Ctrl+N starts a fresh Quest from anywhere, matching the
            // shortcut the menu and the Quest rail advertise.
            let is_n = matches!(&key, iced::keyboard::Key::Character(c)
                if c.eq_ignore_ascii_case("n"));
            is_n.then_some(Message::NewThreadRequested)
        }
        _ => None,
    });

    // Ctrl+Shift+Q flips between the editor and Quest shells, matching the
    // reference IDE's Quest shortcut.
    let mode_toggle_keys = iced::event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.control() && modifiers.shift() =>
        {
            let is_q = matches!(&key, iced::keyboard::Key::Character(c)
                if c.eq_ignore_ascii_case("q"));
            is_q.then_some(Message::ModeToggled)
        }
        _ => None,
    });

    // While a Quest dialog owns the keyboard, Esc dismisses it and Enter
    // confirms the rename draft (the focused input can also fire the
    // confirm; the reducer guards against the double signal).
    let quest_overlay_keys = (!state.palette.open && state.quest.any_open()).then(|| {
        iced::event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(named),
                modifiers,
                ..
            }) => match named {
                iced::keyboard::key::Named::Escape => Some(Message::QuestOverlaysClosed),
                iced::keyboard::key::Named::Enter if !modifiers.shift() => {
                    Some(Message::QuestRenameConfirmed)
                }
                _ => None,
            },
            _ => None,
        })
    });
    let palette_keys = state.palette.open.then(|| {
        iced::event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(named),
                modifiers,
                ..
            }) => match (named, modifiers.shift()) {
                (iced::keyboard::key::Named::Escape, _) => Some(Message::PaletteToggled),
                (iced::keyboard::key::Named::Enter, _) => Some(Message::PaletteConfirmed),
                (iced::keyboard::key::Named::ArrowUp, false) => Some(Message::PaletteMoved(-1)),
                (iced::keyboard::key::Named::ArrowDown, false) => Some(Message::PaletteMoved(1)),
                _ => None,
            },
            _ => None,
        })
    });

    // Ctrl+F raises the transcript search bar unless the palette owns the
    // keyboard; while the bar is up Esc closes it and Enter steps forward.
    let search_toggle = (!state.palette.open).then(|| {
        iced::event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
                if modifiers.control() && !modifiers.shift() =>
            {
                let is_f = matches!(&key, iced::keyboard::Key::Character(c)
                    if c.eq_ignore_ascii_case("f"));
                is_f.then_some(Message::ChatSearchOpened)
            }
            _ => None,
        })
    });
    let search_keys = (state.chat_search.open && !state.palette.open).then(|| {
        iced::event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. })
                if !modifiers.control() && !modifiers.alt() =>
            {
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                        Some(Message::ChatSearchClosed)
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                        // Shift+Enter steps backward, Enter forward.
                        Some(if modifiers.shift() {
                            Message::ChatSearchPrev
                        } else {
                            Message::ChatSearchNext
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    });

    // Esc dismisses an open top-menu dropdown; the menu titles themselves
    // toggle their panels from the bar.
    let menu_keys = state.menu.is_some().then(|| {
        iced::event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => Some(Message::MenuClosed),
            _ => None,
        })
    });

    // Software-renderer pacing logs: first-frame latency, average frame
    // rate, and wheel-event timestamps feed the M4 performance notes.
    let perf_events = iced::event::listen_with(|event, _status, _window| perf::observe(&event));

    Subscription::batch([
        connection,
        drops,
        tick.unwrap_or_else(Subscription::none),
        new_quest_keys,
        mode_toggle_keys,
        quest_overlay_keys.unwrap_or_else(Subscription::none),
        palette_toggle,
        palette_keys.unwrap_or_else(Subscription::none),
        search_toggle.unwrap_or_else(Subscription::none),
        search_keys.unwrap_or_else(Subscription::none),
        menu_keys.unwrap_or_else(Subscription::none),
        perf_events,
    ])
}

/// Adapts the bridge connection into an iced stream (websocket-example
/// pattern: the client handle is delivered through the event stream itself).
///
/// The boxed concrete return keeps the `fn(&(Flags, u64)) -> _` shape that
/// `Subscription::run_with` requires.
fn connection_stream(
    identity: &(Flags, u64),
) -> std::pin::Pin<Box<dyn Stream<Item = GuiEvent> + Send>> {
    let flags = identity.0.clone();
    Box::pin(iced::stream::channel(
        64,
        move |mut output: Sender<GuiEvent>| async move {
            match codex_gui_bridge::start(flags).await {
                Ok((client, mut events)) => {
                    if output.send(GuiEvent::Connected(client)).await.is_err() {
                        return;
                    }
                    while let Some(event) = events.recv().await {
                        if output.send(event).await.is_err() {
                            break;
                        }
                    }
                }
                Err(error) => {
                    let _ignored = output.send(GuiEvent::Disconnected(error.to_string())).await;
                }
            }
        },
    ))
}

/// Boot-to-first-frame latency, redraw pacing, and wheel-event timestamps
/// for the software renderer, emitted as `[perf]` stderr lines.
pub mod perf {
    use crate::message::Message;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    static BOOT: OnceLock<Instant> = OnceLock::new();
    static FRAMES: AtomicUsize = AtomicUsize::new(0);
    static WHEELS: AtomicUsize = AtomicUsize::new(0);

    /// Called once by the shell binary before the iced builder runs.
    pub fn mark_boot() {
        let _ = BOOT.set(Instant::now());
    }

    /// Called from the head of `view()`: with the tiny-skia compositor
    /// every drawn frame passes here, so the first call is the first
    /// frame and every 120th call samples the average frame rate.
    pub fn frame_tick() {
        let Some(boot) = BOOT.get() else {
            return;
        };
        let frames = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
        if frames == 1 {
            eprintln!("[perf] first frame after {:?}", boot.elapsed());
        } else if frames.is_multiple_of(120) {
            let secs = boot.elapsed().as_secs_f32();
            eprintln!(
                "[perf] frames={frames} avg={:.1}fps elapsed={secs:.1}s",
                frames as f32 / secs
            );
        }
    }

    /// The passive observer folded into the subscription batch; it never
    /// produces messages and only samples the event flow.
    pub fn observe(event: &iced::Event) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled { .. }) => {
                let wheels = WHEELS.fetch_add(1, Ordering::Relaxed) + 1;
                if wheels <= 3 || wheels.is_multiple_of(25) {
                    eprintln!("[perf] wheel #{wheels}");
                }
                None
            }
            _ => None,
        }
    }
}
