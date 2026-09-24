//! The Codex GUI binary: assembles the iced application around the
//! UI crate and spawns the app-server backend.

mod flags;

use codex_gui_bridge::Flags;
use codex_gui_ui::State;
use iced::font;

fn main() -> iced::Result {
    let boot = std::time::Instant::now();
    codex_gui_ui::perf::mark_boot();

    let args = match flags::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    relaunch_for_software_rendering(&args);

    init_tracing();

    let app = iced::application(
        move || {
            let setup = std::time::Instant::now();
            let bridge_flags: Flags = args.bridge.clone();
            let state = State::new(bridge_flags);
            eprintln!("[perf] State::new in {:?}", setup.elapsed());
            (state, iced::Task::none())
        },
        codex_gui_ui::update,
        codex_gui_ui::view,
    )
    .title(|_state: &State| "Codex".to_string())
    .theme(codex_gui_ui::application_theme)
    .subscription(codex_gui_ui::subscription);

    // iced builds its font database from embedded fonts only, so CJK text
    // renders as blanks unless a system CJK font is loaded explicitly. The
    // loaded family also becomes the default so every widget picks it up
    // (the replica plan pins Noto Sans CJK SC as the UI typeface).
    let app = match load_cjk_font() {
        Some(bytes) => app
            .font(bytes)
            .default_font(font::Font::with_name("Noto Sans CJK SC")),
        None => {
            eprintln!("warning: no system CJK font found; Chinese text may not render");
            app
        }
    };

    eprintln!("[perf] builder ready after {:?}", boot.elapsed());

    app.run()
}

/// System font paths known to cover CJK glyphs, in preference order.
const CJK_FONT_CANDIDATES: &[&str] = &[
    // Linux: Noto CJK, then the Droid fallback.
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Medium.ttc",
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    // macOS and Windows ship CJK fonts with the OS.
    "/System/Library/Fonts/PingFang.ttc",
    "C:\\Windows\\Fonts\\msyh.ttc",
];

/// Reads the first available system font covering CJK glyphs.
fn load_cjk_font() -> Option<Vec<u8>> {
    CJK_FONT_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
}

/// Honors `--software-rendering` by re-executing this binary with
/// `ICED_BACKEND=tiny-skia`, the renderer preference that
/// `iced/renderer/src/fallback.rs` consults: the wgpu primary candidate fails
/// its backend-preference check and the tiny-skia fallback compositor wins,
/// so no GPU adapter is ever initialized.
///
/// iced reads the variable inside its event loop and exposes no builder API
/// for it, and the workspace forbids `unsafe` (in-process `env::set_var`),
/// so the relaunch is the only way to pass the preference in. The relaunched
/// process sees the variable set and returns from this function immediately,
/// ending the loop. An explicitly set `ICED_BACKEND` always wins over the
/// flag.
fn relaunch_for_software_rendering(args: &flags::GuiArgs) {
    if !args.software_rendering || std::env::var_os("ICED_BACKEND").is_some() {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        eprintln!("warning: --software-rendering could not resolve the executable path");
        return;
    };

    match std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("ICED_BACKEND", "tiny-skia")
        .status()
    {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("warning: --software-rendering relaunch failed: {error}");
        }
    }
}

/// Wires `RUST_LOG` (default `info`) into `tracing` so backend stderr and
/// bridge warnings are visible in the launching terminal.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ignored = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
