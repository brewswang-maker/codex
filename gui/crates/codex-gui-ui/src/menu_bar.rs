//! The VS Code-style main menu bar of editor mode: eight dropdowns laid
//! over the window while one is open.
//!
//! Entries reuse [`Message`]s, so every menu item runs the exact code path
//! its regular control does.

use crate::message::AppMode;
use crate::message::MenuId;
use crate::message::Message;
use crate::message::SidebarTab;
use crate::state::State;
use crate::theme;
use codex_gui_core::Entry;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::stack;
use iced::widget::text;

/// Height of the menu strip, shared by the bar and the dropdown anchor.
pub const MENU_BAR_HEIGHT: f32 = 30.0;

/// Left inset of the bar content.
const BAR_PADDING: f32 = 8.0;
/// The brand mark slot at the bar's left edge.
const LOGO_SLOT: f32 = 24.0;
/// Fixed width of one menu title, so dropdowns can be anchored below it.
const ITEM_WIDTH: f32 = 74.0;
/// Width of one dropdown panel.
const PANEL_WIDTH: f32 = 260.0;

/// One row of a dropdown.
///
/// The size skew comes from [`Message`]'s protocol payloads versus the
/// label-only variants; boxes would contaminate every construction site.
#[allow(clippy::large_enum_variant)]
enum MenuEntry {
    /// A clickable item running `action`.
    Action {
        label: String,
        shortcut: Option<&'static str>,
        action: Message,
    },
    /// A greyed-out placeholder for commands the shell cannot run yet.
    Disabled {
        label: &'static str,
        shortcut: Option<&'static str>,
    },
    /// A hairline separator between item groups.
    Separator,
}

/// The menu strip: brand mark plus the eight dropdown titles.
pub fn menu_bar(state: &State) -> Element<'_, Message> {
    let logo = container(iced::widget::Space::new())
        .width(16.0)
        .height(16.0)
        .style(logo_mark);

    let mut titles = row![].align_y(alignment::Vertical::Center);
    for id in MenuId::ALL {
        titles = titles.push(title_button(id, state.menu == Some(id)));
    }

    container(
        row![logo, titles]
            .spacing(6)
            .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .height(MENU_BAR_HEIGHT)
    .padding(
        iced::Padding::default()
            .right(BAR_PADDING)
            .left(BAR_PADDING),
    )
    .style(bar_surface)
    .into()
}

/// One menu title: a quiet chip that lights up while its dropdown is open.
fn title_button(id: MenuId, open: bool) -> Element<'static, Message> {
    button(
        container(text(id.title()).size(theme::SIZE_SM).style(theme::fg))
            .width(Fill)
            .align_x(alignment::Horizontal::Center),
    )
    .width(ITEM_WIDTH)
    .padding([4, 0])
    .style(theme::rail_button(open))
    .on_press(Message::MenuToggled(id))
    .into()
}

/// Lays the open dropdown over `base` (no mask: clicking elsewhere keeps
/// the window interactive, Esc or a title click closes it).
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let Some(id) = state.menu else {
        return base.into();
    };

    let index = MenuId::ALL.iter().position(|menu| *menu == id).unwrap_or(0);
    let left = BAR_PADDING + LOGO_SLOT + 6.0 + index as f32 * ITEM_WIDTH;

    let panel = container(dropdown(id, state))
        .width(PANEL_WIDTH)
        .style(panel_surface);

    let placement = container(panel)
        .width(Fill)
        .height(Fill)
        .padding(iced::Padding::default().top(MENU_BAR_HEIGHT).left(left))
        .align_x(alignment::Horizontal::Left)
        .align_y(alignment::Vertical::Top);

    stack![base.into(), placement].into()
}

/// The dropdown body for one menu.
fn dropdown<'a>(id: MenuId, state: &'a State) -> Element<'a, Message> {
    let mut list = column![].spacing(1).padding(4);
    for entry in entries(id, state) {
        list = list.push(entry_row(entry));
    }
    list.into()
}

/// One rendered dropdown row.
fn entry_row(entry: MenuEntry) -> Element<'static, Message> {
    match entry {
        MenuEntry::Action {
            label,
            shortcut,
            action,
        } => button(row![
            text(label).size(theme::SIZE_SM).style(theme::fg),
            iced::widget::Space::new().width(Fill),
            shortcut_text(shortcut),
        ])
        .width(Fill)
        .padding([5, 10])
        .style(theme::ghost_button)
        .on_press(action)
        .into(),
        MenuEntry::Disabled { label, shortcut } => container(row![
            text(String::from(label))
                .size(theme::SIZE_SM)
                .style(theme::faint_text),
            iced::widget::Space::new().width(Fill),
            shortcut_text(shortcut),
        ])
        .width(Fill)
        .padding([5, 10])
        .into(),
        MenuEntry::Separator => column![
            iced::widget::Space::new().height(3.0),
            container(iced::widget::Space::new().height(1.0))
                .width(Fill)
                .style(separator_line),
            iced::widget::Space::new().height(3.0),
        ]
        .into(),
    }
}

/// The shortcut hint at a row's right edge.
fn shortcut_text(shortcut: Option<&'static str>) -> Element<'static, Message> {
    match shortcut {
        Some(keys) => text(keys)
            .size(theme::SIZE_XS)
            .style(theme::faint_text)
            .into(),
        None => text("").size(theme::SIZE_XS).into(),
    }
}

/// The entry lists, in reference order.
fn entries(id: MenuId, state: &State) -> Vec<MenuEntry> {
    match id {
        MenuId::File => {
            let mut list = vec![
                action("新建 Quest", Some("Ctrl+N"), Message::NewThreadRequested),
                action("打开文件夹…", None, Message::FolderPickRequested),
            ];
            let recents = state.recents.entries();
            if !recents.is_empty() {
                list.push(MenuEntry::Separator);
                for path in recents.iter().take(3) {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
                    list.push(MenuEntry::Action {
                        label: name,
                        shortcut: None,
                        action: Message::FolderPicked(Some(std::path::PathBuf::from(path))),
                    });
                }
            }
            list.push(MenuEntry::Separator);
            list.push(action("关闭菜单", Some("Esc"), Message::MenuClosed));
            list
        }
        MenuId::Edit => vec![
            MenuEntry::Disabled {
                label: "撤销",
                shortcut: Some("Ctrl+Z"),
            },
            MenuEntry::Disabled {
                label: "重做",
                shortcut: Some("Ctrl+Y"),
            },
            MenuEntry::Separator,
            action("清空输入框", None, Message::ComposerCleared),
            copy_last_reply(state, "复制最后回复"),
        ],
        MenuId::Selection => vec![
            MenuEntry::Disabled {
                label: "全选",
                shortcut: Some("Ctrl+A"),
            },
            MenuEntry::Separator,
            copy_last_reply(state, "复制最后一条回复"),
        ],
        MenuId::View => {
            let mode_label = match state.mode {
                AppMode::Editor => "切换到 Quest 模式",
                AppMode::Quest => "返回编辑器",
            };
            vec![
                action("搜索面板", Some("Ctrl+F"), Message::ChatSearchOpened),
                action("命令面板", Some("Ctrl+Shift+P"), Message::PaletteToggled),
                MenuEntry::Separator,
                action(
                    "文件资源管理器",
                    None,
                    Message::SidebarTab(SidebarTab::Files),
                ),
                action("线程列表", None, Message::SidebarTab(SidebarTab::Threads)),
                action(
                    "源代码管理",
                    None,
                    Message::SidebarTab(SidebarTab::SourceControl),
                ),
                MenuEntry::Separator,
                action(mode_label, None, Message::ModeToggled),
            ]
        }
        MenuId::Go => vec![
            action("转到线程…", None, Message::SidebarTab(SidebarTab::Threads)),
            action("转到文件…", None, Message::SidebarTab(SidebarTab::Files)),
            MenuEntry::Separator,
            action("转到搜索…", None, Message::SidebarTab(SidebarTab::Search)),
            action(
                "转到源代码管理",
                None,
                Message::SidebarTab(SidebarTab::SourceControl),
            ),
        ],
        MenuId::Run => vec![
            action("新建会话", None, Message::NewThreadRequested),
            action("重新连接后端", None, Message::Reconnect),
            MenuEntry::Separator,
            action("刷新 Git 状态", None, Message::GitRefreshRequested),
        ],
        MenuId::Terminal => vec![
            MenuEntry::Disabled {
                label: "新建终端",
                shortcut: Some("Ctrl+Shift+`"),
            },
            MenuEntry::Disabled {
                label: "运行任务…",
                shortcut: None,
            },
            MenuEntry::Separator,
            action("打开命令面板", None, Message::PaletteToggled),
        ],
        MenuId::Help => vec![
            action("命令面板", Some("Ctrl+Shift+P"), Message::PaletteToggled),
            MenuEntry::Disabled {
                label: "键盘快捷键",
                shortcut: None,
            },
            MenuEntry::Separator,
            MenuEntry::Disabled {
                label: "关于 Codex GUI",
                shortcut: None,
            },
        ],
    }
}

/// An enabled row shorthand.
fn action(label: &str, shortcut: Option<&'static str>, message: Message) -> MenuEntry {
    MenuEntry::Action {
        label: String::from(label),
        shortcut,
        action: message,
    }
}

/// The copy-last-reply row, disabled while no agent message exists.
fn copy_last_reply(state: &State, label: &'static str) -> MenuEntry {
    let last = state.transcript.entries().iter().rev().find_map(|entry| {
        if let Entry::AgentMessage { id, text } = entry {
            Some((id.clone(), text.clone()))
        } else {
            None
        }
    });
    match last {
        Some((id, text)) => MenuEntry::Action {
            label: String::from(label),
            shortcut: None,
            action: Message::CopyMessage { id, text },
        },
        None => MenuEntry::Disabled {
            label,
            shortcut: None,
        },
    }
}

/// The strip surface: top bar tone with a hairline base.
fn bar_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::TOPBAR.into()),
        border: iced::Border {
            color: theme::BORDER,
            width: 1.0,
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The small brand mark at the strip's left edge.
fn logo_mark(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::TEXT.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

/// The dropdown card: white fill, hairline border, soft radius.
fn panel_surface(_theme: &iced::Theme) -> iced::widget::container::Style {
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

/// A one-pixel rule between dropdown groups.
fn separator_line(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::BORDER.into()),
        ..iced::widget::container::Style::default()
    }
}
