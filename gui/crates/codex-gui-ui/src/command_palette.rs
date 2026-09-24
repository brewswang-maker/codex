//! The Ctrl+Shift+P command palette: a stack overlay with a query input
//! and a fuzzily filtered action list.
//!
//! Actions map onto existing [`Message`]s, so every palette entry runs the
//! exact code path its regular UI control does.

use crate::message::Message;
use crate::message::SidebarTab;
use crate::state::PaletteState;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;
use iced::widget::text_input;

/// Focus id of the query input, so opening the palette can hand it the
/// keyboard without a click.
pub const PALETTE_INPUT_ID: &str = "palette-input";

/// The full action catalog; filter order follows this order.
const ACTIONS: &[(&str, Message)] = &[
    ("Open folder…", Message::FolderPickRequested),
    ("New thread", Message::NewThreadRequested),
    ("Toggle Quest mode", Message::ModeToggled),
    ("Show threads", Message::SidebarTab(SidebarTab::Threads)),
    ("Show files", Message::SidebarTab(SidebarTab::Files)),
    ("Show search", Message::SidebarTab(SidebarTab::Search)),
    (
        "Show source control",
        Message::SidebarTab(SidebarTab::SourceControl),
    ),
    ("Show repo wiki", Message::SidebarTab(SidebarTab::Wiki)),
    ("Show run and debug", Message::SidebarTab(SidebarTab::Debug)),
    (
        "Show remote explorer",
        Message::SidebarTab(SidebarTab::Remote),
    ),
    (
        "Show extensions",
        Message::SidebarTab(SidebarTab::Extensions),
    ),
    ("Toggle sidebar", Message::SidebarToggled),
    ("Skill picker", Message::SkillsPickerToggled),
    ("Settings", Message::SettingsToggled),
];

/// One filtered palette row: the label plus the message it will run.
pub struct PaletteAction {
    pub label: &'static str,
    pub message: Message,
}

/// Subsequence filter over the catalog; empty queries keep every action.
pub fn filtered_actions(query: &str) -> Vec<PaletteAction> {
    let needle = query.to_lowercase();
    ACTIONS
        .iter()
        .filter(|(label, _)| {
            let haystack = label.to_lowercase();
            let mut haystack_chars = haystack.chars();
            needle
                .chars()
                .all(|needle_char| haystack_chars.any(|hay_char| hay_char == needle_char))
        })
        .map(|(label, message)| PaletteAction {
            label,
            message: message.clone(),
        })
        .collect()
}

/// The palette overlay layered over `base` while open.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.palette.open {
        return base.into();
    }

    let panel = container(palette_panel(state))
        .width(Fill)
        .max_width(560)
        .align_x(alignment::Horizontal::Center)
        .style(palette_surface);

    // Anchor near the top, mirroring the reference launcher.
    let placement = container(panel)
        .width(Fill)
        .height(Fill)
        .padding(iced::Padding::default().top(48))
        .align_x(alignment::Horizontal::Center)
        .style(dim_mask);

    stack![base.into(), placement].into()
}

fn palette_panel(state: &State) -> Element<'_, Message> {
    let input = text_input("Type a command…", &state.palette.query)
        .id(PALETTE_INPUT_ID)
        .on_input(Message::PaletteQueryChanged)
        .on_submit(Message::PaletteConfirmed)
        .size(theme::SIZE_BODY)
        .padding(10)
        .style(palette_input);

    let actions = filtered_actions(&state.palette.query);
    let mut list = column![].spacing(2);
    if actions.is_empty() {
        list = list.push(
            container(
                text("No matching commands")
                    .size(theme::SIZE_SM)
                    .style(theme::dim),
            )
            .width(Fill)
            .padding([8, 12]),
        );
    }
    for (index, action) in actions.iter().enumerate() {
        let highlighted = index == state.palette.selected;
        let label = text(action.label)
            .size(theme::SIZE_SM)
            .style(if highlighted { theme::fg } else { theme::dim });
        list = list.push(
            button(container(label).width(Fill))
                .padding([8, 12])
                .style(theme::rail_button(highlighted))
                .on_press(action.message.clone()),
        );
    }

    column![input, container(scrollable(list).height(240)).width(Fill)]
        .spacing(6)
        .padding(8)
        .into()
}

/// The modal card behind the palette.
fn palette_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_MD.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

fn palette_input(
    _theme: &iced::Theme,
    _status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    iced::widget::text_input::Style {
        background: theme::PANEL.into(),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        icon: theme::MUTED,
        placeholder: theme::FAINT,
        value: theme::TEXT,
        selection: iced::Color::from_rgb8(0xB4, 0xD5, 0xFE),
    }
}

/// The click-away mask: translucent black, click does not close (the
/// palette answers to Esc, like the reference launcher).
fn dim_mask(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(
            iced::Color {
                a: 0.25,
                ..iced::Color::BLACK
            }
            .into(),
        ),
        ..iced::widget::container::Style::default()
    }
}

/// Advances the palette cursor within the filtered list.
pub fn move_selection(palette: &mut PaletteState, delta: i32) {
    let len = filtered_actions(&palette.query).len();
    if len == 0 {
        palette.selected = 0;
        return;
    }
    let selected = palette.selected as i32 + delta;
    palette.selected = selected.clamp(0, len as i32 - 1) as usize;
}

/// The message the confirmed action resolves to, if any.
pub fn confirmed_action(palette: &PaletteState) -> Option<Message> {
    filtered_actions(&palette.query)
        .get(palette.selected)
        .map(|action| action.message.clone())
}
