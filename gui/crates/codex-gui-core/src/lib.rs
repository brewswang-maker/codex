//! UI-agnostic conversation state for the Codex GUI.
//!
//! The crate deliberately has no dependency on `iced`: the transcript model is
//! driven purely by app-server notifications and is unit-testable headlessly.
//! The UI layer renders whatever [`Transcript`] holds.

mod approvals;
mod compose;
mod git_info;
mod markdown_stream;
mod pins;
mod recent_projects;
mod search;
mod sessions;
mod settings;
mod skills;
mod status;
mod transcript;

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod approvals_tests;

#[cfg(test)]
#[path = "markdown_stream_tests.rs"]
mod markdown_stream_tests;

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod sessions_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;

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
pub use git_info::GitInfo;
pub use git_info::GitStatus;
pub use markdown_stream::MarkdownStream;
pub use pins::PinnedThreads;
pub use recent_projects::RecentProjects;
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
pub use skills::SkillRow;
pub use skills::SkillsBoard;
pub use status::AccountBadge;
pub use status::ErrorBanner;
pub use status::ModelEntry;
pub use status::StatusBoard;
pub use transcript::Entry;
pub use transcript::FileChangeRecord;
pub use transcript::PlanStep;
pub use transcript::Transcript;
pub use transcript::TurnPlan;
