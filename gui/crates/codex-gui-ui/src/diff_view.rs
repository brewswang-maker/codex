//! The turn-diff overlay: changed files on the left, the selected file's
//! unified diff on the right (borrowed from AgentCodeGUI's change viewer).
//!
//! Browse-only by design: the mask dismisses the overlay, unlike the
//! approval dialog whose decision must be explicit.

use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_gui_core::FileChangeRecord;
use iced::Color;
use iced::Element;
use iced::Fill;
use iced::Font;
use iced::alignment;
use iced::widget::button;
use iced::widget::center;
use iced::widget::column;
use iced::widget::container;
use iced::widget::mouse_area;
use iced::widget::opaque;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::stack;
use iced::widget::text;

/// Diff lines above this count are truncated to keep soft rendering snappy.
const MAX_DIFF_LINES: usize = 2000;
/// Overlay dimensions, in the approval-dialog size class.
const OVERLAY_WIDTH: f32 = 860.0;
const OVERLAY_HEIGHT: f32 = 520.0;
const FILE_LIST_WIDTH: f32 = 240.0;

/// Wraps the main UI with the diff overlay while it is open.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if !state.diff_overlay_open {
        return base.into();
    }

    stack![
        base.into(),
        mouse_area(opaque(
            container(iced::widget::Space::new().width(Fill).height(Fill)).style(mask),
        ))
        .on_press(Message::DiffOverlayToggled),
        center(opaque(overlay(state))),
    ]
    .into()
}

/// Every file change record across the transcript's patch items, in
/// arrival order; also the data behind the task-bar Changes chip.
pub fn file_changes(state: &State) -> Vec<&FileChangeRecord> {
    state
        .transcript
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            codex_gui_core::Entry::FileChange { changes, .. } => Some(changes),
            _ => None,
        })
        .flatten()
        .collect()
}

/// One classified line of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineKind {
    Add,
    Delete,
    Hunk,
    Meta,
    Context,
}

/// One rendered diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

/// Classifies a unified diff into display lines; a pure function.
pub fn parse_unified_diff(diff: &str) -> Vec<DiffLine> {
    diff.lines()
        .map(|line| {
            let kind = if line.starts_with("diff --git")
                || line.starts_with("index ")
                || line.starts_with("--- ")
                || line.starts_with("+++ ")
            {
                DiffLineKind::Meta
            } else if line.starts_with("@@") {
                DiffLineKind::Hunk
            } else if line.starts_with('+') {
                DiffLineKind::Add
            } else if line.starts_with('-') {
                DiffLineKind::Delete
            } else {
                DiffLineKind::Context
            };
            DiffLine {
                kind,
                text: String::from(line),
            }
        })
        .collect()
}

/// Parses and caps a diff for rendering; returns the visible lines and
/// how many were cut.
fn visible_lines(diff: &str) -> (Vec<DiffLine>, usize) {
    let parsed = parse_unified_diff(diff);
    if parsed.len() <= MAX_DIFF_LINES {
        (parsed, 0)
    } else {
        let omitted = parsed.len() - MAX_DIFF_LINES;
        (parsed.into_iter().take(MAX_DIFF_LINES).collect(), omitted)
    }
}

fn overlay(state: &State) -> Element<'_, Message> {
    let changes = file_changes(state);

    let body: Element<'_, Message> = if changes.is_empty() {
        match &state.turn_diff {
            // No per-file items, but the backend streamed a turn diff.
            Some(turn_diff) => diff_panel(turn_diff),
            None => container(
                text("No file changes in this turn.")
                    .size(theme::SIZE_SM)
                    .style(theme::dim),
            )
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .align_y(alignment::Vertical::Center)
            .into(),
        }
    } else {
        let selected = state
            .diff_selected
            .map_or(0, |index| index.min(changes.len() - 1));

        let mut list = column![].spacing(2);
        for (index, change) in changes.iter().enumerate() {
            list = list.push(file_row(index, change, index == selected));
        }

        row![
            container(scrollable(list).width(Fill).height(Fill))
                .width(FILE_LIST_WIDTH)
                .height(Fill),
            diff_panel(&changes[selected].diff),
        ]
        .spacing(12)
        .width(Fill)
        .height(Fill)
        .into()
    };

    let count = changes.len();
    container(
        column![
            row![
                text(if count == 0 {
                    String::from("Turn changes")
                } else if count == 1 {
                    String::from("Turn changes · 1 file")
                } else {
                    format!("Turn changes · {count} files")
                })
                .size(theme::SIZE_BODY)
                .style(theme::fg),
                iced::widget::Space::new().width(Fill),
                button(text("Close").size(theme::SIZE_SM).style(theme::dim))
                    .padding([4, 10])
                    .style(quiet_close)
                    .on_press(Message::DiffOverlayToggled),
            ]
            .align_y(alignment::Vertical::Center),
            body,
        ]
        .spacing(10),
    )
    .width(Fill)
    .max_width(OVERLAY_WIDTH)
    .height(Fill)
    .max_height(OVERLAY_HEIGHT)
    .padding(12)
    .style(dialog_surface)
    .into()
}

/// One file in the left list: a kind letter badge plus the path.
fn file_row<'a>(index: usize, change: &FileChangeRecord, selected: bool) -> Element<'a, Message> {
    let (kind_color, kind_letter) = match change.kind.as_str() {
        "add" => (theme::BRAND, "A"),
        "delete" => (theme::DANGER, "D"),
        _ => (theme::GOLD, "U"),
    };
    button(
        row![
            text(kind_letter)
                .size(theme::SIZE_XS)
                .font(Font::MONOSPACE)
                .style(move |_| iced::widget::text::Style {
                    color: Some(kind_color),
                }),
            text(change.path.clone())
                .size(theme::SIZE_XS)
                .style(if selected { theme::fg } else { theme::dim })
                .width(Fill),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding([4, 6])
    .width(Fill)
    .style(move |_theme, _status| file_row_style(selected))
    .on_press(Message::DiffFileSelected(index))
    .into()
}

fn file_row_style(selected: bool) -> button::Style {
    button::Style {
        background: selected.then(|| theme::CARD.into()),
        text_color: theme::MUTED,
        border: iced::Border {
            color: if selected {
                theme::BORDER
            } else {
                Color::TRANSPARENT
            },
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..button::Style::default()
    }
}

/// The right pane: classified diff lines in a mono well, capped.
fn diff_panel(diff: &str) -> Element<'_, Message> {
    let (lines, omitted) = visible_lines(diff);

    let mut body = column![].spacing(0);
    for line in &lines {
        body = body.push(diff_row(line));
    }
    if omitted > 0 {
        body = body.push(
            container(
                text(format!("… {omitted} more lines truncated"))
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            )
            .width(Fill)
            .padding([4, 0]),
        );
    }

    container(scrollable(body).width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .padding([6, 8])
        .style(diff_well)
        .into()
}

fn diff_row(line: &DiffLine) -> Element<'static, Message> {
    let color = match line.kind {
        DiffLineKind::Add => theme::BRAND,
        DiffLineKind::Delete => theme::DANGER,
        DiffLineKind::Hunk => theme::FAINT,
        DiffLineKind::Meta | DiffLineKind::Context => theme::MUTED,
    };
    let wash = match line.kind {
        DiffLineKind::Add => Some(Color {
            a: 0.10,
            ..theme::BRAND
        }),
        DiffLineKind::Delete => Some(Color {
            a: 0.10,
            ..theme::DANGER
        }),
        _ => None,
    };
    container(
        text(line.text.clone())
            .font(Font::MONOSPACE)
            .size(theme::SIZE_XS)
            .style(move |_| iced::widget::text::Style { color: Some(color) }),
    )
    .width(Fill)
    .style(move |_| iced::widget::container::Style {
        background: wash.map(iced::Background::Color),
        ..iced::widget::container::Style::default()
    })
    .into()
}

fn mask(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(
            Color {
                a: 0.45,
                ..Color::BLACK
            }
            .into(),
        ),
        ..container::Style::default()
    }
}

/// The overlay card: white surface, hairline border, large radius (the
/// approval-dialog surface treatment).
fn dialog_surface(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_LG.into(),
        },
        ..container::Style::default()
    }
}

/// The mono well behind the diff body.
fn diff_well(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    }
}

fn quiet_close(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(theme::BG.into()),
        text_color: theme::MUTED,
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::DiffLineKind;
    use super::MAX_DIFF_LINES;
    use super::parse_unified_diff;
    use super::visible_lines;

    #[test]
    fn unified_diff_lines_are_classified() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n\
                    index 8b13789..71a1a3b 100644\n\
                    --- a/src/main.rs\n\
                    +++ b/src/main.rs\n\
                    @@ -1,3 +1,4 @@\n\
                    context line\n\
                    +added line\n\
                    -removed line\n";
        let lines = parse_unified_diff(diff);

        let kinds: Vec<_> = lines.iter().map(|line| line.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Meta,
                DiffLineKind::Meta,
                DiffLineKind::Meta,
                DiffLineKind::Meta,
                DiffLineKind::Hunk,
                DiffLineKind::Context,
                DiffLineKind::Add,
                DiffLineKind::Delete,
            ]
        );
        assert_eq!(lines[6].text, "+added line");
        assert_eq!(lines[4].text, "@@ -1,3 +1,4 @@");
    }

    #[test]
    fn long_diffs_are_truncated_with_a_count() {
        let diff: String = "+line\n".repeat(MAX_DIFF_LINES + 7);
        let (lines, omitted) = visible_lines(&diff);
        assert_eq!(lines.len(), MAX_DIFF_LINES);
        assert_eq!(omitted, 7);
    }

    #[test]
    fn short_diffs_pass_through_untouched() {
        let (lines, omitted) = visible_lines("@@ -1 +1 @@\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(omitted, 0);
    }
}
