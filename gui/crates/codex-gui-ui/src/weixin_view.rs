//! The WeChat bridge pane: in-GUI QR binding, daemon liveness, login
//! state, and the pending-authorization queue with one-click grants.
//!
//! The pane is a pure projection of [`codex_gui_core::WeixinPanelData`];
//! loading and granting go through `commands::load_weixin_panel` /
//! `commands::allow_weixin_contact`, which coordinate with the bridge
//! daemon through the runtime files only. Binding lives in
//! `weixin_binding` and swaps the pane's content while it runs.

use crate::message::Message;
use crate::state::State;
use crate::theme;
use crate::weixin_binding::BindingPhase;
use crate::weixin_binding::QrCanvas;
use crate::weixin_binding::WeixinBinding;
use codex_gui_core::WeixinPanelData;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::scrollable;
use iced::widget::text;
use iced::widget::text_input;

/// Renders the WeChat bridge pane for the sidebar.
pub fn panel(state: &State) -> Element<'_, Message> {
    if let Some(binding) = state.weixin_binding.as_ref() {
        return binding_pane(binding);
    }
    let Some(data) = state.weixin.as_ref() else {
        return placeholder("桥状态尚未加载。");
    };
    scrollable(
        column![
            section_header("桥状态"),
            liveness_section(data),
            section_header("待授权联系人"),
            pending_section(data),
            section_header("已放行"),
            allowed_section(data),
        ]
        .width(Fill)
        .spacing(14)
        .padding([4, 2]),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

/// The in-GUI binding flow: QR code, live status, verify-code input.
fn binding_pane(binding: &WeixinBinding) -> Element<'_, Message> {
    let mut body = column![section_header("绑定微信")]
        .spacing(12)
        .width(Fill)
        .padding([4, 2]);

    match &binding.phase {
        BindingPhase::Starting => {
            body = body.push(
                text("正在获取登录二维码…")
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            );
        }
        BindingPhase::QrReady { matrix, note } => {
            body = body.push(
                container(QrCanvas::widget(matrix.clone()))
                    .style(theme::surface(theme::BG))
                    .padding(10),
            );
            body = body.push(
                text("请用手机微信扫描二维码，并在手机上确认登录。")
                    .size(theme::SIZE_XS)
                    .style(theme::dim),
            );
            if !note.is_empty() {
                body = body.push(
                    text(note.clone())
                        .size(theme::SIZE_XS)
                        .style(theme::faint_text),
                );
            }
        }
        BindingPhase::Scanned => {
            body = body.push(
                row![
                    status_dot(theme::BRAND),
                    text("已扫码，请在手机上确认。")
                        .size(theme::SIZE_XS)
                        .style(theme::fg),
                ]
                .spacing(8),
            );
        }
        BindingPhase::NeedVerifyCode => {
            body = body.push(
                text("请输入手机微信上显示的数字：")
                    .size(theme::SIZE_XS)
                    .style(theme::fg),
            );
            body = body.push(
                row![
                    text_input("数字验证码", &binding.verify_input)
                        .on_input(Message::WeixinVerifyCodeChanged)
                        .on_submit(Message::WeixinVerifyCodeSubmitted)
                        .width(140)
                        .size(theme::SIZE_XS)
                        .padding([4, 6]),
                    button(text("提交").size(theme::SIZE_XS).style(theme::fg))
                        .padding([4, 10])
                        .style(theme::ghost_button)
                        .on_press(Message::WeixinVerifyCodeSubmitted),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }
        BindingPhase::Connected { notice } => {
            body = body.push(
                row![
                    status_dot(theme::BRAND),
                    text(notice.clone()).size(theme::SIZE_XS).style(theme::fg),
                ]
                .spacing(8),
            );
        }
        BindingPhase::Failed { message } => {
            body = body.push(
                row![
                    status_dot(theme::DANGER),
                    text(message.clone()).size(theme::SIZE_XS).style(theme::fg),
                ]
                .spacing(8),
            );
        }
    }

    let (label, action) = match binding.phase {
        BindingPhase::Connected { .. } | BindingPhase::Failed { .. } => {
            ("完成", Message::WeixinBindingDismiss)
        }
        _ => ("取消绑定", Message::WeixinBindingCancel),
    };
    body = body.push(
        button(text(label).size(theme::SIZE_XS).style(theme::fg))
            .padding([4, 10])
            .style(theme::ghost_button)
            .on_press(action),
    );

    scrollable(body).width(Fill).height(Fill).into()
}

/// The liveness card: daemon + login state, with an honest hint per state.
fn liveness_section(data: &WeixinPanelData) -> Element<'static, Message> {
    let (dot, label) = if data.running {
        (theme::BRAND, "桥接进程运行中")
    } else {
        (theme::FAINT, "桥接进程未运行")
    };
    let mut lines = column![
        row![
            status_dot(dot),
            text(label).size(theme::SIZE_MD).style(theme::fg),
        ]
        .spacing(8)
        .padding([2, 2]),
    ]
    .spacing(4);

    if data.logged_in {
        lines = lines.push(
            row![
                text("已登录").size(theme::SIZE_XS).style(theme::dim),
                text(data.bot_id.clone().unwrap_or_default())
                    .size(theme::SIZE_XS)
                    .style(theme::faint_text),
            ]
            .spacing(8)
            .padding([0, 2]),
        );
    } else {
        lines = lines.push(
            button(text("绑定微信").size(theme::SIZE_XS).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::WeixinBindingStart),
        );
    }
    if !data.running && data.logged_in {
        lines = lines.push(
            button(text("启动桥接").size(theme::SIZE_XS).style(theme::fg))
                .padding([4, 10])
                .style(theme::ghost_button)
                .on_press(Message::WeixinDaemonStart),
        );
    }
    lines.into()
}

/// The pending queue: each contact carries a one-click grant button.
fn pending_section(data: &WeixinPanelData) -> Element<'_, Message> {
    let mut entries = column![].spacing(2);
    if data.pending.is_empty() {
        entries = entries.push(
            text("暂无待授权联系人；未放行的联系人发消息会出现在这里。")
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
    }
    for pending in &data.pending {
        entries = entries.push(
            row![
                text(pending.contact.clone())
                    .size(theme::SIZE_XS)
                    .style(theme::fg)
                    .wrapping(text::Wrapping::None),
                text(relative_ms(pending.last_seen_ms))
                    .size(theme::SIZE_XS)
                    .style(theme::faint_text),
                button(text("放行").size(theme::SIZE_XS).style(theme::fg))
                    .padding([2, 8])
                    .style(theme::ghost_button)
                    .on_press(Message::WeixinAllowContact(pending.contact.clone())),
            ]
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([3, 2]),
        );
    }
    entries.into()
}

/// The allowlist: read-only names for now (revocation stays a file edit).
fn allowed_section(data: &WeixinPanelData) -> Element<'static, Message> {
    let mut entries = column![].spacing(2);
    if data.allowed.is_empty() {
        entries = entries.push(
            text("白名单为空。".to_string())
                .size(theme::SIZE_XS)
                .style(theme::dim),
        );
    }
    for contact in &data.allowed {
        entries = entries.push(
            row![
                text("微").size(theme::SIZE_XS).style(brand_text),
                text(contact.clone()).size(theme::SIZE_XS).style(theme::fg),
            ]
            .spacing(8)
            .padding([3, 2]),
        );
    }
    entries.into()
}

/// Shared placeholder for the not-yet-loaded state.
fn placeholder(label: &'static str) -> Element<'static, Message> {
    container(
        text(label)
            .size(theme::SIZE_XS)
            .style(theme::dim)
            .width(Fill),
    )
    .width(Fill)
    .padding([8, 4])
    .into()
}

/// The pane group label, mirroring the remote pane's small-caps header.
fn section_header(label: &'static str) -> Element<'static, Message> {
    container(
        text(label.to_uppercase())
            .size(theme::SIZE_XS)
            .style(theme::faint_text),
    )
    .width(Fill)
    .padding([0, 2])
    .align_x(alignment::Horizontal::Left)
    .into()
}

fn status_dot(color: iced::Color) -> Element<'static, Message> {
    container(
        text("●")
            .size(theme::SIZE_XS)
            .style(move |_theme| iced::widget::text::Style { color: Some(color) }),
    )
    .into()
}

/// Text painted in the brand green (the allowlist's 微 marker).
fn brand_text(_theme: &iced::Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(theme::BRAND),
    }
}

/// A coarse relative age for the pending list's last-seen stamp.
fn relative_ms(ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_millis() as i64);
    let minutes = (now - ms).max(0) / 60_000;
    match minutes {
        0 => "刚刚".to_string(),
        n if n < 60 => format!("{n} 分钟前"),
        n => format!("{} 小时前", n / 60),
    }
}
