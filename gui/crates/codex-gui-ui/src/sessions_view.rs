//! The sidebar: resumable threads grouped by project, plus a directory-tree
//! view of the working directory (Files pane). Pane switching lives in the
//! activity bar.

use crate::chat;
use crate::icons::Icon;
use crate::icons::IconKind;
use crate::message::Message;
use crate::message::SidebarTab;
use crate::state::State;
use crate::theme;
use codex_gui_core::ThreadSummary;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Fixed sidebar width; the transcript takes the rest.
const SIDEBAR_WIDTH: f32 = 270.0;

/// Renders the sidebar dispatch on the active activity-bar tab.
pub fn sidebar(state: &State) -> Element<'_, Message> {
    let (title, body): (&str, Element<'_, Message>) = match state.sidebar_tab {
        SidebarTab::Threads => ("线程列表", thread_list(state)),
        SidebarTab::Files => ("资源管理器", files_body(state)),
        SidebarTab::Search => ("搜索", search_pane(state)),
        SidebarTab::SourceControl => ("源代码管理", crate::scm_view::panel(state)),
        SidebarTab::Wiki => (
            "仓库 Wiki",
            plain_pane(
                IconKind::Wiki,
                "仓库 Wiki",
                "从仓库生成的 Wiki 页面会显示在这里。",
            ),
        ),
        SidebarTab::Debug => (
            "运行和调试",
            plain_pane(
                IconKind::Debug,
                "运行和调试",
                "暂无启动配置；内嵌终端里程碑后提供调试。",
            ),
        ),
        SidebarTab::Remote => ("远程资源管理器", crate::remote_view::panel(state)),
        SidebarTab::Extensions => ("扩展", extensions_body(state)),
    };

    let header = text(title).size(theme::SIZE_MD).style(theme::fg);

    container(
        column![header, body]
            .width(Fill)
            .height(Fill)
            .padding(8)
            .spacing(8),
    )
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .style(sidebar_surface)
    .into()
}

/// The workspace explorer pane: the project caption above the tree.
/// Working-tree changes moved to the source-control pane.
fn files_body(state: &State) -> Element<'_, Message> {
    let project = state.tree.root_path().file_name().map_or_else(
        || state.tree.root_path().display().to_string(),
        |name| name.to_string_lossy().to_uppercase(),
    );
    // The tree widget ships its own scrollable; pin the row height so the
    // sidebar clips instead of growing with the expansion.
    container(column![section_caption(project), state.tree.view(Message::Tree),].spacing(4))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The workspace search pane (VS Code search view): the query filters
/// the directory tree live, the replace well is display-only until the
/// batch-replace milestone, and the match count reports honestly.
fn search_pane(state: &State) -> Element<'_, Message> {
    let query = state.tree.search_query().unwrap_or("").to_string();
    let match_count = state.tree.search_match_count();
    let result_line: Element<'_, Message> = if state.tree.is_searching() {
        text(format!("{match_count} 个匹配文件"))
            .size(theme::SIZE_XS)
            .style(theme::dim)
            .into()
    } else {
        text("输入关键字过滤左侧目录树。")
            .size(theme::SIZE_XS)
            .style(theme::faint_text)
            .into()
    };

    column![
        text_input("搜索", &query)
            .on_input(Message::SearchQueryChanged)
            .on_submit_maybe(Some(Message::SidebarTab(SidebarTab::Files)))
            .size(theme::SIZE_SM)
            .padding([5, 8])
            .style(theme::bare_input)
            .width(Fill),
        text_input("替换", &state.search_replace)
            .on_input(Message::SearchReplaceChanged)
            .size(theme::SIZE_SM)
            .padding([5, 8])
            .style(theme::bare_input)
            .width(Fill),
        result_line,
        text("批量替换即将提供；当前按文件名过滤目录树。")
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    ]
    .spacing(6)
    .padding([4, 2])
    .into()
}

/// The extensions pane: marketplace scaffold with honest empty groups.
fn extensions_body(state: &State) -> Element<'_, Message> {
    column![
        text_input("在应用商店中搜索扩展", &state.extension_filter)
            .on_input(Message::ExtensionFilterChanged)
            .size(theme::SIZE_XS)
            .padding([5, 8])
            .style(theme::bare_input)
            .width(Fill),
        group_caption("已安装", Some(0)),
        text("内置工具（codex-gui、codex-cli、codex-mcp）以内置能力提供，不在此列出。")
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
        group_caption("热门", None),
        text("找不到扩展。").size(theme::SIZE_XS).style(theme::dim),
        group_caption("推荐", Some(0)),
    ]
    .spacing(6)
    .padding([4, 2])
    .into()
}

/// A shared empty-pane hint: brand glyph, title, and one-line caption.
pub(crate) fn plain_pane(
    icon: IconKind,
    title: &'static str,
    hint: &'static str,
) -> Element<'static, Message> {
    column![
        Icon::new(icon, theme::BRAND, 28.0).widget(),
        text(title).size(theme::SIZE_MD).style(theme::fg),
        text(hint).size(theme::SIZE_XS).style(theme::dim),
    ]
    .spacing(8)
    .padding([12, 2])
    .into()
}

/// Threads grouped under their project directory, pinned ones on top,
/// both filtered by the sidebar search box (ZCode TaskList borrow); the
/// archived partition folds out underneath.
fn thread_list(state: &State) -> Element<'_, Message> {
    let now = now_secs();
    let mut list = column![].spacing(6);

    let filter_row = text_input("Filter threads…", &state.sidebar_filter)
        .size(theme::SIZE_XS)
        .on_input(Message::SidebarFilterChanged)
        .padding([4, 8])
        .width(Fill);
    list = list.push(container(filter_row).style(filter_well));

    if state.sessions.threads().is_empty() {
        list = list.push(text("no threads yet").style(theme::dim));
    } else {
        let (pinned, groups) = codex_gui_core::organized_sidebar(
            state.sessions.threads(),
            state.pins.id_set(),
            &state.sidebar_filter,
        );

        if pinned.is_empty() && groups.is_empty() {
            list = list.push(text("no matching threads").style(theme::dim));
        }
        if !pinned.is_empty() {
            list = list.push(group_label("Pinned"));
            for thread in &pinned {
                list = list.push(thread_row(state, thread, now));
            }
        }
        for (cwd, threads) in groups {
            if threads.is_empty() {
                continue;
            }
            list = list.push(group_header(cwd));
            for thread in threads {
                list = list.push(thread_row(state, thread, now));
            }
        }
    }

    list = list.push(archived_section(state, now));

    container(scrollable(list.padding([2, 0])).height(Fill))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The archived partition under the live list: a fold header carrying the
/// count, then one restore row per archived thread once expanded.
fn archived_section(state: &State, now: i64) -> Element<'_, Message> {
    let marker = if state.archived_open { "▾" } else { "▸" };
    let header = button(
        row![
            text(format!("{marker} Archived"))
                .size(theme::SIZE_XS)
                .style(theme::fg)
                .font(bold()),
            text(state.archived.threads().len().to_string())
                .size(theme::SIZE_XS)
                .style(faint),
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding([4, 6])
    .style(theme::ghost_button)
    .on_press(Message::ArchivedToggled);

    let mut body = column![].spacing(6);
    if state.archived_open {
        if !state.archived_loaded {
            body = body.push(
                text("loading archived threads…")
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            );
        } else if state.archived.threads().is_empty() {
            body = body.push(
                text("no archived threads")
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            );
        } else {
            for thread in state.archived.threads() {
                body = body.push(archived_row(thread, now));
            }
        }
    }

    column![header, body].spacing(4).into()
}

/// One archived thread row: label, relative age, and the restore action.
fn archived_row(thread: &ThreadSummary, now: i64) -> Element<'_, Message> {
    let preview = if thread.preview.is_empty() {
        String::from("(untitled)")
    } else {
        String::from(thread.preview.as_str())
    };
    let restore = button(text("restore").size(theme::SIZE_XS).style(faint))
        .padding([2, 6])
        .style(theme::ghost_button)
        .on_press(Message::SessionUnarchived(thread.id.clone()));

    container(
        row![
            container(text(preview).size(theme::SIZE_MD).style(theme::dim)).width(Fill),
            text(chat::relative_time(thread.updated_at, now))
                .size(theme::SIZE_XS)
                .style(faint),
            restore,
        ]
        .spacing(6)
        .align_y(alignment::Vertical::Center)
        .padding([4, 2]),
    )
    .width(Fill)
    .into()
}

/// The pinned partition header.
fn group_label(label: &str) -> Element<'static, Message> {
    text(String::from(label))
        .size(theme::SIZE_XS)
        .style(theme::fg)
        .font(bold())
        .into()
}

/// The filter box well: hairline-bordered input surface (shared with the
/// Quest rail's filter).
pub(crate) fn filter_well(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            radius: theme::RADIUS_SM.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// One project header: the directory's display name in small caps color.
fn group_header(cwd: &str) -> Element<'static, Message> {
    let name = std::path::Path::new(cwd).file_name().map_or_else(
        || String::from(cwd),
        |name| name.to_string_lossy().into_owned(),
    );
    text(name)
        .size(theme::SIZE_XS)
        .style(theme::fg)
        .font(bold())
        .into()
}

/// One thread row: white card while active, preview plus relative age,
/// a pin toggle, and (on the active row) the live diff totals badge.
fn thread_row<'a>(state: &'a State, thread: &'a ThreadSummary, now: i64) -> Element<'a, Message> {
    let id = thread.id.clone();
    let active = state.thread_id.as_deref() == Some(thread.id.as_str());

    let preview = if thread.preview.is_empty() {
        String::from("(untitled)")
    } else {
        String::from(thread.preview.as_str())
    };
    let label = text(preview)
        .size(theme::SIZE_MD)
        .style(theme::fg)
        .font(bold_if(active));

    let age = chat::relative_time(thread.updated_at, now);
    let pinned = state.pins.is_pinned(&thread.id);
    let pin = button(
        text(if pinned { "unpin" } else { "pin" })
            .size(theme::SIZE_XS)
            .style(faint),
    )
    .padding([2, 6])
    .style(theme::ghost_button)
    .on_press(Message::SessionPinToggled(id));
    let archive = button(text("archive").size(theme::SIZE_XS).style(faint))
        .padding([2, 6])
        .style(theme::ghost_button)
        .on_press(Message::SessionArchived(thread.id.clone()));

    let mut trailing = row![].spacing(6).align_y(alignment::Vertical::Center);
    if active {
        let (added, removed) = state.transcript.diff_totals();
        if added > 0 || removed > 0 {
            trailing = trailing
                .push(
                    text(format!("+{added}"))
                        .size(theme::SIZE_XS)
                        .style(added_style),
                )
                .push(
                    text(format!("-{removed}"))
                        .size(theme::SIZE_XS)
                        .style(removed_style),
                );
        }
    }
    trailing = trailing
        .push(text(age).size(theme::SIZE_XS).style(faint))
        .push(pin)
        .push(archive);

    container(
        button(row![container(label).width(Fill), trailing,])
            .padding([8, 10])
            .style(theme::rail_button(active))
            .on_press(Message::SessionSelected(thread.id.clone()))
            .width(Fill),
    )
    .width(Fill)
    .style(row_card(active))
    .into()
}

/// Added-line badge: brand green.
fn added_style(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::BRAND),
    }
}

/// Removed-line badge: danger red.
fn removed_style(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::DANGER),
    }
}

/// The row card: hairline-bordered white chip only while active.
fn row_card(active: bool) -> impl Fn(&iced::Theme) -> iced::widget::container::Style {
    move |_theme| {
        if active {
            iced::widget::container::Style {
                background: Some(theme::BG.into()),
                border: iced::Border {
                    color: theme::BORDER,
                    width: 1.0,
                    radius: theme::RADIUS_SM.into(),
                },
                ..iced::widget::container::Style::default()
            }
        } else {
            iced::widget::container::Style::default()
        }
    }
}

/// An uppercase group caption, optionally with a trailing count badge.
fn group_caption(label: &'static str, count: Option<usize>) -> Element<'static, Message> {
    let caption = match count {
        Some(count) => format!("{label} {count}"),
        None => label.to_string(),
    };
    section_caption(caption)
}

/// An uppercase section caption (VS Code group header style).
fn section_caption(label: String) -> Element<'static, Message> {
    container(
        text(label.to_uppercase())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    )
    .width(Fill)
    .padding([2, 0])
    .into()
}

/// The sidebar surface with its hairline divider from the main pane.
fn sidebar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::SIDEBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

fn faint(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::FAINT),
    }
}

fn bold() -> font::Font {
    font::Font {
        weight: font::Weight::Bold,
        ..font::Font::default()
    }
}

/// A default font with bold weight applied or not.
fn bold_if(active: bool) -> font::Font {
    if active {
        bold()
    } else {
        font::Font::default()
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_secs() as i64)
}

#[cfg(test)]
#[path = "sessions_view_tests.rs"]
mod tests;
