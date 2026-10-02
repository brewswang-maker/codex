//! Floating quote actions anchored where a drag selection finished —
//! the same affordance as the reference IDE's "add to conversation"
//! pills hovering right above the highlighted text.

use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::{button, container, opaque, row, stack, text};

use crate::Message;
use crate::State;
use crate::theme;

/// Gap between the release spot and the popup; also the distance the
/// popup falls back below the pointer when there is no room above.
const GAP: f32 = 40.0;
const BELOW_GAP: f32 = 14.0;
/// Screen inset the popup never crosses while clamping.
const INSET: f32 = 8.0;

/// Stacks the floating quote actions over `base` while a drag selection
/// is live and the user has finished it. The bar only owns its own
/// footprint: clicks elsewhere fall through to the transcript, whose
/// click-away handler clears the selection and dismisses the bar.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let Some(selected) = state
        .text_selection
        .as_ref()
        .and_then(|selection| selection.text())
        .map(|text| text.trim().to_owned())
    else {
        return base.into();
    };
    if selected.is_empty() {
        return base.into();
    }
    let Some(origin) = state.selection_popup else {
        return base.into();
    };

    let bar = row![
        button(
            text(String::from("添加到会话"))
                .size(theme::SIZE_XS)
                .style(theme::fg)
        )
        .padding([4, 10])
        .style(theme::ghost_button)
        .on_press(Message::SelectionAppendRequested {
            text: selected.clone(),
        }),
        button(
            text(String::from("复制"))
                .size(theme::SIZE_XS)
                .style(theme::fg)
        )
        .padding([4, 10])
        .style(theme::ghost_button)
        .on_press(Message::CopyMessage {
            key: String::from("selection-popup"),
            text: selected,
        }),
    ]
    .spacing(6)
    .align_y(alignment::Vertical::Center);

    // Anchor just above the release spot; flip below when the bar would
    // leave the window through the top. The horizontal clamp is a rough
    // estimate — the bar is small, so the window's right edge is only a
    // concern for releases hugging it.
    let estimated_half_width = 96.0;
    let x = origin.x.max(INSET + estimated_half_width) - estimated_half_width;
    let y = if origin.y - GAP < INSET {
        origin.y + BELOW_GAP
    } else {
        origin.y - GAP
    };

    let popup = container(bar).padding([6, 8]).style(theme::card);
    let placed = container(opaque(popup))
        .width(Fill)
        .height(Fill)
        .padding(iced::Padding {
            left: x,
            top: y,
            ..iced::Padding::ZERO
        })
        .align_x(alignment::Horizontal::Left)
        .align_y(alignment::Vertical::Top);

    stack![base.into(), placed].into()
}
