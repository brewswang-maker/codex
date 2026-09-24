//! Renders parsed markdown content, with syntax-highlighted code blocks
//! (the `highlighter` feature drives them inside `markdown::Content`).

use crate::message::Message;
use iced::Element;
use iced::Theme;
use iced::widget::markdown;

/// Turns parsed markdown content into a rendered element.
///
/// `markdown::view` styles itself from the theme and reports link clicks
/// through the URI message; the light theme keeps body text dark on the
/// white conversation surface.
pub fn render(content: &markdown::Content) -> Element<'_, Message> {
    markdown::view(content.items(), &Theme::Light).map(Message::LinkClicked)
}

#[cfg(test)]
#[path = "markdown_view_tests.rs"]
mod tests;
