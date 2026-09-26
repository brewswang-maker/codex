//! The settings panel state: reads the effective config through the config
//! RPC and drafts user edits for a `config/batchWrite`.
//!
//! Values are plain strings in the `config.toml` wire form (snake_case keys,
//! kebab-case enum values) exactly as `config/read` reports them; the panel
//! never re-derives typed enums, so unknown server values round-trip safely.

use codex_app_server_protocol::{Account, GetAccountResponse};
use serde_json::Value;

/// One editable settings field, identified by its `config.toml` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingField {
    Model,
    ApprovalPolicy,
    SandboxMode,
    ReasoningEffort,
    Verbosity,
    WebSearch,
    ProviderId,
    ProviderBaseUrl,
    ProviderModel,
    ProviderApiKey,
}

impl SettingField {
    /// The `config.toml` key mirrored by the config RPC value layer.
    /// Provider fields are aggregated into the `model_providers` table by
    /// [`Settings::edits`] instead of mapping to a single top-level key.
    pub fn key(self) -> &'static str {
        match self {
            SettingField::Model => "model",
            SettingField::ApprovalPolicy => "approval_policy",
            SettingField::SandboxMode => "sandbox_mode",
            SettingField::ReasoningEffort => "model_reasoning_effort",
            SettingField::Verbosity => "model_verbosity",
            SettingField::WebSearch => "web_search",
            SettingField::ProviderId => "model_providers.<id>",
            SettingField::ProviderBaseUrl => "model_providers.<id>.base_url",
            SettingField::ProviderModel => "model",
            SettingField::ProviderApiKey => "model_providers.<id>.experimental_bearer_token",
        }
    }
}

/// A third-party model provider entry for the `model_providers` config table.
/// Only fields the GUI manages are present; `wire_api` defaults to
/// `responses`, the only supported protocol in current Codex builds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderDraft {
    /// Config-table key, also the value of the top-level `model_provider`.
    pub id: String,
    /// The provider's OpenAI-Responses-compatible base URL.
    pub base_url: String,
    /// Suggested model id sent as the top-level `model`.
    pub model: String,
    /// Bearer token for the provider; empty means "keep the current one".
    pub api_key: String,
}

/// How a vendor account is billed. Vendors expose different endpoints per
/// plan type; only types whose official docs speak the OpenAI Responses
/// API (the only `wire_api` current Codex builds support) exist here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billing {
    /// 按量付费: standard platform key on the public endpoint.
    PayAsYouGo,
    /// Coding plan subscription with its dedicated key/endpoint.
    CodingPlan,
}

impl Billing {
    /// The picker label shown in the settings panel.
    pub fn label(self) -> &'static str {
        match self {
            Billing::PayAsYouGo => "按量付费",
            Billing::CodingPlan => "Coding plan",
        }
    }

    /// Parses back a picker label.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "按量付费" => Some(Billing::PayAsYouGo),
            "Coding plan" => Some(Billing::CodingPlan),
            _ => None,
        }
    }
}

/// One `(billing × endpoint)` entry of a vendor preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanEntry {
    /// Which kind of billing this endpoint belongs to.
    pub billing: Billing,
    /// The vendor's official OpenAI-Responses base URL.
    pub base_url: &'static str,
    /// Suggested model id sent as the top-level `model`.
    pub model: &'static str,
}

/// A vendor and its per-billing endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorPreset {
    /// Config-table key, also the value of the top-level `model_provider`.
    pub id: &'static str,
    /// Display name in the settings panel.
    pub name: &'static str,
    /// Endpoints per billing type; empty for custom/OpenAI.
    pub entries: &'static [PlanEntry],
}

/// Built-in provider presets. Base URLs are the vendors' official
/// OpenAI-Responses endpoints (Kimi: `platform.kimi.com` API overview and
/// the Kimi Code docs; GLM: `docs.bigmodel.cn` coding-plan Codex guide;
/// Qwen: Aliyun Model Studio Responses compatibility guide; Doubao:
/// Volcengine Ark Responses docs; DeepSeek: the `api-docs.deepseek.com`
/// Responses guide). Suggested models track each vendor's current catalog
/// (the GLM-5.3 tier per the `docs.bigmodel.cn` model overview; DeepSeek
/// `deepseek-flash`/`deepseek-v4-pro` per its Codex integration guide;
/// Doubao's recommended Ark snapshot). Plan types without an official
/// Responses endpoint — Qwen's Token and Coding plans are
/// Chat/Completions-only — are not listed.
pub const PROVIDER_PRESETS: &[VendorPreset] = &[
    VendorPreset {
        id: "",
        name: "custom",
        entries: &[],
    },
    VendorPreset {
        id: "openai",
        name: "OpenAI",
        entries: &[],
    },
    VendorPreset {
        id: "deepseek",
        name: "DeepSeek",
        entries: &[
            // Current ids per the official Codex integration guide; the
            // legacy `deepseek-v4-flash` name is offline and off the list.
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://api.deepseek.com",
                model: "deepseek-flash",
            },
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://api.deepseek.com",
                model: "deepseek-v4-pro",
            },
        ],
    },
    VendorPreset {
        id: "glm",
        name: "GLM (智谱)",
        entries: &[
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://open.bigmodel.cn/api/v1",
                model: "glm-5.3",
            },
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://open.bigmodel.cn/api/v1",
                model: "glm-5.3-flash",
            },
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://open.bigmodel.cn/api/v1",
                model: "glm-5.3-flashx",
            },
            // Official Codex design (docs.bigmodel.cn): both plans speak
            // Responses on this one endpoint; the plan only changes the key.
            PlanEntry {
                billing: Billing::CodingPlan,
                base_url: "https://open.bigmodel.cn/api/v1",
                model: "glm-5.3",
            },
            // The coding plan also carries GLM-5.3-Flash (triple quota);
            // FlashX stays pay-as-you-go.
            PlanEntry {
                billing: Billing::CodingPlan,
                base_url: "https://open.bigmodel.cn/api/v1",
                model: "glm-5.3-flash",
            },
        ],
    },
    VendorPreset {
        id: "kimi",
        name: "Kimi (月之暗面)",
        entries: &[
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://api.moonshot.cn/v1",
                model: "kimi-k3",
            },
            // The official coding pick of the K2.7 tier.
            PlanEntry {
                billing: Billing::PayAsYouGo,
                base_url: "https://api.moonshot.cn/v1",
                model: "kimi-k2.7-code-highspeed",
            },
            PlanEntry {
                billing: Billing::CodingPlan,
                base_url: "https://api.kimi.com/coding/v1",
                model: "kimi-for-coding",
            },
        ],
    },
    VendorPreset {
        id: "qwen",
        name: "Qwen (阿里云百炼)",
        entries: &[PlanEntry {
            billing: Billing::PayAsYouGo,
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            model: "qwen3.8-max",
        }],
    },
    VendorPreset {
        id: "doubao",
        name: "豆包 (火山方舟)",
        entries: &[PlanEntry {
            billing: Billing::PayAsYouGo,
            base_url: "https://ark.cn-beijing.volces.com/api/v3",
            model: "doubao-seed-2-1-pro-260915",
        }],
    },
];

/// Looks up one vendor preset by id.
pub fn provider_preset(id: &str) -> Option<&'static VendorPreset> {
    PROVIDER_PRESETS.iter().find(|preset| preset.id == id)
}

/// The billing type matching a loaded `(vendor, base URL, model)` config,
/// or `None` when it does not resolve to a known preset entry.
fn billing_of(vendor_id: &str, base_url: &str, model: &str) -> Option<Billing> {
    provider_preset(vendor_id)?
        .entries
        .iter()
        .find(|entry| entry.base_url == base_url && entry.model == model)
        .map(|entry| entry.billing)
}

/// Pick-list candidates per field, in server wire form. An empty list means
/// free-form text (the model id comes from the catalog, not a fixed set).
pub fn options(field: SettingField) -> Vec<String> {
    let candidates: &[&str] = match field {
        SettingField::Model => &[],
        SettingField::ApprovalPolicy => &["untrusted", "on-request"],
        SettingField::SandboxMode => &["read-only", "workspace-write", "danger-full-access"],
        SettingField::ReasoningEffort => &["minimal", "low", "medium", "high", "xhigh"],
        SettingField::Verbosity => &["low", "medium", "high"],
        SettingField::WebSearch => &["disabled", "cached", "indexed", "live"],
        // Provider ids, base URLs and keys are free-form text.
        SettingField::ProviderId
        | SettingField::ProviderBaseUrl
        | SettingField::ProviderModel
        | SettingField::ProviderApiKey => &[],
    };
    candidates
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

/// String drafts of the editable values. An empty string means "unset":
/// the field is inherited from lower config layers and is never written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsDraft {
    pub model: String,
    pub approval_policy: String,
    pub sandbox_mode: String,
    pub reasoning_effort: String,
    pub verbosity: String,
    pub web_search: String,
}

/// The settings panel state machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Whether the panel is visible.
    pub open: bool,
    /// What the user is editing; reseeded on every config read.
    pub draft: SettingsDraft,
    /// The effective values as last reported by the server.
    pub current: SettingsDraft,
    /// The model-provider form and its server-side baseline.
    pub provider: ProviderDraft,
    /// Provider values as last reported by the server (key never stored).
    pub provider_current: ProviderDraft,
    /// The drafted billing type; derived from the loaded config and reset
    /// by every preset or billing pick.
    pub billing: Option<Billing>,
    /// Draft of the account API key; consumed and cleared on submit.
    pub api_key_draft: String,
    /// How the signed-in account shows in the panel; `None` means signed out.
    pub signed_in_label: Option<String>,
    /// Whether a config read has populated the panel.
    pub loaded: bool,
    /// Outcome of the last save/login attempt, for the panel footer.
    pub notice: Option<String>,
}

impl Settings {
    /// Flips visibility; returns whether the panel is now open so the caller
    /// can kick off a config read at the right moment.
    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.notice = None;
        self.open
    }

    /// Seeds the draft from a raw `config/read` result. Keys the server left
    /// unset stay empty in the draft (inherit from lower layers).
    pub fn apply_config(&mut self, result: &Value) {
        let config = &result["config"];
        let read = |key: &str| config[key].as_str().unwrap_or_default().to_string();
        self.current = SettingsDraft {
            model: read("model"),
            approval_policy: read("approval_policy"),
            sandbox_mode: read("sandbox_mode"),
            reasoning_effort: read("model_reasoning_effort"),
            verbosity: read("model_verbosity"),
            web_search: read("web_search"),
        };
        self.draft = self.current.clone();

        // The active provider's table entry lives in the flattened remainder
        // under its config-table key; the key itself is never echoed back.
        let id = read("model_provider");
        let table = &config["model_providers"][&id];
        self.provider_current = ProviderDraft {
            id: id.clone(),
            base_url: table["base_url"].as_str().unwrap_or_default().to_string(),
            model: self.current.model.clone(),
            api_key: String::new(),
        };
        self.provider = ProviderDraft {
            id,
            base_url: self.provider_current.base_url.clone(),
            model: self.provider_current.model.clone(),
            api_key: String::new(),
        };
        self.billing = billing_of(
            &self.provider.id,
            &self.provider.base_url,
            &self.provider.model,
        );
        self.loaded = true;
    }

    /// Applies a vendor pick: seeds the base URL and suggested model from
    /// the vendor's first billing entry; custom keeps whatever is typed.
    pub fn select_provider_preset(&mut self, id: &str) {
        if self.provider.id == id {
            return;
        }
        self.provider.id = id.to_string();
        let first = provider_preset(id).and_then(|preset| preset.entries.first());
        self.billing = first.map(|entry| entry.billing);
        if let Some(entry) = first
            && !id.is_empty()
        {
            self.provider.base_url = entry.base_url.to_string();
            self.provider.model = entry.model.to_string();
        }
    }

    /// Applies a billing-type pick: reseeds the base URL and suggested
    /// model from the drafted vendor's entry for that type; vendors
    /// without the picked type keep the current form.
    pub fn select_provider_billing(&mut self, billing: Billing) {
        let matched = provider_preset(&self.provider.id).and_then(|preset| {
            preset
                .entries
                .iter()
                .find(|candidate| candidate.billing == billing)
        });
        let Some(entry) = matched else {
            return;
        };
        let same_endpoint =
            entry.base_url == self.provider.base_url && entry.model == self.provider.model;
        self.billing = Some(billing);
        self.provider.base_url = entry.base_url.to_string();
        self.provider.model = entry.model.to_string();
        self.notice = Some(if same_endpoint {
            // Official design on same-URL vendors (GLM): plans swap keys.
            "Same endpoint; the billing type only changes the API key".to_string()
        } else {
            "Endpoint seeded; paste the matching API key".to_string()
        });
    }

    /// The current draft of one field, across both the top-level config
    /// and the provider form.
    pub fn value(&self, field: SettingField) -> &str {
        match field {
            SettingField::Model => &self.draft.model,
            SettingField::ApprovalPolicy => &self.draft.approval_policy,
            SettingField::SandboxMode => &self.draft.sandbox_mode,
            SettingField::ReasoningEffort => &self.draft.reasoning_effort,
            SettingField::Verbosity => &self.draft.verbosity,
            SettingField::WebSearch => &self.draft.web_search,
            SettingField::ProviderId => &self.provider.id,
            SettingField::ProviderBaseUrl => &self.provider.base_url,
            SettingField::ProviderModel => &self.provider.model,
            SettingField::ProviderApiKey => &self.provider.api_key,
        }
    }

    /// Records a user edit of one field.
    pub fn set_field(&mut self, field: SettingField, value: String) {
        match field {
            SettingField::Model => self.draft.model = value,
            SettingField::ApprovalPolicy => self.draft.approval_policy = value,
            SettingField::SandboxMode => self.draft.sandbox_mode = value,
            SettingField::ReasoningEffort => self.draft.reasoning_effort = value,
            SettingField::Verbosity => self.draft.verbosity = value,
            SettingField::WebSearch => self.draft.web_search = value,
            SettingField::ProviderId => self.provider.id = value,
            SettingField::ProviderBaseUrl => self.provider.base_url = value,
            SettingField::ProviderModel => self.provider.model = value,
            SettingField::ProviderApiKey => self.provider.api_key = value,
        }
    }

    /// Seeds the account badge from an `account/read` response.
    pub fn apply_account(&mut self, response: &GetAccountResponse) {
        self.signed_in_label = match &response.account {
            Some(Account::Chatgpt { email, .. }) => {
                Some(email.clone().unwrap_or_else(|| "ChatGPT".to_string()))
            }
            Some(Account::ApiKey {} | Account::AmazonBedrock { .. }) => Some("API key".to_string()),
            None => None,
        };
    }

    /// Consumes the drafted account API key for `account/login/start`;
    /// the draft is cleared either way.
    pub fn take_api_key(&mut self) -> String {
        std::mem::take(&mut self.api_key_draft)
    }

    /// Records the outcome of an account login/logout attempt.
    pub fn apply_login_outcome(&mut self, outcome: Result<bool, String>) {
        self.notice = Some(match outcome {
            Ok(true) => {
                self.signed_in_label = Some("API key".to_string());
                "Signed in with API key".to_string()
            }
            Ok(false) => {
                self.signed_in_label = None;
                "Signed out".to_string()
            }
            Err(message) => format!("Login failed: {message}"),
        });
    }

    /// The pending writes: every non-empty draft that differs from the
    /// effective value, as `(config.toml key, JSON value)` pairs. Provider
    /// edits are aggregated into one `model_providers.<id>` table upsert
    /// plus the top-level `model_provider`/`model` pointers.
    pub fn edits(&self) -> Vec<(String, Value)> {
        let mut edits: Vec<(String, Value)> = Vec::new();
        let mut push = |key: String, value: Value| edits.push((key, value));

        let candidates = [
            (SettingField::Model, &self.draft.model, &self.current.model),
            (
                SettingField::ApprovalPolicy,
                &self.draft.approval_policy,
                &self.current.approval_policy,
            ),
            (
                SettingField::SandboxMode,
                &self.draft.sandbox_mode,
                &self.current.sandbox_mode,
            ),
            (
                SettingField::ReasoningEffort,
                &self.draft.reasoning_effort,
                &self.current.reasoning_effort,
            ),
            (
                SettingField::Verbosity,
                &self.draft.verbosity,
                &self.current.verbosity,
            ),
            (
                SettingField::WebSearch,
                &self.draft.web_search,
                &self.current.web_search,
            ),
        ];
        for (field, draft, current) in candidates {
            if !draft.is_empty() && draft != current {
                push(field.key().to_string(), Value::String(draft.clone()));
            }
        }

        self.push_provider_edits(&mut push);
        edits
    }

    /// Whether the pending edits touch the model setup (top-level model or
    /// the provider table). Such saves only apply to threads started
    /// afterwards, so the UI opens a fresh conversation after saving them.
    pub fn touches_model_setup(&self) -> bool {
        self.edits().iter().any(|(key, _)| {
            key == "model" || key == "model_provider" || key.starts_with("model_providers")
        })
    }

    /// Provider-side edits. The key is only written when the user typed one;
    /// otherwise the existing table entry is preserved as-is.
    fn push_provider_edits(&self, push: &mut impl FnMut(String, Value)) {
        let provider = &self.provider;
        let baseline = &self.provider_current;
        let table_changed = provider.id != baseline.id
            || provider.base_url != baseline.base_url
            || provider.model != baseline.model;
        if provider.id.is_empty() || (!table_changed && provider.api_key.is_empty()) {
            return;
        }

        let mut table = serde_json::Map::new();
        table.insert(
            "name".to_string(),
            Value::String(
                provider_preset(&provider.id)
                    .map(|preset| preset.name)
                    .unwrap_or(&provider.id)
                    .to_string(),
            ),
        );
        if !provider.base_url.is_empty() {
            table.insert(
                "base_url".to_string(),
                Value::String(provider.base_url.clone()),
            );
        }
        if !provider.api_key.is_empty() {
            table.insert(
                "experimental_bearer_token".to_string(),
                Value::String(provider.api_key.clone()),
            );
        }
        push(
            format!("model_providers.{}", provider.id),
            Value::Object(table),
        );
        push(
            "model_provider".to_string(),
            Value::String(provider.id.clone()),
        );
        if !provider.model.is_empty() && provider.model != self.draft.model {
            // The provider form wins unless the user separately edited the
            // top-level model; either way "model" is written at most once.
            push("model".to_string(), Value::String(provider.model.clone()));
        }
    }

    /// Records the outcome of a save attempt for the panel footer.
    pub fn apply_saved(&mut self, outcome: Result<(), String>) {
        self.notice = Some(match outcome {
            Ok(()) => "Saved to config.toml".to_string(),
            Err(message) => format!("Save failed: {message}"),
        });
    }
}
