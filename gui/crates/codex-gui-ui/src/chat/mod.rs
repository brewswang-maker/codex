//! The chat pane: the transcript rendered with streaming markdown.
//!
//! Layout follows the reference conversation view: user turns as gray
//! rounded cards on the right, agent replies as plain markdown with a
//! copy/react action bar, command items as filled mono cards.

mod markdown_view;

use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::message::Reaction;
use crate::state::State;
use crate::theme;
use codex_app_server_protocol::ThreadTokenUsage;
use codex_gui_core::Entry;
use codex_gui_core::FileChangeRecord;
use codex_gui_core::TurnPlan;
use iced::Element;
use iced::Fill;
use iced::Font;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::image;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Scrollable id targeted by `operation::snap_to_end`.
pub const TRANSCRIPT_ID: &str = "transcript";

/// Text-input id of the transcript search bar, focused when it opens.
pub const SEARCH_INPUT_ID: &str = "chat-search-input";

/// Reading-width cap for the conversation column, mirroring the centered
/// column of the reference layout.
const CONTENT_WIDTH: f32 = 760.0;
/// Width cap for one user bubble.
const USER_BUBBLE_WIDTH: f32 = 560.0;

/// Renders the conversation transcript.
pub fn transcript_view(state: &State) -> Element<'_, Message> {
    let searching = state.chat_search.open && !state.chat_search.query.trim().is_empty();
    let hits = if searching {
        codex_gui_core::search(state.transcript.entries(), &state.chat_search.query)
    } else {
        Vec::new()
    };

    if state.transcript.entries().is_empty() && state.transcript.plan().is_none() && !searching {
        // The welcome headline and its recents list belong to the
        // pre-project state; once a project is open the pane stays a bare
        // conversation surface above the composer.
        return match state.status_board.cwd() {
            Some(_) => iced::widget::Space::new().width(Fill).height(Fill).into(),
            None => container(crate::welcome_view::welcome(state))
                .width(Fill)
                .height(Fill)
                .center_x(Fill)
                .align_y(alignment::Vertical::Center)
                .into(),
        };
    }

    let body: Element<'_, Message> = if searching {
        filtered_body(state, &hits)
    } else {
        full_body(state).into()
    };

    // Center the reading column inside the scrollable so wide windows keep
    // the reference proportions; the search bar stays pinned above it.
    let scroll = scrollable(
        container(body)
            .max_width(CONTENT_WIDTH)
            .width(Fill)
            .padding([12, 24]),
    )
    .id(TRANSCRIPT_ID)
    .width(Fill)
    .height(Fill);

    if state.chat_search.open {
        column![search_bar(state, hits.len()), scroll]
            .width(Fill)
            .into()
    } else {
        scroll.into()
    }
}

/// The unfiltered transcript: the live plan, then every entry, with runs
/// of consecutive finished commands collapsed into group cards.
fn full_body(state: &State) -> iced::widget::Column<'_, Message> {
    let now = now_secs();
    let mut body = column![].spacing(16);
    if let Some(plan) = state.transcript.plan() {
        body = body.push(plan_pane(plan));
    }
    let entries = state.transcript.entries();
    for row in transcript_rows(entries) {
        match row {
            TranscriptRow::Entry(index) => {
                body = body.push(render_entry(state, &entries[index], index, now));
            }
            TranscriptRow::Group(start, end) => {
                body = body.push(command_group_card(state, start, end));
            }
        }
    }
    body
}

/// The search-filtered transcript: only matching entries, the selected one
/// outlined so prev/next reads as navigation (no scroll-to-widget support).
fn filtered_body<'a>(state: &'a State, hits: &[codex_gui_core::SearchHit]) -> Element<'a, Message> {
    if hits.is_empty() {
        return container(text("No matches").style(theme::dim))
            .width(Fill)
            .center_x(Fill)
            .padding([12, 0])
            .into();
    }

    let now = now_secs();
    let selected = hits[state.chat_search.hit.min(hits.len() - 1)].entry_index;
    let mut body = column![].spacing(16);
    for hit in hits {
        let Some(entry) = state.transcript.entries().get(hit.entry_index) else {
            continue;
        };
        let card = render_entry(state, entry, hit.entry_index, now);
        body = body.push(
            container(card)
                .width(Fill)
                .padding(4)
                .style(hit_card(hit.entry_index == selected)),
        );
    }
    body.into()
}

/// Moves the highlighted search hit, wrapping at both ends; a query with
/// no hits resets the cursor.
pub fn step_search_hit(state: &mut State, delta: i32) {
    let hits = codex_gui_core::search(state.transcript.entries(), &state.chat_search.query);
    if hits.is_empty() {
        state.chat_search.hit = 0;
        return;
    }
    let len = hits.len() as i32;
    state.chat_search.hit = (state.chat_search.hit as i32 + delta).rem_euclid(len) as usize;
}

/// The search bar pinned above the transcript while search is open.
fn search_bar<'a>(state: &'a State, hit_count: usize) -> Element<'a, Message> {
    let count_label = if state.chat_search.query.trim().is_empty() {
        String::from("Type to filter")
    } else if hit_count == 0 {
        String::from("no matches")
    } else {
        format!(
            "{}/{}",
            state.chat_search.hit.min(hit_count - 1) + 1,
            hit_count
        )
    };

    let bar = row![
        Icon::new(IconKind::Search, theme::MUTED, 14.0).widget(),
        text_input("Search transcript…", &state.chat_search.query)
            .id(SEARCH_INPUT_ID)
            .size(theme::SIZE_SM)
            .on_input(Message::ChatSearchQueryChanged)
            .width(260),
        text(count_label).size(theme::SIZE_XS).style(faint),
        button(text("‹").size(theme::SIZE_SM).style(theme::fg))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::ChatSearchPrev),
        button(text("›").size(theme::SIZE_SM).style(theme::fg))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::ChatSearchNext),
        button(text("esc").size(theme::SIZE_XS).style(faint))
            .padding([2, 8])
            .style(theme::ghost_button)
            .on_press(Message::ChatSearchClosed),
        iced::widget::Space::new().width(Fill),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    container(bar)
        .width(Fill)
        .padding([6, 24])
        .style(search_bar_style)
        .into()
}

/// The pinned search-bar surface: panel gray with a hairline bottom edge.
fn search_bar_style(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::PANEL.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 0.5,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The filter-view wrapper: the selected hit gets an accent outline, the
/// rest stay plain.
fn hit_card(highlight: bool) -> impl Fn(&iced::Theme) -> iced::widget::container::Style {
    move |_theme| {
        if highlight {
            iced::widget::container::Style {
                border: iced::Border {
                    color: theme::BRAND,
                    width: 1.5,
                    radius: theme::RADIUS_SM.into(),
                },
                ..iced::widget::container::Style::default()
            }
        } else {
            iced::widget::container::Style::default()
        }
    }
}

/// One render row of the unfiltered transcript: a single entry (by its
/// transcript index), or a run of consecutive finished command entries
/// shown as one collapsed card.
enum TranscriptRow {
    Entry(usize),
    /// Half-open `[start, end)` range into the transcript entries.
    Group(usize, usize),
}

/// Groups runs of two-or-more finished command entries; anything else (and
/// a still-running tail command) renders row by row.
fn transcript_rows(entries: &[Entry]) -> Vec<TranscriptRow> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        let run_end = match &entries[index] {
            Entry::CommandExecution {
                exit_code: Some(_), ..
            } => {
                let mut end = index + 1;
                while matches!(
                    entries.get(end),
                    Some(Entry::CommandExecution {
                        exit_code: Some(_),
                        ..
                    })
                ) {
                    end += 1;
                }
                end
            }
            _ => index + 1,
        };
        if run_end - index > 1 {
            rows.push(TranscriptRow::Group(index, run_end));
        } else {
            rows.push(TranscriptRow::Entry(index));
        }
        index = run_end;
    }
    rows
}

/// Whether the agent message at `index` closes its turn: it is the last
/// agent message before the next user message (or the end of the
/// transcript). Only the closer renders the copy/react action bar -- one
/// end-of-task marker per turn, not one per streaming segment.
fn agent_message_closes_turn(entries: &[Entry], index: usize) -> bool {
    for entry in &entries[index + 1..] {
        match entry {
            // A new user turn starts: this message closed its turn.
            Entry::UserMessage { .. } => return true,
            // A later agent message in the same turn: not the closer.
            Entry::AgentMessage { .. } => return false,
            Entry::CommandExecution { .. } | Entry::FileChange { .. } => {}
        }
    }
    // Transcript tail: the live (or latest) turn's closing message.
    true
}

/// One collapsed run of finished command cards: a summary header that
/// expands to the individual cards (ZCode execute-group borrow).
fn command_group_card<'a>(state: &'a State, start: usize, end: usize) -> Element<'a, Message> {
    let entries = state.transcript.entries();
    let run = &entries[start..end];
    let Entry::CommandExecution { id: group_id, .. } = &run[0] else {
        // `transcript_rows` only emits groups over command runs.
        return render_entry(state, &entries[start], start, now_secs());
    };
    let expanded = state.expanded_command_groups.contains(group_id);

    let failed = run
        .iter()
        .filter(|entry| {
            matches!(entry, Entry::CommandExecution { exit_code: Some(code), .. } if *code != 0)
        })
        .count();
    let total_ms: i64 = run
        .iter()
        .filter_map(|entry| match entry {
            Entry::CommandExecution { duration_ms, .. } => *duration_ms,
            _ => None,
        })
        .sum();
    let mut label = format!("Ran {} commands", run.len());
    if failed > 0 {
        label.push_str(&format!(" · {failed} failed"));
    }
    if total_ms > 0 {
        label.push_str(&format!(" · {}", format_duration(total_ms)));
    }
    let marker = if expanded { "▾" } else { "▸" };

    let mut card = column![
        button(
            text(format!("{marker} {label}"))
                .size(theme::SIZE_SM)
                .style(theme::fg)
        )
        .padding([2, 4])
        .style(theme::ghost_button)
        .on_press(Message::CommandGroupToggled(String::from(group_id)))
        .width(Fill),
    ]
    .spacing(4);

    if expanded {
        for entry in run {
            if let Entry::CommandExecution {
                id,
                command,
                output,
                exit_code,
                duration_ms,
            } = entry
            {
                card = card.push(command_card(
                    state,
                    id,
                    command,
                    output,
                    *exit_code,
                    *duration_ms,
                ));
            }
        }
    }

    container(card)
        .width(Fill)
        .padding([8, 12])
        .style(command_card_style)
        .into()
}

/// The live turn plan above the conversation entries.
fn plan_pane(plan: &TurnPlan) -> Element<'_, Message> {
    let mut pane = column![text("Plan").style(theme::dim)];
    if let Some(explanation) = &plan.explanation {
        pane = pane.push(text(explanation.clone()).style(theme::dim));
    }
    for step in &plan.steps {
        pane = pane.push(text(format!("• {} [{}]", step.step, step.status)));
    }
    container(pane.spacing(4).padding([6, 10]))
        .width(Fill)
        .style(theme::surface(theme::PANEL))
        .into()
}

/// The task bar above the composer: Plan and Context chips. Each chip
/// only shows once its backing data exists (a live plan, backend usage).
pub fn taskbar(state: &State) -> Option<Element<'_, Message>> {
    let plan = state.transcript.plan();
    let usage = state.token_usage.as_ref();
    let changes = crate::diff_view::file_changes(state);
    if plan.is_none() && usage.is_none() && changes.is_empty() {
        return None;
    }

    let mut chips = row![].spacing(8);
    if let Some(plan) = plan {
        let active = plan
            .steps
            .iter()
            .filter(|step| step.status == "in progress")
            .count();
        chips = chips.push(
            button(
                text(format!(
                    "Plan · {} steps · {} active",
                    plan.steps.len(),
                    active
                ))
                .size(theme::SIZE_XS)
                .style(theme::dim),
            )
            .padding([3, 10])
            .style(chip_button)
            .on_press(Message::PlanOverlayToggled),
        );
    }
    if !changes.is_empty() {
        let count = changes.len();
        let noun = if count == 1 { "file" } else { "files" };
        chips = chips.push(
            button(
                text(format!("Changes · {count} {noun}"))
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            )
            .padding([3, 10])
            .style(chip_button)
            .on_press(Message::DiffOverlayToggled),
        );
    }
    if let Some(usage) = usage {
        chips = chips.push(chip(context_label(usage)));
    }

    Some(container(chips).width(Fill).max_width(CONTENT_WIDTH).into())
}

/// One pill chip in the task bar.
fn chip(label: String) -> Element<'static, Message> {
    container(text(label).size(theme::SIZE_XS).style(theme::dim))
        .padding([3, 10])
        .style(chip_style)
        .into()
}

/// `Context · 12.4k · 5%`, dropping the percentage when the context
/// window size is unknown.
fn context_label(usage: &ThreadTokenUsage) -> String {
    let total = usage.total.total_tokens;
    match usage.model_context_window {
        Some(window) if window > 0 => {
            format!(
                "Context · {} · {}%",
                fmt_tokens(total),
                total * 100 / window
            )
        }
        _ => format!("Context · {}", fmt_tokens(total)),
    }
}

/// Compact token count: `940`, `12.4k`, `1.2M`.
fn fmt_tokens(tokens: i64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens}")
    }
}

/// The pill surface: panel gray with a hairline border.
fn chip_style(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::PANEL.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// The clickable Changes chip: the same pill, as a button.
fn chip_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(theme::PANEL.into()),
        text_color: theme::MUTED,
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..button::Style::default()
    }
}

fn render_entry<'a>(
    state: &'a State,
    entry: &'a Entry,
    index: usize,
    now: i64,
) -> Element<'a, Message> {
    match entry {
        Entry::UserMessage {
            id,
            text: body,
            images,
        } => user_message(state, id, body, images, now),
        Entry::AgentMessage { id, text } => agent_message(
            state,
            id,
            text,
            agent_message_closes_turn(state.transcript.entries(), index),
        ),
        Entry::CommandExecution {
            id,
            command,
            output,
            exit_code,
            duration_ms,
        } => command_card(state, id, command, output, *exit_code, *duration_ms),
        Entry::FileChange { changes, .. } => file_change_card(changes),
    }
}

/// One file-change item: a single summary row; the per-file diffs live
/// in the overlay opened from the task-bar Changes chip.
fn file_change_card(changes: &[FileChangeRecord]) -> Element<'static, Message> {
    let adds = changes.iter().filter(|c| c.kind == "add").count();
    let updates = changes.iter().filter(|c| c.kind == "update").count();
    let deletes = changes.iter().filter(|c| c.kind == "delete").count();

    let mut detail = Vec::new();
    if adds > 0 {
        detail.push(format!("+{adds}"));
    }
    if updates > 0 {
        detail.push(format!("~{updates}"));
    }
    if deletes > 0 {
        detail.push(format!("-{deletes}"));
    }

    let files = if changes.len() == 1 {
        String::from("1 file changed")
    } else {
        format!("{} files changed", changes.len())
    };

    container(
        row![
            text(files).size(theme::SIZE_SM).style(theme::fg),
            iced::widget::Space::new().width(Fill),
            text(detail.join(" "))
                .size(theme::SIZE_XS)
                .font(Font::MONOSPACE)
                .style(theme::dim),
        ]
        .spacing(8),
    )
    .width(Fill)
    .padding([8, 12])
    .style(command_card_style)
    .into()
}

/// One user turn: right-aligned gray card with a faint timestamp below.
fn user_message<'a>(
    state: &'a State,
    id: &str,
    body: &str,
    images: &[std::path::PathBuf],
    now: i64,
) -> Element<'a, Message> {
    let mut card = column![
        text(String::from(body))
            .size(theme::SIZE_BODY)
            .style(theme::fg)
    ]
    .spacing(8)
    .padding([10, 14]);

    if !images.is_empty() {
        let mut thumbnails = row![].spacing(6);
        for path in images {
            thumbnails = thumbnails.push(image(image::Handle::from_path(path)).height(120));
        }
        card = card.push(thumbnails);
    }

    let bubble = container(card)
        .max_width(USER_BUBBLE_WIDTH)
        .style(user_bubble);

    let stamp = state
        .user_times
        .get(id)
        .map_or_else(String::new, |sent| relative_time(*sent, now));
    let stamp = text(stamp).size(theme::SIZE_XS).style(faint);

    container(
        column![
            row![iced::widget::Space::new().width(Fill), bubble],
            row![iced::widget::Space::new().width(Fill), stamp],
        ]
        .spacing(2),
    )
    .width(Fill)
    .into()
}

/// One agent message: markdown body; the turn-closing message also
/// renders the copy/react action bar (one end-of-task marker per turn).
fn agent_message<'a>(
    state: &'a State,
    id: &str,
    text_body: &str,
    closes_turn: bool,
) -> Element<'a, Message> {
    // Deltas reach the parsed cache on the render tick, so a message may
    // briefly exist only in the transcript.
    let body: Element<'_, Message> = match state.markdowns.get(id) {
        Some(content) => markdown_view::render(content),
        None => text(String::new()).into(),
    };

    let mut card = column![body].spacing(6);
    if closes_turn {
        let copy_kind = if state.copied_id.as_deref() == Some(id) {
            theme::BRAND
        } else {
            theme::MUTED
        };
        let up_kind = if state.reactions.get(id) == Some(&Reaction::Up) {
            theme::BRAND
        } else {
            theme::MUTED
        };
        let down_kind = if state.reactions.get(id) == Some(&Reaction::Down) {
            theme::DANGER
        } else {
            theme::MUTED
        };

        let payload = String::from(text_body);
        card = card.push(
            row![
                icon_action(
                    IconKind::Copy,
                    copy_kind,
                    Message::CopyMessage {
                        id: String::from(id),
                        text: payload,
                    },
                ),
                icon_action(
                    IconKind::ThumbUp,
                    up_kind,
                    Message::ReactionToggled {
                        id: String::from(id),
                        reaction: Reaction::Up,
                    },
                ),
                icon_action(
                    IconKind::ThumbDown,
                    down_kind,
                    Message::ReactionToggled {
                        id: String::from(id),
                        reaction: Reaction::Down,
                    },
                ),
                iced::widget::Space::new().width(Fill),
                text("AI-generated").size(theme::SIZE_XS).style(faint),
            ]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
        );
    }

    container(card.padding([6, 4])).width(Fill).into()
}

/// One command tool card: a clickable header row (`$ command` plus a
/// status chip); expanding it reveals the output well with the exit code
/// and wall duration.
fn command_card<'a>(
    state: &'a State,
    id: &str,
    command: &str,
    output: &str,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
) -> Element<'a, Message> {
    let (status_color, status_label) = match (exit_code, duration_ms) {
        (Some(0), Some(ms)) => (theme::BRAND, format_duration(ms)),
        (Some(0), None) => (theme::BRAND, String::from("done")),
        (Some(code), Some(ms)) => (
            theme::DANGER,
            format!("exit {code} · {}", format_duration(ms)),
        ),
        (Some(code), None) => (theme::DANGER, format!("exit {code}")),
        (None, _) => (theme::MUTED, String::from("running…")),
    };

    let header = row![
        text(format!("$ {command}"))
            .font(Font::MONOSPACE)
            .size(theme::SIZE_SM)
            .style(theme::fg),
        iced::widget::Space::new().width(Fill),
        text(status_label)
            .size(theme::SIZE_XS)
            .style(move |_| iced::widget::text::Style {
                color: Some(status_color),
            }),
    ]
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    let mut card = column![
        button(header)
            .padding([2, 4])
            .style(theme::ghost_button)
            .on_press(Message::CommandCardToggled(String::from(id)))
            .width(Fill)
    ]
    .spacing(4);

    if state.expanded_commands.contains(id) {
        let body: Element<'_, Message> = if output.is_empty() {
            text("(no output)").size(theme::SIZE_XS).style(faint).into()
        } else {
            scrollable(
                text(String::from(output))
                    .font(Font::MONOSPACE)
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            )
            .height(220)
            .width(Fill)
            .into()
        };
        card = card.push(container(body).padding([4, 8]).style(output_well));
    }

    container(card)
        .width(Fill)
        .padding([8, 12])
        .style(command_card_style)
        .into()
}

/// The output well: white surface with a hairline border.
fn output_well(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BG.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// Short wall-clock label: `940ms`, `1.2s`, `14.0s`.
fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// A borderless canvas-icon button for the action bar.
fn icon_action(kind: IconKind, color: iced::Color, on_press: Message) -> Element<'static, Message> {
    button(
        container(Icon::new(kind, color, 15.0).widget())
            .align_x(alignment::Horizontal::Center)
            .width(Fill),
    )
    .padding(4)
    .style(theme::ghost_button)
    .on_press(on_press)
    .into()
}

/// The user-bubble card: panel gray with the medium radius.
fn user_bubble(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::PANEL.into()),
        border: iced::Border {
            radius: theme::RADIUS_MD.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The command card: filled surface with a small radius.
fn command_card_style(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// Faint text for timestamps and attributions.
fn faint(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::FAINT),
    }
}

/// Human-short relative age: `just now`, `5m ago`, `3h ago`, `2d ago`.
pub fn relative_time(sent_at: i64, now: i64) -> String {
    let seconds = (now - sent_at).max(0);
    if seconds < 60 {
        String::from("just now")
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

/// Unix seconds via the std clock; mirrors the update-loop helper.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_secs() as i64)
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
