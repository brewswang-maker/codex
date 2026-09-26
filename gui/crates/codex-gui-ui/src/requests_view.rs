//! The request dialogs for agent questions and MCP elicitations.
//!
//! Same modal pattern as the approvals overlay: the mask is inert, and
//! every request is answered explicitly (submit, accept, decline, or
//! cancel). Agent questions take precedence over elicitations because
//! they gate the running turn.

use crate::approvals_view;
use crate::message::Message;
use crate::state::State;
use crate::theme;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequest;
use codex_app_server_protocol::ToolRequestUserInputQuestion;
use codex_gui_core::FieldKind;
use codex_gui_core::FormField;
use codex_gui_core::PendingElicitation;
use codex_gui_core::PendingQuestion;
use iced::Element;
use iced::Fill;
use iced::alignment;
use iced::font;
use iced::widget::button;
use iced::widget::center;
use iced::widget::column;
use iced::widget::container;
use iced::widget::mouse_area;
use iced::widget::opaque;
use iced::widget::row;
use iced::widget::stack;
use iced::widget::text;
use iced::widget::text_input;

/// Wraps the main UI with the request dialogs while any is pending.
///
/// The queues answer oldest first; questions win over elicitations so the
/// turn the agent is blocked on gets unblocked before any MCP detour.
pub fn layered<'a>(
    state: &'a State,
    base: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let dialog: Option<Element<'a, Message>> = state
        .questions
        .pending()
        .first()
        .map(|pending| question_dialog(state, pending))
        .or_else(|| {
            state
                .elicitations
                .pending()
                .first()
                .map(|pending| elicitation_dialog(state, pending))
        });

    match dialog {
        Some(dialog) => stack![
            base.into(),
            opaque(mouse_area(
                center(opaque(dialog)).style(approvals_view::mask)
            ))
        ]
        .into(),
        None => base.into(),
    }
}

/// The agent-question form: every question of the request, then one
/// submit that answers them all at once.
fn question_dialog<'a>(state: &'a State, pending: &'a PendingQuestion) -> Element<'a, Message> {
    let mut body = column![
        text("The agent has questions")
            .size(theme::SIZE_BODY)
            .style(theme::fg)
    ]
    .spacing(10)
    .padding(16);

    for question in &pending.params.questions {
        body = body.push(question_block(state, question));
    }

    body = body.push(
        row![
            container(
                text("Unanswered questions resolve on the server side.")
                    .size(theme::SIZE_XS)
                    .style(approvals_view::muted),
            )
            .width(Fill),
            button(
                container(
                    text("Submit")
                        .size(theme::SIZE_SM)
                        .style(approvals_view::allow_label)
                )
                .align_x(alignment::Horizontal::Center)
                .width(Fill),
            )
            .padding([6, 16])
            .style(theme::primary_button)
            .on_press(Message::QuestionAnswered),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    );

    container(body)
        .width(Fill)
        .max_width(520)
        .style(approvals_view::dialog_surface)
        .into()
}

/// One question: its header and prompt, the option rows, and a free-form
/// notes field (secret questions mask their notes).
fn question_block<'a>(
    state: &'a State,
    question: &'a ToolRequestUserInputQuestion,
) -> Element<'a, Message> {
    let draft = state.question_drafts.get(&question.id);
    let selected = draft.and_then(|draft| draft.selected);
    let notes = draft.map_or("", |draft| draft.notes.as_str());

    let mut block = column![
        text(question.header.clone())
            .size(theme::SIZE_SM)
            .style(theme::fg)
            .font(bold()),
        text(question.question.clone())
            .size(theme::SIZE_SM)
            .style(approvals_view::muted),
    ]
    .spacing(6);

    for index in 0..codex_gui_core::question_option_rows(question) {
        let Some(label) = codex_gui_core::question_option_label(question, index) else {
            continue;
        };
        block = block.push(option_row(
            &question.id,
            index,
            label,
            selected == Some(index),
        ));
    }

    block = block.push(
        text_input("Add a note (optional)", notes)
            .size(theme::SIZE_SM)
            .padding([6, 8])
            .secure(question.is_secret)
            .style(theme::bare_input)
            .width(Fill)
            .on_input({
                let question_id = question.id.clone();
                move |notes| Message::QuestionNotesChanged {
                    question_id: question_id.clone(),
                    notes,
                }
            }),
    );

    container(block)
        .width(Fill)
        .padding(10)
        .style(question_card)
        .into()
}

/// One selectable option row; a filled marker once picked.
fn option_row<'a>(
    question_id: &str,
    index: usize,
    label: String,
    picked: bool,
) -> Element<'a, Message> {
    let marker_style: fn(&iced::Theme) -> iced::widget::text::Style = if picked {
        theme::fg
    } else {
        approvals_view::muted
    };
    button(
        row![
            text(if picked { "●" } else { "○" })
                .size(theme::SIZE_XS)
                .style(marker_style),
            text(label).size(theme::SIZE_SM).style(theme::fg),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([6, 10])
    .style(theme::rail_button(picked))
    .on_press(Message::QuestionOptionPicked {
        question_id: question_id.to_string(),
        index,
    })
    .into()
}

/// The elicitation dialog, dispatched on the request mode. The raw OpenAI
/// form modes never enter the queue; they only appear if that changes, so
/// they get a minimal decline surface instead of panicking.
fn elicitation_dialog<'a>(
    state: &'a State,
    pending: &'a PendingElicitation,
) -> Element<'a, Message> {
    let mut body = column![
        text(format!("Request from {}", pending.server_name()))
            .size(theme::SIZE_BODY)
            .style(theme::fg)
    ]
    .spacing(10)
    .padding(16);

    match &pending.params.request {
        McpServerElicitationRequest::Form {
            message,
            requested_schema,
            ..
        } => {
            body = body.push(approvals_view::detail(message));
            for field in codex_gui_core::elicitation_form_fields(requested_schema) {
                body = body.push(field_block(state, field));
            }
            if let Some(error) = &state.elicitation_error {
                body = body.push(
                    text(error.clone())
                        .size(theme::SIZE_XS)
                        .style(approvals_view::deny_label),
                );
            }
            body = body.push(
                row![
                    container(text(" ")).width(Fill),
                    quiet_action("Decline", McpServerElicitationAction::Decline),
                    quiet_action("Cancel", McpServerElicitationAction::Cancel),
                    primary_action("Accept", McpServerElicitationAction::Accept),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }
        McpServerElicitationRequest::Url { message, url, .. } => {
            body = body.push(approvals_view::detail(message));
            body = body.push(approvals_view::detail(url));
            body = body.push(
                row![
                    container(
                        text("Open the link in a browser, then confirm here.")
                            .size(theme::SIZE_XS)
                            .style(approvals_view::muted),
                    )
                    .width(Fill),
                    copy_button(url.clone()),
                    quiet_action("Decline", McpServerElicitationAction::Decline),
                    primary_action("I finished", McpServerElicitationAction::Accept),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }
        McpServerElicitationRequest::UserVerification {
            title,
            description,
            challenge,
            ..
        } => {
            body = body.push(
                text(title.clone())
                    .size(theme::SIZE_SM)
                    .style(theme::fg)
                    .font(bold()),
            );
            body = body.push(approvals_view::detail(description));
            body = body.push(approvals_view::detail(challenge));
            body = body.push(
                row![
                    container(
                        text("Device verification needs a proof this client cannot produce; decline to keep the server running.")
                            .size(theme::SIZE_XS)
                            .style(approvals_view::muted),
                    )
                    .width(Fill),
                    copy_button(challenge.clone()),
                    quiet_action("Decline", McpServerElicitationAction::Decline),
                    quiet_action("Cancel", McpServerElicitationAction::Cancel),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }
        McpServerElicitationRequest::OpenAiForm { message, .. }
        | McpServerElicitationRequest::OpenAiElicitationForm { message, .. } => {
            body = body.push(approvals_view::detail(message));
            body = body.push(
                row![
                    container(
                        text("This request form is not supported by this client.")
                            .size(theme::SIZE_XS)
                            .style(approvals_view::muted),
                    )
                    .width(Fill),
                    quiet_action("Decline", McpServerElicitationAction::Decline),
                ]
                .spacing(8)
                .align_y(alignment::Vertical::Center),
            );
        }
    }

    container(body)
        .width(Fill)
        .max_width(520)
        .style(approvals_view::dialog_surface)
        .into()
}

/// One schema field of a form elicitation: label, optional description,
/// and the editor matching the field kind.
fn field_block<'a>(state: &'a State, field: FormField) -> Element<'a, Message> {
    let draft = state.elicitation_drafts.get(&field.key);
    let label = if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    };

    let mut block = column![text(label).size(theme::SIZE_SM).style(theme::fg)].spacing(4);
    if let Some(description) = field.description {
        block = block.push(
            text(description)
                .size(theme::SIZE_XS)
                .style(approvals_view::muted),
        );
    }

    block = match field.kind {
        FieldKind::Text => block.push(text_editor(
            state,
            &field.key,
            "Text value",
            /*secure*/ false,
        )),
        FieldKind::Number { .. } => {
            block.push(text_editor(state, &field.key, "0", /*secure*/ false))
        }
        FieldKind::Boolean => {
            let enabled = draft.is_some_and(|draft| draft.boolean);
            block.push(marker_row(
                if enabled { "✓" } else { "○" },
                if enabled {
                    "Enabled".to_string()
                } else {
                    "Disabled".to_string()
                },
                enabled,
                Message::ElicitationFieldToggled {
                    key: field.key.clone(),
                },
            ))
        }
        FieldKind::SingleSelect { options } => {
            let picked = draft.and_then(|draft| draft.single.clone());
            for (value, option_label) in options {
                let active = picked.as_deref() == Some(value.as_str());
                block = block.push(marker_row(
                    if active { "●" } else { "○" },
                    option_label,
                    active,
                    Message::ElicitationFieldPicked {
                        key: field.key.clone(),
                        value,
                    },
                ));
            }
            block
        }
        FieldKind::MultiSelect { options } => {
            for (value, option_label) in options {
                let active = draft
                    .is_some_and(|draft| draft.multi.iter().any(|selected| selected == &value));
                block = block.push(marker_row(
                    if active { "■" } else { "□" },
                    option_label,
                    active,
                    Message::ElicitationFieldValueToggled {
                        key: field.key.clone(),
                        value,
                    },
                ));
            }
            block
        }
    };

    block.into()
}

/// A framing text editor for one elicitation string or number draft.
fn text_editor<'a>(
    state: &'a State,
    key: &str,
    placeholder: &'static str,
    secure: bool,
) -> Element<'a, Message> {
    let value = state
        .elicitation_drafts
        .get(key)
        .map_or("", |draft| draft.text.as_str());
    text_input(placeholder, value)
        .size(theme::SIZE_SM)
        .padding([6, 8])
        .secure(secure)
        .style(theme::bare_input)
        .width(Fill)
        .on_input({
            let key = key.to_string();
            move |text| Message::ElicitationFieldTextChanged {
                key: key.clone(),
                text,
            }
        })
        .into()
}

/// A full-width selectable row: marker, label, and its press message.
fn marker_row<'a>(
    marker: &'static str,
    label: String,
    active: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let marker_style: fn(&iced::Theme) -> iced::widget::text::Style = if active {
        theme::fg
    } else {
        approvals_view::muted
    };
    button(
        row![
            text(marker).size(theme::SIZE_XS).style(marker_style),
            text(label).size(theme::SIZE_SM).style(theme::fg),
        ]
        .spacing(8)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .padding([6, 10])
    .style(theme::rail_button(active))
    .on_press(on_press)
    .into()
}

/// A quiet dialog button carrying a decline/cancel decision.
fn quiet_action<'a>(
    label: &'static str,
    action: McpServerElicitationAction,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(theme::SIZE_SM)
            .style(approvals_view::muted),
    )
    .padding([6, 12])
    .style(approvals_view::quiet_button)
    .on_press(Message::ElicitationAnswered(action))
    .into()
}

/// The filled dialog button carrying the affirmative decision.
fn primary_action<'a>(
    label: &'static str,
    action: McpServerElicitationAction,
) -> Element<'a, Message> {
    button(
        container(
            text(label)
                .size(theme::SIZE_SM)
                .style(approvals_view::allow_label),
        )
        .align_x(alignment::Horizontal::Center)
        .width(Fill),
    )
    .padding([6, 16])
    .style(theme::primary_button)
    .on_press(Message::ElicitationAnswered(action))
    .into()
}

/// The quiet chip that parks one URL on the clipboard.
fn copy_button<'a>(url: String) -> Element<'a, Message> {
    button(
        text("Copy link")
            .size(theme::SIZE_SM)
            .style(approvals_view::muted),
    )
    .padding([6, 12])
    .style(approvals_view::quiet_button)
    .on_press(Message::ElicitationLinkCopied(url))
    .into()
}

/// The per-question well inside the dialog card.
fn question_card(_theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme::CARD.into()),
        border: iced::Border {
            radius: theme::RADIUS_SM.into(),
            ..iced::Border::default()
        },
        ..iced::widget::container::Style::default()
    }
}

fn bold() -> font::Font {
    font::Font {
        weight: font::Weight::Bold,
        ..font::Font::default()
    }
}
