//! Floating quote actions anchored where a drag selection finished —
//! the same affordance as the reference IDE's "add to conversation"
//! pills hovering right above the highlighted text.

use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::{button, container, mouse_area, opaque, row, stack, text};

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
/// is live and the user has finished it. The layer always returns a
/// `stack`, even without a popup: switching the base between its own
/// element and a stack would rebuild the whole window's widget tree,
/// resetting every scrollable's state (the transcript would jump back
/// to the top the moment the bar appeared). The bar only owns its own
/// footprint: clicks elsewhere fall through to the transcript, whose
/// click-away handler clears the selection and dismisses the bar.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let selected = state
        .text_selection
        .as_ref()
        .and_then(|selection| selection.text())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    let origin = state.selection_popup;
    let placed = selected
        .zip(origin)
        .map(|(selected, origin)| -> Element<'a, Message> {
            // The pill fires on press, not release: the click-away
            // handler tears the popup (and any button's held press state)
            // down on the very press that activates the pill, so a
            // release-triggered button would never get to publish. The
            // inner button is purely visual; both messages carry their
            // text payload, so the cleared selection does not matter.
            let pill = |label: &'a str, message: Message| -> Element<'a, Message> {
                mouse_area(
                    button(text(label).size(theme::SIZE_XS).style(theme::fg))
                        .padding([4, 10])
                        .style(theme::ghost_button),
                )
                .on_press(message)
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
            };
            let bar = row![
                pill(
                    "添加到会话",
                    Message::SelectionAppendRequested {
                        text: selected.clone(),
                    },
                ),
                pill(
                    "复制",
                    Message::CopyMessage {
                        key: String::from("selection-popup"),
                        text: selected,
                    },
                ),
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
            container(opaque(popup))
                .width(Fill)
                .height(Fill)
                .padding(iced::Padding {
                    left: x,
                    top: y,
                    ..iced::Padding::ZERO
                })
                .align_x(alignment::Horizontal::Left)
                .align_y(alignment::Vertical::Top)
                .into()
        });

    match placed {
        Some(placed) => stack![base.into(), placed].into(),
        // A single-child stack keeps the widget tree identical across the
        // popup's lifetime, so the transcript's scroll offset survives.
        None => stack![base.into()].into(),
    }
}
