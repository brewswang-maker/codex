//! The welcome surface shown while the transcript is empty: brand
//! headline plus the persisted recent-projects list.

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::text;

/// The welcome column: glyph, headline, hint, recent projects.
pub fn welcome(state: &State) -> Element<'_, Message> {
    let mut pane = column![
        Icon::new(IconKind::Chat, theme::BRAND, 48.0).widget(),
        text("What can I help you ship?")
            .size(theme::SIZE_TITLE)
            .style(theme::fg),
        text("Describe a task, attach an image, or resume a thread from the sidebar.")
            .size(theme::SIZE_MD)
            .style(theme::dim),
    ]
    .spacing(12)
    .align_x(alignment::Horizontal::Center);

    let recents = state.recents.entries();
    if !recents.is_empty() {
        pane = pane.push(recent_list(recents));
    }

    pane.into()
}

/// The recent-projects block: a small caption over clickable rows.
fn recent_list(recents: &[String]) -> Element<'_, Message> {
    let caption = text("Recent").size(theme::SIZE_XS).style(theme::dim);

    let mut list = column![].spacing(2);
    for path in recents.iter().take(theme::WELCOME_RECENTS_SHOWN) {
        let name = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        list = list.push(
            button(
                row![
                    Icon::new(IconKind::Folder, theme::MUTED, 16.0).widget(),
                    container(
                        column![
                            text(name).size(theme::SIZE_SM).style(theme::fg),
                            text(path.clone())
                                .size(theme::SIZE_XS)
                                .style(theme::faint_text),
                        ]
                        .spacing(2)
                    )
                    .width(Fill),
                ]
                .spacing(10)
                .align_y(alignment::Vertical::Center),
            )
            .padding([8, 12])
            .style(theme::ghost_button)
            .on_press(Message::FolderPicked(Some(std::path::PathBuf::from(path))))
            .width(420.0),
        );
    }

    container(
        column![caption, list]
            .spacing(6)
            .align_x(alignment::Horizontal::Center),
    )
    .padding(iced::Padding::default().top(28))
    .into()
}
