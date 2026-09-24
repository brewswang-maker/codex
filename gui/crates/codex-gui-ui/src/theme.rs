//! Qoder-style light theme: color, type, and spacing tokens.
//!
//! Every color is pixel-sampled from the reference screenshots (see the
//! replica plan's style spec); type and radius scales are the matching
//! estimates and stay in one place so tweaks land here first.

// The token table is the design vocabulary; later milestones (message
// cards, welcome page, timestamps) consume the entries not yet wired.
#![allow(dead_code)]

use iced::Color;
use iced::Theme;
use iced::color;

/// The GUI renders the Qoder-style light theme.
pub fn application_theme(_state: &crate::State) -> Theme {
    Theme::Light
}

// ---- Surfaces (pixel-sampled) ----

/// Editor and conversation background.
pub const BG: iced::Color = color!(0xFFFFFF);
/// Activity bar, status bar, and panel chrome.
pub const PANEL: iced::Color = color!(0xF3F3F3);
/// Sidebar surface (the Quest-style rail column).
pub const SIDEBAR: iced::Color = color!(0xF8F8F8);
/// Top bar strip.
pub const TOPBAR: iced::Color = color!(0xF9F9F9);
/// Hairline borders and separators.
pub const BORDER: iced::Color = color!(0xE3E3E3);
/// Filled cards (welcome entries, quoted blocks).
pub const CARD: iced::Color = color!(0xF0F0F0);

// ---- Brand and accents ----

/// Brand green: icons, active entries, links.
pub const BRAND: iced::Color = color!(0x208F40);
/// Live-status glow dot green.
pub const GLOW: iced::Color = color!(0x71E191);
/// Copper accent for the wordmark.
pub const GOLD: iced::Color = color!(0xB69A76);
/// Neutral badge fill.
pub const BADGE: iced::Color = color!(0xC4C4C4);

// ---- Text ----

/// Primary text.
pub const TEXT: iced::Color = color!(0x1F2328);
/// Secondary text.
pub const MUTED: iced::Color = color!(0x6B7280);
/// Faint text (placeholders, timestamps).
pub const FAINT: iced::Color = color!(0x9AA1AB);

// ---- Functional ----

/// Destructive or warning red.
pub const DANGER: iced::Color = color!(0xD9534F);
/// Info blue: the running Quest state.
pub const INFO: iced::Color = color!(0x3B82F6);
/// Warning orange: the waiting-on-user Quest state.
pub const WARN: iced::Color = color!(0xD97706);

// ---- Type scale ----

/// Tiny labels (badges, timestamps).
pub const SIZE_XS: f32 = 11.0;
/// Status bar and chrome.
pub const SIZE_SM: f32 = 12.0;
/// Sidebar rows and the file tree.
pub const SIZE_MD: f32 = 13.0;
/// Conversation body.
pub const SIZE_BODY: f32 = 14.0;
/// Welcome headline.
pub const SIZE_TITLE: f32 = 28.0;

// ---- Radii ----

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;

/// Muted text for secondary labels and hints.
pub fn dim(_theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(MUTED) }
}

/// Primary-colored text.
pub fn fg(_theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(TEXT) }
}

/// Faint text for timestamps, paths, and attributions.
pub fn faint_text(_theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style { color: Some(FAINT) }
}

/// How many recent projects the welcome page lists.
pub const WELCOME_RECENTS_SHOWN: usize = 4;

/// A flat surface filled with `background`.
pub fn surface(background: iced::Color) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |_theme| iced::widget::container::Style {
        background: Some(background.into()),
        ..iced::widget::container::Style::default()
    }
}

/// A card surface: white fill, hairline border, rounded corners.
pub fn card(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(BG.into()),
        border: iced::Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_LG.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// A borderless button whose content carries its own color.
pub fn ghost_button(
    _theme: &Theme,
    _status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: None,
        text_color: MUTED,
        border: iced::Border {
            radius: RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::button::Style::default()
    }
}

/// An activity-rail entry: transparent until active, then a white chip.
pub fn rail_button(
    active: bool,
) -> impl Fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    move |_theme, _status| iced::widget::button::Style {
        background: active.then_some(BG.into()),
        text_color: TEXT,
        border: iced::Border {
            radius: RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::button::Style::default()
    }
}

/// The frameless composer input inside the white rounded card.
pub fn bare_input(
    _theme: &Theme,
    _status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: Color::TRANSPARENT.into(),
        border: iced::Border::default(),
        icon: MUTED,
        placeholder: FAINT,
        value: TEXT,
        selection: color!(0xB4D5FE),
    }
}

/// The model pick list: transparent until opened, white behind the menu.
pub fn pick_list(
    _theme: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let opened = matches!(status, iced::widget::pick_list::Status::Opened { .. });
    iced::widget::pick_list::Style {
        text_color: MUTED,
        placeholder_color: FAINT,
        handle_color: MUTED,
        background: if opened {
            BG.into()
        } else {
            Color::TRANSPARENT.into()
        },
        border: iced::Border {
            color: if opened { BORDER } else { Color::TRANSPARENT },
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
    }
}

/// A card-shaped clickable surface: white with a hairline border, which
/// tints on hover (board cards, scenario entries).
pub fn card_button(
    _theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let background = match status {
        iced::widget::button::Status::Hovered => CARD,
        _ => BG,
    };
    iced::widget::button::Style {
        background: Some(background.into()),
        text_color: TEXT,
        border: iced::Border {
            color: BORDER,
            width: 1.0,
            radius: RADIUS_SM.into(),
        },
        ..iced::widget::button::Style::default()
    }
}

/// The brand-filled send button.
pub fn primary_button(
    _theme: &Theme,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let base = color!(0x2AA252);
    iced::widget::button::Style {
        background: Some(
            match status {
                iced::widget::button::Status::Hovered => base,
                _ => BRAND,
            }
            .into(),
        ),
        text_color: BG,
        border: iced::Border {
            radius: RADIUS_MD.into(),
            ..iced::Border::default()
        },
        ..iced::widget::button::Style::default()
    }
}
