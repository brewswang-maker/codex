//! Shared chrome for the four full-screen board panels (scheduler,
//! knowledge, plugin market, Better Harness): page header, notice row,
//! and small form/card building blocks.

use crate::message::Message;
use crate::state::PanelNotice;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::widget::Space;
use iced::widget::button;
use iced::widget::container;
use iced::widget::row;
use iced::widget::text;

/// The page header: title on the left, Close on the right.
pub fn header(title: &'static str, close: Message) -> Element<'static, Message> {
    row![
        text(title).size(20),
        Space::new().width(Fill),
        button(text("关闭")).on_press(close),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center)
    .into()
}

/// The last operation's outcome: green on success, red on failure.
pub fn notice_row(notice: &PanelNotice) -> Element<'_, Message> {
    let style: fn(&iced::Theme) -> iced::widget::text::Style = if notice.error {
        theme::danger_text
    } else {
        theme::brand
    };
    container(text(notice.text.clone()).size(theme::SIZE_SM).style(style))
        .width(Fill)
        .padding([4, 8])
        .style(theme::surface(theme::CARD))
        .into()
}

/// A labeled form row: a fixed-width label over a filling input.
pub fn form_row<'a>(
    label: &'static str,
    input: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![text(label).width(80), input.into()]
        .spacing(12)
        .align_y(iced::Alignment::Center)
        .into()
}

/// A padded card wrapping a column body.
pub fn card<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(body.into())
        .width(Fill)
        .padding(10)
        .style(theme::card)
        .into()
}

/// A small action button (ghost style, extra-small text).
pub fn small_button(label: &'static str, on_press: Message) -> Element<'static, Message> {
    button(text(label).size(theme::SIZE_XS))
        .style(theme::ghost_button)
        .on_press(on_press)
        .into()
}
