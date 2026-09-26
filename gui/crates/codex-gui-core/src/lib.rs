//! UI-agnostic conversation state for the Codex GUI.
//!
//! The crate deliberately has no dependency on `iced`: the transcript model is
//! driven purely by app-server notifications and is unit-testable headlessly.
//! The UI layer renders whatever [`Transcript`] holds.

mod approvals;
mod compose;
mod elicitation;
mod git_info;
mod markdown_stream;
mod mentions;
mod model_vendors;
mod pins;
mod plugin_i18n;
mod questions;
mod recent_projects;
mod remote_targets;
mod search;
mod sessions;
mod settings;
mod skill_files;
mod skill_i18n;
mod skill_market;
mod skills;
mod status;
mod transcript;

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod approvals_tests;

#[cfg(test)]
#[path = "elicitation_tests.rs"]
mod elicitation_tests;

#[cfg(test)]
#[path = "markdown_stream_tests.rs"]
mod markdown_stream_tests;

#[cfg(test)]
#[path = "mentions_tests.rs"]
mod mentions_tests;

#[cfg(test)]
#[path = "questions_tests.rs"]
mod questions_tests;

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod sessions_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "skill_files_tests.rs"]
mod skill_files_tests;

#[cfg(test)]
#[path = "skill_market_tests.rs"]
mod skill_market_tests;

#[cfg(test)]
#[path = "skills_tests.rs"]
mod skills_tests;

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod transcript_tests;

pub use approvals::ApprovalKind;
pub use approvals::Approvals;
pub use approvals::Decision;
pub use approvals::PendingApproval;
pub use compose::compose_inputs;
pub use elicitation::ElicitationDraft;
pub use elicitation::Elicitations;
pub use elicitation::FieldKind;
pub use elicitation::FormField;
pub use elicitation::PendingElicitation;
pub use elicitation::auto_decline_elicitation;
pub use elicitation::default_drafts_for as elicitation_default_drafts;
pub use elicitation::form_content as elicitation_form_content;
pub use elicitation::form_fields as elicitation_form_fields;
pub use elicitation::response_payload as elicitation_response_payload;
pub use git_info::GitCommit;
pub use git_info::GitInfo;
pub use git_info::GitStatus;
pub use markdown_stream::MarkdownStream;
pub use mentions::MentionHit;
pub use mentions::Mentions;
pub use model_vendors::ConfiguredProvider;
pub use model_vendors::ModelChoice;
pub use model_vendors::VendorCatalog;
pub use model_vendors::VendorGroup;
pub use pins::PinnedThreads;
pub use questions::PendingQuestion;
pub use questions::QuestionDraft;
pub use questions::Questions;
pub use questions::option_label as question_option_label;
pub use questions::option_rows as question_option_rows;
pub use questions::response_payload as question_response_payload;
pub use recent_projects::RecentProjects;
pub use remote_targets::SshTarget;
pub use remote_targets::docker_available;
pub use remote_targets::ssh_targets;
pub use search::SearchField;
pub use search::SearchHit;
pub use search::search;
pub use sessions::QuestStatus;
pub use sessions::SessionHistory;
pub use sessions::Sessions;
pub use sessions::ThreadSummary;
pub use sessions::organized_sidebar;
pub use settings::Billing;
pub use settings::PROVIDER_PRESETS;
pub use settings::ProviderDraft;
pub use settings::SettingField;
pub use settings::Settings;
pub use settings::SettingsDraft;
pub use settings::options;
pub use settings::provider_preset;
pub use skill_files::LinkImportReport;
pub use skill_files::SKILLS_DIR;
pub use skill_files::SkillDocument;
pub use skill_files::create_skill;
pub use skill_files::delete_skill;
pub use skill_files::import_skill_directory;
pub use skill_files::import_skill_file;
pub use skill_files::import_skill_from_link;
pub use skill_files::parse_skill_document;
pub use skill_files::update_skill;
pub use skill_i18n::localize as localize_skill_description;
pub use skill_market::MarketDetail;
pub use skill_market::MarketPlugin;
pub use skill_market::MarketSelector;
pub use skill_market::MarketSkill;
pub use skill_market::SkillMarket;
pub use skills::SkillAdd;
pub use skills::SkillEdit;
pub use skills::SkillNotice;
pub use skills::SkillRow;
pub use skills::SkillSource;
pub use skills::SkillsBoard;
pub use skills::SkillsTab;
pub use status::AccountBadge;
pub use status::ErrorBanner;
pub use status::ModelEntry;
pub use status::StatusBoard;
pub use transcript::Entry;
pub use transcript::FileChangeRecord;
pub use transcript::PlanStep;
pub use transcript::Transcript;
pub use transcript::TurnPlan;
