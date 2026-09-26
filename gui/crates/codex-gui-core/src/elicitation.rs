//! Pending MCP elicitation requests and their form answers.
//!
//! MCP servers escalate through `mcpServer/elicitation/request` when they
//! need structured input. The typed `Form` mode carries a small JSON-schema
//! object; this module projects that schema into renderable fields, seeds
//! drafts from the schema defaults, validates them on accept, and builds
//! the wire response. Modes the GUI cannot represent are declined.

use codex_app_server_protocol::McpElicitationConstOption;
use codex_app_server_protocol::McpElicitationEnumSchema;
use codex_app_server_protocol::McpElicitationMultiSelectEnumSchema;
use codex_app_server_protocol::McpElicitationNumberType;
use codex_app_server_protocol::McpElicitationPrimitiveSchema;
use codex_app_server_protocol::McpElicitationSchema;
use codex_app_server_protocol::McpElicitationSingleSelectEnumSchema;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequest;
use codex_app_server_protocol::McpServerElicitationRequestParams;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequest;
use serde_json::Map;
use serde_json::Value;
use std::collections::HashMap;

/// One outstanding elicitation awaiting a decision.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingElicitation {
    /// Wire id used both to answer the request and to match the
    /// `serverRequest/resolved` notification.
    pub request_id: RequestId,
    /// The server, the mode, and the requested schema or URL.
    pub params: McpServerElicitationRequestParams,
}

impl PendingElicitation {
    /// The MCP server that raised the request.
    pub fn server_name(&self) -> &str {
        &self.params.server_name
    }
}

/// Queue of pending elicitations, answered in arrival order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Elicitations {
    pending: Vec<PendingElicitation>,
}

impl Elicitations {
    /// Outstanding elicitations, oldest first; the dialog shows the head.
    pub fn pending(&self) -> &[PendingElicitation] {
        &self.pending
    }

    /// Normalizes a server request into the queue. The raw OpenAI form
    /// modes carry untyped schemas this client cannot render, so they are
    /// left to [`auto_decline_elicitation`]; everything else returns
    /// `None`.
    pub fn offered(&mut self, request: &ServerRequest) -> Option<PendingElicitation> {
        let ServerRequest::McpServerElicitationRequest { request_id, params } = request else {
            return None;
        };
        match &params.request {
            McpServerElicitationRequest::Form { .. }
            | McpServerElicitationRequest::Url { .. }
            | McpServerElicitationRequest::UserVerification { .. } => {}
            McpServerElicitationRequest::OpenAiForm { .. }
            | McpServerElicitationRequest::OpenAiElicitationForm { .. } => return None,
        }
        let pending = PendingElicitation {
            request_id: request_id.clone(),
            params: params.clone(),
        };
        self.pending.push(pending.clone());
        Some(pending)
    }

    /// Drops the entry matched by a `serverRequest/resolved` notification;
    /// the server answered the request itself (e.g. the turn was cancelled).
    pub fn resolved(&mut self, request_id: &RequestId) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|elicitation| elicitation.request_id != *request_id);
        self.pending.len() != before
    }

    /// Takes the entry out of the queue so its answer can be sent.
    pub fn take(&mut self, request_id: &str) -> Option<PendingElicitation> {
        let index = self
            .pending
            .iter()
            .position(|elicitation| elicitation.request_id.to_string() == request_id)?;
        Some(self.pending.remove(index))
    }
}

/// The auto-decline reply for elicitation modes without a GUI dialog
/// (the raw OpenAI form modes); `None` for every other server request.
pub fn auto_decline_elicitation(request: &ServerRequest) -> Option<(RequestId, Value)> {
    let ServerRequest::McpServerElicitationRequest { request_id, params } = request else {
        return None;
    };
    match &params.request {
        McpServerElicitationRequest::OpenAiForm { .. }
        | McpServerElicitationRequest::OpenAiElicitationForm { .. } => Some((
            request_id.clone(),
            response_payload(McpServerElicitationAction::Decline, /*content*/ None),
        )),
        McpServerElicitationRequest::Form { .. }
        | McpServerElicitationRequest::Url { .. }
        | McpServerElicitationRequest::UserVerification { .. } => None,
    }
}

/// One editable field of an elicitation form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    /// Property key on the wire.
    pub key: String,
    /// Display label: the schema title, falling back to the key.
    pub label: String,
    /// Optional one-line hint.
    pub description: Option<String>,
    /// Whether the schema lists the key as required.
    pub required: bool,
    /// How the field renders and what it accepts.
    pub kind: FieldKind,
}

/// How one form field renders; mirrors the schema primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Free-form text.
    Text,
    /// Numeric input; `integer` rejects values with a fraction.
    Number {
        /// Whether the schema declares `integer` instead of `number`.
        integer: bool,
    },
    /// Checkbox.
    Boolean,
    /// Exactly one of the `(value, label)` options.
    SingleSelect {
        /// The choices in schema order.
        options: Vec<(String, String)>,
    },
    /// Any of the `(value, label)` options.
    MultiSelect {
        /// The choices in schema order.
        options: Vec<(String, String)>,
    },
}

/// The GUI's draft for one elicitation form field. All four slots exist on
/// every draft; the field's [`FieldKind`] decides which one is rendered
/// and read back into the payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElicitationDraft {
    /// Text/number input value.
    pub text: String,
    /// Checkbox value.
    pub boolean: bool,
    /// Selected value of a single-select field.
    pub single: Option<String>,
    /// Selected values of a multi-select field, in click order.
    pub multi: Vec<String>,
}

/// Projects the requested schema into renderable fields, in schema order.
pub fn form_fields(schema: &McpElicitationSchema) -> Vec<FormField> {
    let required = schema.required.as_deref().unwrap_or_default();
    schema
        .properties
        .iter()
        .map(|(key, primitive)| {
            let (title, description, kind) = describe(primitive);
            FormField {
                key: key.clone(),
                label: title.unwrap_or_else(|| key.clone()),
                description,
                required: required.iter().any(|name| name == key),
                kind,
            }
        })
        .collect()
}

/// Splits a schema primitive into its display parts and render kind.
fn describe(
    primitive: &McpElicitationPrimitiveSchema,
) -> (Option<String>, Option<String>, FieldKind) {
    match primitive {
        McpElicitationPrimitiveSchema::String(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Text,
        ),
        McpElicitationPrimitiveSchema::Number(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Number {
                integer: schema.type_ == McpElicitationNumberType::Integer,
            },
        ),
        McpElicitationPrimitiveSchema::Boolean(schema) => (
            schema.title.clone(),
            schema.description.clone(),
            FieldKind::Boolean,
        ),
        McpElicitationPrimitiveSchema::Enum(schema) => match schema {
            McpElicitationEnumSchema::SingleSelect(single) => match single {
                McpElicitationSingleSelectEnumSchema::Untitled(single) => (
                    single.title.clone(),
                    single.description.clone(),
                    FieldKind::SingleSelect {
                        options: untitled_options(&single.enum_),
                    },
                ),
                McpElicitationSingleSelectEnumSchema::Titled(single) => (
                    single.title.clone(),
                    single.description.clone(),
                    FieldKind::SingleSelect {
                        options: titled_options(&single.one_of),
                    },
                ),
            },
            McpElicitationEnumSchema::MultiSelect(multi) => match multi {
                McpElicitationMultiSelectEnumSchema::Untitled(multi) => (
                    multi.title.clone(),
                    multi.description.clone(),
                    FieldKind::MultiSelect {
                        options: untitled_options(&multi.items.enum_),
                    },
                ),
                McpElicitationMultiSelectEnumSchema::Titled(multi) => (
                    multi.title.clone(),
                    multi.description.clone(),
                    FieldKind::MultiSelect {
                        options: titled_options(&multi.items.any_of),
                    },
                ),
            },
            // The legacy shape predates titled oneOf/anyOf; `enumNames`
            // supplies display labels positionally when present.
            McpElicitationEnumSchema::Legacy(legacy) => {
                let options = legacy
                    .enum_
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let label = legacy
                            .enum_names
                            .as_ref()
                            .and_then(|names| names.get(index))
                            .cloned()
                            .unwrap_or_else(|| value.clone());
                        (value.clone(), label)
                    })
                    .collect();
                (
                    legacy.title.clone(),
                    legacy.description.clone(),
                    FieldKind::SingleSelect { options },
                )
            }
        },
    }
}

/// Untitled enums use each value as its own label.
fn untitled_options(values: &[String]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|value| (value.clone(), value.clone()))
        .collect()
}

/// Titled enums pair a wire value with a display label.
fn titled_options(options: &[McpElicitationConstOption]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|option| (option.const_.clone(), option.title.clone()))
        .collect()
}

/// Fresh drafts for the dialog on top: schema defaults for a form, empty
/// otherwise (URL and verification dialogs have no fields).
pub fn default_drafts_for(pending: &PendingElicitation) -> HashMap<String, ElicitationDraft> {
    match &pending.params.request {
        McpServerElicitationRequest::Form {
            requested_schema, ..
        } => requested_schema
            .properties
            .iter()
            .map(|(key, primitive)| (key.clone(), default_draft(primitive)))
            .collect(),
        McpServerElicitationRequest::UserVerification { .. }
        | McpServerElicitationRequest::OpenAiForm { .. }
        | McpServerElicitationRequest::OpenAiElicitationForm { .. }
        | McpServerElicitationRequest::Url { .. } => HashMap::new(),
    }
}

/// The draft value seeded from one property's schema `default`.
fn default_draft(primitive: &McpElicitationPrimitiveSchema) -> ElicitationDraft {
    let mut draft = ElicitationDraft::default();
    match primitive {
        McpElicitationPrimitiveSchema::String(schema) => {
            draft.text = schema.default.clone().unwrap_or_default();
        }
        McpElicitationPrimitiveSchema::Number(schema) => {
            if let Some(value) = schema.default {
                draft.text = format_number(value);
            }
        }
        McpElicitationPrimitiveSchema::Boolean(schema) => {
            draft.boolean = schema.default.unwrap_or(false);
        }
        McpElicitationPrimitiveSchema::Enum(schema) => match schema {
            McpElicitationEnumSchema::SingleSelect(single) => match single {
                McpElicitationSingleSelectEnumSchema::Untitled(single) => {
                    draft.single = single.default.clone();
                }
                McpElicitationSingleSelectEnumSchema::Titled(single) => {
                    draft.single = single.default.clone();
                }
            },
            McpElicitationEnumSchema::MultiSelect(multi) => match multi {
                McpElicitationMultiSelectEnumSchema::Untitled(multi) => {
                    draft.multi = multi.default.clone().unwrap_or_default();
                }
                McpElicitationMultiSelectEnumSchema::Titled(multi) => {
                    draft.multi = multi.default.clone().unwrap_or_default();
                }
            },
            McpElicitationEnumSchema::Legacy(legacy) => {
                draft.single = legacy.default.clone();
            }
        },
    }
    draft
}

/// Renders an `f64` default without a trailing `.0` for whole numbers.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

/// Validates the drafted values against the schema and serializes them
/// into the accept `content` object. Required fields must be filled in;
/// unparseable numbers fail with a user-facing message.
pub fn form_content(
    schema: &McpElicitationSchema,
    drafts: &HashMap<String, ElicitationDraft>,
) -> Result<Value, String> {
    let empty = ElicitationDraft::default();
    let mut content = Map::new();
    for field in form_fields(schema) {
        let draft = drafts.get(&field.key).unwrap_or(&empty);
        if let Some(value) = field_value(&field, draft)? {
            content.insert(field.key.clone(), value);
        }
    }
    Ok(Value::Object(content))
}

/// The wire value for one field: `None` omits it (empty optional), `Err`
/// blocks submission with the shown message.
fn field_value(field: &FormField, draft: &ElicitationDraft) -> Result<Option<Value>, String> {
    match &field.kind {
        FieldKind::Text => {
            let text = draft.text.trim();
            if text.is_empty() {
                if field.required {
                    Err(required_message(field))
                } else {
                    Ok(None)
                }
            } else {
                Ok(Some(Value::String(text.to_string())))
            }
        }
        FieldKind::Number { integer } => {
            let text = draft.text.trim();
            if text.is_empty() {
                return if field.required {
                    Err(required_message(field))
                } else {
                    Ok(None)
                };
            }
            if *integer {
                text.parse::<i64>()
                    .map(|value| Some(Value::from(value)))
                    .map_err(|_| format!("\"{}\" must be a whole number", field.label))
            } else {
                text.parse::<f64>()
                    .map(|value| Some(Value::from(value)))
                    .map_err(|_| format!("\"{}\" must be a number", field.label))
            }
        }
        FieldKind::Boolean => Ok(Some(Value::Bool(draft.boolean))),
        FieldKind::SingleSelect { .. } => match &draft.single {
            Some(selected) => Ok(Some(Value::String(selected.clone()))),
            None if field.required => Err(required_message(field)),
            None => Ok(None),
        },
        FieldKind::MultiSelect { options } => {
            if draft.multi.is_empty() {
                if field.required {
                    Err(required_message(field))
                } else {
                    Ok(None)
                }
            } else {
                // Emit in schema order so payloads are stable regardless
                // of the click order.
                let selected = options
                    .iter()
                    .filter(|(value, _)| draft.multi.contains(value))
                    .map(|(value, _)| Value::String(value.clone()))
                    .collect();
                Ok(Some(Value::Array(selected)))
            }
        }
    }
}

/// The message shown when a required field is missing.
fn required_message(field: &FormField) -> String {
    format!("\"{}\" is required", field.label)
}

/// The wire payload answering an elicitation; `content` carries the form
/// values on accept and is `None` on decline/cancel.
#[allow(clippy::expect_used)]
pub fn response_payload(action: McpServerElicitationAction, content: Option<Value>) -> Value {
    serde_json::to_value(McpServerElicitationRequestResponse {
        action,
        content,
        meta: None,
    })
    .expect("elicitation response serializes")
}
