//! The multi-vendor model menu: a Qoder-style grouped picker layered
//! over the window, one section per vendor with its switchable models.

use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::VendorGroup;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::mouse_area;
use iced::widget::opaque;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;

/// Panel metrics, in the palette's size class.
const PANEL_WIDTH: f32 = 560.0;
const LIST_HEIGHT: f32 = 420.0;

/// The model menu overlaid over `base` while it is open.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.model_menu_open {
        return base.into();
    }

    let panel = container(menu_panel(state))
        .width(Fill)
        .max_width(PANEL_WIDTH)
        .align_x(alignment::Horizontal::Center)
        .style(surface);

    // Anchor below the top bar, mirroring the palette placement; the
    // mask dismisses like the other overlays, the panel stays clickable.
    let placement = container(opaque(panel))
        .width(Fill)
        .height(Fill)
        .padding(iced::Padding::default().top(64))
        .align_x(alignment::Horizontal::Center);

    stack![
        base.into(),
        // The shield sits outside the `mouse_area`: an `opaque` inside
        // would capture the press first and swallow the dismissal.
        opaque(
            mouse_area(container(iced::widget::Space::new().width(Fill).height(Fill)).style(mask),)
                .on_press(Message::ModelMenuClosed),
        ),
        placement,
    ]
    .into()
}

fn menu_panel(state: &State) -> Element<'_, Message> {
    // The live thread's own binding marks the ticks (the resumed thread's
    // model can differ from the config default); the config pair stands in
    // until a binding is known.
    let (current_provider, current_model) = match state.active_binding.as_ref() {
        Some((provider, model)) => (Some(provider.as_str()), Some(model.as_str())),
        None => state.vendors.current(),
    };

    let mut list = column![].spacing(2);
    for group in state.vendors.groups() {
        list = list.push(group_header(group, current_provider));
        for choice in &group.models {
            let selected = group.provider_id.as_deref() == current_provider
                && Some(choice.slug.as_str()) == current_model;
            list = list.push(model_row(group, &choice.slug, &choice.name, selected));
        }
    }

    column![
        row![
            text("选择模型").size(theme::SIZE_SM).style(theme::fg),
            iced::widget::Space::new().width(Fill),
            button(text("关闭").size(theme::SIZE_XS).style(theme::dim))
                .padding([2, 8])
                .style(theme::ghost_button)
                .on_press(Message::ModelMenuClosed),
        ]
        .align_y(alignment::Vertical::Center),
        container(scrollable(list).height(LIST_HEIGHT)).width(Fill),
    ]
    .spacing(10)
    .into()
}

/// One vendor header row: name plus the configured/current badge, or the
/// settings shortcut while the vendor has no provider yet.
fn group_header<'a>(
    group: &'a VendorGroup,
    current_provider: Option<&str>,
) -> Element<'a, Message> {
    let is_current = group.provider_id.as_deref() == current_provider;
    let badge: Element<'a, Message> = if is_current {
        text("当前").size(theme::SIZE_XS).style(theme::brand).into()
    } else if group.configured {
        text("已配置").size(theme::SIZE_XS).style(theme::dim).into()
    } else {
        button(
            text("未配置 · 去设置")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        )
        .padding([2, 8])
        .style(theme::ghost_button)
        .on_press(Message::SettingsToggled)
        .into()
    };

    container(
        row![
            text(group.name.clone())
                .size(theme::SIZE_SM)
                .style(theme::fg),
            iced::widget::Space::new().width(Fill),
            badge,
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([6, 10])
    .into()
}

/// One model row: tick for the current pick, name, description; rows of
/// an unconfigured vendor are dim and inert.
fn model_row<'a>(
    group: &'a VendorGroup,
    slug: &'a str,
    name: &'a str,
    selected: bool,
) -> Element<'a, Message> {
    let tick = if selected { "◉" } else { "○" };
    let per_row: Element<'a, Message> = if group.configured {
        let provider_id = group.provider_id.clone().unwrap_or_default();
        let slug_owned = slug.to_string();
        button(row_body(tick, name, group.configured))
            .padding([6, 24])
            .width(Fill)
            .style(theme::ghost_button)
            .on_press(Message::VendorModelPicked {
                provider_id,
                slug: slug_owned,
            })
            .into()
    } else {
        container(row_body(tick, name, group.configured))
            .padding([6, 24])
            .width(Fill)
            .into()
    };
    per_row
}

/// The shared inner row: state glyph plus the model name.
fn row_body<'a>(tick: &'a str, name: &'a str, enabled: bool) -> Element<'a, Message> {
    row![
        text(tick).size(theme::SIZE_XS).style(if enabled {
            theme::brand
        } else {
            theme::faint_text
        }),
        text(name.to_string())
            .size(theme::SIZE_SM)
            .style(if enabled {
                theme::dim
            } else {
                theme::faint_text
            }),
        iced::widget::Space::new().width(Fill),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// The dialog surface behind the menu.
fn surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_LG.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// The dismissible shield dimming the window behind the menu.
fn mask(_theme: &iced::Theme) -> iced::widget::container::Style {
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
