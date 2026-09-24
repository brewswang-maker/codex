//! The central file-preview area between the sidebar and the chat panel:
//! an open-tab strip, a read-only body, and an empty state inviting the
//! file-tree click (Qoder's editor group, read-only).

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::state::FileBody;
use crate::state::State;
use crate::theme;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::lazy;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use std::path::Path;

/// Renders the central file-preview area.
pub fn editor_area(state: &State) -> Element<'_, Message> {
    if state.editor.tabs.is_empty() {
        return empty_state();
    }

    container(
        column![tab_strip(state), preview(state)]
            .width(Fill)
            .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(preview_surface)
    .into()
}

/// The no-tabs welcome: glyph, title, and the file-tree hint.
fn empty_state() -> Element<'static, Message> {
    container(
        column![
            Icon::new(IconKind::Folder, theme::FAINT, 40.0).widget(),
            text("文件浏览区").size(theme::SIZE_MD).style(theme::fg),
            text("在左侧文件树中点击任意文件即可在此预览")
                .size(theme::SIZE_SM)
                .style(theme::dim),
        ]
        .spacing(10)
        .align_x(alignment::Horizontal::Center),
    )
    .width(Fill)
    .height(Fill)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(preview_surface)
    .into()
}

/// The open-tab strip along the top edge.
fn tab_strip(state: &State) -> Element<'_, Message> {
    let mut strip = row![].spacing(2).padding([6, 8]);
    for path in &state.editor.tabs {
        strip = strip.push(tab(state, path));
    }
    // Horizontal panning keeps every tab reachable once the strip
    // overflows a narrow editor pane.
    let strip = scrollable(strip).direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::default(),
    ));
    container(strip).width(Fill).style(tab_bar_surface).into()
}

/// One tab: the file-name button plus its close button.
fn tab(state: &State, path: &Path) -> Element<'static, Message> {
    let active = state.editor.active.as_deref() == Some(path);
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let label_color = if active { theme::TEXT } else { theme::MUTED };

    container(
        row![
            button(
                text(name)
                    .size(theme::SIZE_XS)
                    .style(move |_| iced::widget::text::Style {
                        color: Some(label_color),
                    }),
            )
            .padding([3, 6])
            .style(theme::ghost_button)
            .on_press(Message::FilePreviewSelected(path.to_path_buf())),
            button(
                text("×")
                    .size(theme::SIZE_XS)
                    .style(move |_| iced::widget::text::Style {
                        color: Some(label_color),
                    }),
            )
            .padding([3, 5])
            .style(theme::ghost_button)
            .on_press(Message::FilePreviewClosed(path.to_path_buf())),
        ]
        .spacing(2)
        .align_y(alignment::Vertical::Center),
    )
    .style(tab_card(active))
    .into()
}

/// The largest body the preview pane will shape and paint per frame.
/// The read path still loads up to 512 KiB; this render cap keeps the
/// software rasterizer's per-glyph work bounded on long files.
const PREVIEW_RENDER_LIMIT: usize = 64 * 1024;

/// The active tab's body: the path bar plus the classified content.
fn preview(state: &State) -> Element<'_, Message> {
    let Some(active) = state.editor.active.clone() else {
        return column![].into();
    };
    let body = state
        .editor
        .bodies
        .get(&active)
        .unwrap_or(&FileBody::Loading);

    let content: Element<'_, Message> = match body {
        FileBody::Text(body) => {
            // The body is shared (`Arc<str>`), so rebuilding the view
            // per frame only bumps a reference count. The lazy wrapper
            // then keeps the shaped paragraph alive across scroll and
            // hover frames, re-shaping only when the tab or its size
            // actually changes.
            let body = body.clone();
            lazy(
                (active.clone(), body.len()),
                move |_| -> Element<'static, Message> {
                    let mut end = body.len().min(PREVIEW_RENDER_LIMIT);
                    while !body.is_char_boundary(end) {
                        end -= 1;
                    }
                    let mut display = String::from(&body[..end]);
                    if body.len() > PREVIEW_RENDER_LIMIT {
                        display.push_str(&format!(
                            "\n\n… （预览截断：仅显示前 {} KiB）",
                            PREVIEW_RENDER_LIMIT / 1024,
                        ));
                    }
                    scrollable(
                        container(
                            text(display)
                                .size(theme::SIZE_SM)
                                .style(theme::fg)
                                .font(font::Font::MONOSPACE),
                        )
                        .width(Fill)
                        .padding(10),
                    )
                    .width(Fill)
                    .height(Fill)
                    .into()
                },
            )
            .into()
        }
        FileBody::Loading => pane_hint("加载中…"),
        FileBody::Binary => pane_hint("二进制文件暂不支持预览"),
        FileBody::TooLarge => pane_hint("文件超过预览大小上限（512 KiB）"),
        FileBody::Failed(reason) => pane_hint(&format!("无法读取文件：{reason}")),
    };

    column![
        container(
            text(active.display().to_string())
                .size(theme::SIZE_XS)
                .style(theme::faint_text),
        )
        .width(Fill)
        .padding([6, 10])
        .style(path_bar_surface),
        content,
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// A centered one-line hint for non-text bodies.
fn pane_hint(label: &str) -> Element<'static, Message> {
    container(
        text(String::from(label))
            .size(theme::SIZE_SM)
            .style(theme::dim),
    )
    .width(Fill)
    .height(Fill)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// White card with a hairline border (mirrors the sidebar divider).
fn preview_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The tab strip background, hairline-separated from the body.
fn tab_bar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::PANEL.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The path bar under the tab strip.
fn path_bar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// Active tabs get a white chip; inactive ones stay panel-gray.
fn tab_card(active: bool) -> impl Fn(&iced::Theme) -> iced::widget::container::Style {
    move |_theme| iced::widget::container::Style {
        background: Some(if active { theme::BG } else { theme::PANEL }.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}
