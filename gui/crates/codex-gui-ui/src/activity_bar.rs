//! Qoder-style activity bar: the icon rail on the window's left edge.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::message::SidebarTab;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;

/// Rail width, mirroring the reference layout.
const RAIL_WIDTH: f32 = 48.0;
/// Icon size inside the rail.
const ICON_SIZE: f32 = 22.0;

/// The rail entries from top to bottom: glyph plus the tab it opens
/// (VS Code activity-bar order, minus the settings gear pinned below).
const ENTRIES: &[(IconKind, SidebarTab)] = &[
    (IconKind::Chat, SidebarTab::Threads),
    (IconKind::Folder, SidebarTab::Files),
    (IconKind::Search, SidebarTab::Search),
    (IconKind::SourceControl, SidebarTab::SourceControl),
    (IconKind::Wiki, SidebarTab::Wiki),
    (IconKind::Debug, SidebarTab::Debug),
    (IconKind::Remote, SidebarTab::Remote),
    (IconKind::Extensions, SidebarTab::Extensions),
];

/// The icon rail: pane entries on top, settings pinned to the bottom.
pub fn activity_bar(state: &State) -> Element<'_, Message> {
    let mut panes = column![].spacing(2);
    for (kind, tab) in ENTRIES {
        panes = panes.push(rail_entry(
            *kind,
            state.sidebar_tab == *tab,
            Message::SidebarTab(*tab),
        ));
    }

    let rail = column![
        panes,
        Space::new().height(Fill),
        rail_entry(
            IconKind::Sparkle,
            state.skills.page_open,
            Message::SkillsPageToggled
        ),
        rail_entry(
            IconKind::Gear,
            state.settings.open,
            Message::SettingsToggled
        ),
    ]
    .width(RAIL_WIDTH)
    .height(Fill)
    .padding([10, 0])
    .spacing(6);

    container(rail)
        .width(RAIL_WIDTH)
        .height(Fill)
        .style(|_theme| iced::widget::container::Style {
            background: Some(theme::PANEL.into()),
            border: iced::Border {
                color: theme::BORDER,
                width: 1.0,
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// One rail button: a 3px active indicator plus the glyph.
fn rail_entry(kind: IconKind, active: bool, on_press: Message) -> Element<'static, Message> {
    let color = if active { theme::BRAND } else { theme::MUTED };
    let glyph = container(
        row![
            indicator(active),
            Icon::new(kind, color, ICON_SIZE).widget(),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .align_x(iced::Alignment::Center);

    button(glyph)
        .width(Fill)
        .padding([6, 0])
        .style(theme::rail_button(active))
        .on_press(on_press)
        .into()
}

/// The left active-pane indicator strip: brand green while active, else a
/// transparent spacer that keeps the glyphs aligned.
fn indicator(active: bool) -> Element<'static, Message> {
    let strip = Space::new().width(3.0).height(ICON_SIZE);
    if active {
        container(strip).width(3.0).style(indicator_style).into()
    } else {
        strip.into()
    }
}

/// Paints the indicator strip in brand green.
pub fn indicator_style(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BRAND.into()),
        ..iced::widget::container::Style::default()
    }
}
