//! The iced update function: one place where every state transition lives.
//!
//! Requests to the app-server are produced by [`crate::commands`]; this
//! module keeps the dispatch table, connection lifecycle, and markdown
//! sidecar bookkeeping.

use crate::chat;
use crate::commands;
use crate::message::AppMode;
use crate::message::Bootstrap;
use crate::message::Message;
use crate::message::SidebarTab;
use crate::plan_view::plan_text;
use crate::state::GitOverlayStatus;
use crate::state::QuestMenu;
use crate::state::QuestRename;
use crate::state::State;
use crate::state::Status;
use crate::status_notifications;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use codex_gui_bridge::GuiEvent;
use codex_gui_core::Billing;
use codex_gui_core::MarkdownStream;
use codex_gui_core::QuestStatus;
use iced::Task;
use iced::widget::markdown;
use iced::widget::operation;
use iced_swdir_tree::DirectoryTree;
use iced_swdir_tree::DirectoryTreeEvent;

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;

/// Runs the iced update loop.
pub fn update(state: &mut State, message: Message) -> Task<Message> {
    let task = dispatch(state, message);
    stamp_user_times(state);
    task
}

fn dispatch(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Event(event) => on_event(state, event),
        Message::Bootstrap(step) => on_bootstrap(state, step),
        Message::ComposerChanged(text) => {
            state.composer = text;
            Task::none()
        }
        Message::Submit => commands::submit(state),
        Message::TurnAcked => Task::none(),
        Message::TurnSettled {
            prompt,
            images,
            result,
        } => match result {
            Ok(_) => Task::none(),
            Err(error) => {
                // The turn never started; hand the text and attachments back
                // and surface why.
                state.composer = prompt;
                state.attachments.images = images;
                state.status = Status::Ready;
                state
                    .status_board
                    .raise_banner(format!("turn failed to start: {error}"));
                Task::none()
            }
        },
        Message::LinkClicked(uri) => {
            tracing::info!(%uri, "link click ignored; external opener lands later");
            Task::none()
        }
        Message::Tick => flush_stream(state),
        Message::ApprovalDecided {
            request_id,
            decision,
        } => commands::decide_approval(state, request_id, decision),
        Message::SessionsLoaded(result) => match result {
            Ok(response) => {
                state.sessions.apply_list(&response);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "thread/list failed");
                Task::none()
            }
        },
        Message::SessionSelected(thread_id) => {
            // Opening a quest dismisses the transient overlays and the
            // scenario chip; the chip belongs to the quest it was picked
            // for.
            state.quest.close_transient();
            state.quest.scenario = None;
            commands::resume_thread(state, thread_id)
        }
        Message::SessionResumed(Ok(history)) => {
            state.quest.scenario = None;
            state.thread_id = Some(history.thread_id);
            let task = confirm_cwd(state, history.cwd);
            state.transcript.replay(&history.turns);
            rebuild_markdowns(state, &history.turns);
            state.status = Status::Ready;
            Task::batch([task, commands::load_sessions(state)])
        }
        Message::SessionResumed(Err(error)) => {
            state.status = Status::Disconnected(error.to_string());
            Task::none()
        }
        Message::SessionArchived(thread_id) => {
            commands::archive_thread(state, thread_id).chain(commands::load_sessions(state))
        }
        Message::ModelsLoaded(result) => match result {
            Ok(response) => {
                state.status_board.apply_models(&response);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "model/list failed");
                Task::none()
            }
        },
        Message::ModelSelected(id) => commands::select_model(state, id),
        Message::AccountLoaded(result) => match result {
            Ok(response) => {
                state.status_board.apply_account(&response);
                state.settings.apply_account(&response);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "account/read failed");
                Task::none()
            }
        },
        Message::SettingsToggled => {
            let opened = state.settings.toggle();
            if opened {
                commands::load_config(state)
            } else {
                Task::none()
            }
        }
        Message::ConfigLoaded(result) => match result {
            Ok(value) => {
                state.settings.apply_config(&value);
                // The status bar mirrors the effective model, not the
                // catalog seed.
                let model = state.settings.current.model.clone();
                state.status_board.sync_model(&model);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "config/read failed");
                Task::none()
            }
        },
        Message::SettingEdited { field, value } => {
            state.settings.set_field(field, value);
            Task::none()
        }
        Message::SettingsSave => commands::save_settings(state),
        Message::SettingsSaved(result) => {
            match result {
                Ok(_response) => {
                    // Provider/model edits only apply to threads started
                    // afterwards; reopen the conversation thread for them.
                    let touched = state.settings.touches_model_setup();
                    state.settings.apply_saved(Ok(()));
                    if touched {
                        state.settings.notice = Some(
                            "Saved; fresh session started with the new model setup".to_string(),
                        );
                    }
                    let refresh = commands::load_config(state);
                    if touched {
                        refresh.chain(commands::restart_thread(state))
                    } else {
                        refresh
                    }
                }
                Err(error) => {
                    state.settings.apply_saved(Err(error.to_string()));
                    Task::none()
                }
            }
        }
        Message::AccountApiKeyChanged(text) => {
            state.settings.api_key_draft = text;
            Task::none()
        }
        Message::LoginSubmit => commands::login_with_api_key(state),
        Message::LoginCompleted(result) => match result {
            Ok(_) => {
                state.settings.apply_login_outcome(Ok(true));
                commands::load_account(state)
            }
            Err(error) => {
                state.settings.apply_login_outcome(Err(error.to_string()));
                Task::none()
            }
        },
        Message::LogoutSubmit => commands::logout(state),
        Message::LogoutCompleted(result) => match result {
            Ok(_) => {
                state.settings.apply_login_outcome(Ok(false));
                commands::load_account(state)
            }
            Err(error) => {
                state.settings.apply_login_outcome(Err(error.to_string()));
                Task::none()
            }
        },
        Message::ProviderPresetSelected(id) => {
            state.settings.select_provider_preset(&id);
            Task::none()
        }
        Message::ProviderBillingSelected(label) => {
            if let Some(billing) = Billing::from_label(&label) {
                state.settings.select_provider_billing(billing);
            }
            Task::none()
        }
        Message::SkillsLoaded(result) => match result {
            Ok(response) => {
                state.skills.apply_list(&response);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "skills/list failed");
                Task::none()
            }
        },
        Message::SkillsPickerToggled => {
            state.skills.picker_open = !state.skills.picker_open;
            Task::none()
        }
        Message::SkillPicked(name) => {
            if let Some(mention) = state.skills.mention(&name) {
                state.composer.push_str(&mention);
            }
            state.skills.picker_open = false;
            Task::none()
        }
        Message::SkillEnabledChanged { name, enabled } => {
            commands::set_skill_enabled(state, name, enabled)
        }
        Message::SkillEnabledSaved { name, result } => match result {
            Ok(value) => {
                let effective = value["effectiveEnabled"].as_bool().unwrap_or(false);
                state.skills.apply_enabled(&name, effective);
                Task::none()
            }
            Err(error) => {
                state
                    .status_board
                    .raise_banner(format!("skill toggle failed: {error}"));
                Task::none()
            }
        },
        Message::ErrorDismissed => {
            state.status_board.dismiss_error();
            Task::none()
        }
        Message::DragHovered => {
            state.attachments.hovered = true;
            Task::none()
        }
        Message::DragLeft => {
            state.attachments.hovered = false;
            Task::none()
        }
        Message::FileDropped(path) => {
            if state.attachments.admit(&path) {
                tracing::info!(path = %path.display(), "image attachment dropped");
            } else {
                tracing::debug!(path = %path.display(), "dropped file is not an image");
            }
            state.attachments.hovered = false;
            Task::none()
        }
        Message::AttachImages => commands::attach_images(),
        Message::ImagesPicked(paths) => {
            for path in paths {
                state.attachments.admit(&path);
            }
            state.attachments.hovered = false;
            Task::none()
        }
        Message::AttachmentRemoved(index) => {
            state.attachments.remove(index);
            Task::none()
        }
        Message::FolderPickRequested => commands::pick_folder(),
        Message::FolderPicked(picked) => match picked {
            Some(path) => {
                state.tree = DirectoryTree::new(path.clone());
                // Previews from the previous workspace no longer belong
                // here; drop every open tab with it.
                state.editor = crate::state::EditorPane::default();
                state.quest.scenario = None;
                state.quest.workspace_menu = false;
                commands::restart_thread_in(state, Some(path.to_string_lossy().into_owned()))
            }
            // The user closed the picker; keep the live session as-is.
            None => Task::none(),
        },
        Message::Tree(event) => {
            // Every event routes back into the tree widget so its row
            // selection state stays fresh; a file-row click additionally
            // opens the central preview.
            let tree_task = state.tree.update(event.clone()).map(Message::Tree);
            if let DirectoryTreeEvent::Selected(path, /*is_dir*/ false, _) = &event {
                let path = path.clone();
                return if state.editor.open(path.clone()) {
                    Task::batch([tree_task, commands::read_preview(path)])
                } else {
                    tree_task
                };
            }
            tree_task
        }
        Message::FilePreviewLoaded { path, body } => {
            state.editor.bodies.insert(path, body);
            Task::none()
        }
        Message::FilePreviewSelected(path) => {
            state.editor.select(&path);
            Task::none()
        }
        Message::FilePreviewClosed(path) => {
            state.editor.close(&path);
            Task::none()
        }
        Message::SidebarToggled => {
            state.sidebar_tab = if state.sidebar_tab == SidebarTab::Files {
                SidebarTab::Threads
            } else {
                SidebarTab::Files
            };
            Task::none()
        }
        Message::SidebarTab(tab) => {
            state.sidebar_tab = tab;
            Task::none()
        }
        Message::CopyMessage { id, text } => {
            state.copied_id = Some(id);
            iced::clipboard::write(text)
        }
        Message::ReactionToggled { id, reaction } => {
            // Same reaction pressed again clears it; otherwise it replaces.
            if state.reactions.get(&id) == Some(&reaction) {
                state.reactions.remove(&id);
            } else {
                state.reactions.insert(id, reaction);
            }
            Task::none()
        }
        Message::CommandCardToggled(id) => {
            // Toggle the output well under one command tool card.
            if !state.expanded_commands.remove(&id) {
                state.expanded_commands.insert(id);
            }
            Task::none()
        }
        Message::CommandGroupToggled(id) => {
            // Expand or collapse one run of consecutive command cards.
            if !state.expanded_command_groups.remove(&id) {
                state.expanded_command_groups.insert(id);
            }
            Task::none()
        }
        Message::ChatSearchOpened => {
            state.chat_search.open = true;
            operation::focus(iced::widget::Id::new(chat::SEARCH_INPUT_ID))
        }
        Message::ChatSearchClosed => {
            state.chat_search = crate::state::ChatSearch::default();
            Task::none()
        }
        Message::ChatSearchQueryChanged(query) => {
            state.chat_search.query = query;
            state.chat_search.hit = 0;
            Task::none()
        }
        Message::ChatSearchNext => {
            chat::step_search_hit(state, 1);
            Task::none()
        }
        Message::ChatSearchPrev => {
            chat::step_search_hit(state, -1);
            Task::none()
        }
        Message::SidebarFilterChanged(filter) => {
            state.sidebar_filter = filter;
            Task::none()
        }
        Message::SessionPinToggled(id) => {
            state.pins.toggle(&id);
            state.pins.save();
            Task::none()
        }
        Message::DiffOverlayToggled => {
            state.diff_overlay_open = !state.diff_overlay_open;
            Task::none()
        }
        Message::DiffFileSelected(index) => {
            state.diff_selected = Some(index);
            Task::none()
        }
        Message::PlanOverlayToggled => {
            state.plan_overlay_open = !state.plan_overlay_open;
            Task::none()
        }
        Message::PlanQuoted => {
            if let Some(plan) = state.transcript.plan() {
                state.composer = plan_text(plan);
            }
            state.plan_overlay_open = false;
            Task::none()
        }
        Message::GitOverlayToggled => {
            state.git_overlay.open = !state.git_overlay.open;
            if !state.git_overlay.open {
                // Closing cancels a running draft and resets the form.
                state.git_overlay.selected.clear();
                state.git_overlay.status = GitOverlayStatus::Idle;
                state.git_overlay.draft_thread = None;
                state.git_overlay.draft_diff = None;
            }
            Task::none()
        }
        Message::GitFileToggled(index) => {
            // Same checkbox pressed again clears it; otherwise it joins.
            if !state.git_overlay.selected.remove(&index) {
                state.git_overlay.selected.insert(index);
            }
            Task::none()
        }
        Message::GitFilesSelectAll(select_all) => {
            state.git_overlay.selected = if select_all {
                (0..state.git.change_count()).collect()
            } else {
                std::collections::BTreeSet::new()
            };
            Task::none()
        }
        Message::GitCommitDraftChanged(text) => {
            state.git_overlay.message = text;
            Task::none()
        }
        Message::GitCommitRequested => commands::commit_selected(state),
        Message::GitAiDraftRequested => commands::request_commit_draft(state),
        Message::CommitFinished(Ok(())) => {
            state.git_overlay.open = false;
            state.git_overlay.status = GitOverlayStatus::Idle;
            state
                .status_board
                .cwd()
                .map(|cwd| commands::refresh_git(cwd.to_string()))
                .unwrap_or_else(Task::none)
        }
        Message::CommitFinished(Err(stderr)) => {
            state.git_overlay.status = GitOverlayStatus::Failed(stderr);
            Task::none()
        }
        Message::DraftThreadStarted { diff, result } => match result {
            Ok(thread_id) => {
                state.git_overlay.draft_diff = Some(diff);
                state.git_overlay.draft_thread = Some(thread_id);
                commands::start_draft_turn(state)
            }
            Err(error) => {
                state.git_overlay.status =
                    GitOverlayStatus::Failed(format!("draft thread failed: {error}"));
                Task::none()
            }
        },
        Message::CommitDraftFailed(reason) => {
            state.git_overlay.status = GitOverlayStatus::Failed(reason);
            Task::none()
        }
        Message::Reconnect => reconnect(state),
        Message::Noop => Task::none(),
        Message::NewThreadRequested => {
            state.quest.scenario = None;
            commands::restart_thread(state)
        }
        Message::PaletteToggled => {
            state.palette.open = !state.palette.open;
            state.palette.query.clear();
            state.palette.selected = 0;
            if state.palette.open {
                // Hand the query input the keyboard so the palette is
                // typable the moment it appears.
                iced::widget::operation::focus(crate::command_palette::PALETTE_INPUT_ID)
            } else {
                Task::none()
            }
        }
        Message::PaletteQueryChanged(text) => {
            state.palette.query = text;
            state.palette.selected = 0;
            Task::none()
        }
        Message::PaletteMoved(delta) => {
            crate::command_palette::move_selection(&mut state.palette, delta);
            Task::none()
        }
        Message::PaletteConfirmed => {
            // The overlay subscription and the focused input can both fire
            // this; only the first one (while still open) runs the action.
            if !state.palette.open {
                return Task::none();
            }
            match crate::command_palette::confirmed_action(&state.palette) {
                Some(action) => {
                    state.palette.open = false;
                    state.palette.query.clear();
                    state.palette.selected = 0;
                    // Re-run the action through the regular dispatch path.
                    dispatch(state, action)
                }
                None => Task::none(),
            }
        }
        Message::GitInfoArrived(info) => {
            state.git = info;
            Task::none()
        }
        Message::MenuToggled(id) => {
            // Clicking the open title again closes its dropdown; another
            // title switches the panel over to itself.
            state.menu = if state.menu == Some(id) {
                None
            } else {
                Some(id)
            };
            Task::none()
        }
        Message::MenuClosed => {
            state.menu = None;
            Task::none()
        }
        Message::ModeToggled => {
            state.mode = match state.mode {
                AppMode::Editor => AppMode::Quest,
                AppMode::Quest => AppMode::Editor,
            };
            state.menu = None;
            state.quest.close_transient();
            Task::none()
        }
        Message::QuestCreatePressed => {
            state.quest.picker_open = true;
            state.quest.menu = QuestMenu::default();
            Task::none()
        }
        Message::QuestScenarioPicked(Some(scenario)) => {
            state.quest.picker_open = false;
            state.quest.scenario = Some(scenario);
            // Seed the composer once so the first turn carries the intent;
            // existing drafts stay untouched.
            if state.composer.is_empty() {
                state.composer = String::from(scenario.template());
            }
            commands::restart_thread(state)
        }
        Message::QuestScenarioPicked(None) => {
            state.quest.picker_open = false;
            Task::none()
        }
        Message::QuestMenuToggled(thread_id) => {
            // Pressing the open row's entry again closes its menu; another
            // row switches the menu over to itself.
            state.quest.menu = if state.quest.menu.thread_id.as_deref() == Some(thread_id.as_str())
            {
                QuestMenu::default()
            } else {
                QuestMenu {
                    thread_id: Some(thread_id),
                    confirm_delete: false,
                }
            };
            Task::none()
        }
        Message::QuestMenuClosed => {
            state.quest.menu = QuestMenu::default();
            Task::none()
        }
        Message::QuestDeleteArmed => {
            state.quest.menu.confirm_delete = true;
            Task::none()
        }
        Message::QuestDeleteRequested(thread_id) => {
            state.quest.menu = QuestMenu::default();
            // Optimistically drop the row, then refresh from the server;
            // deleting the live quest reopens a fresh thread.
            state.sessions.remove(&thread_id);
            let refresh = commands::delete_thread(state, thread_id.clone())
                .chain(commands::load_sessions(state));
            if state.thread_id.as_deref() == Some(thread_id.as_str()) {
                state.quest.scenario = None;
                return refresh.chain(commands::restart_thread(state));
            }
            refresh
        }
        Message::QuestForkRequested(thread_id) => {
            state.quest.menu = QuestMenu::default();
            commands::fork_thread(state, thread_id)
        }
        Message::QuestForked { source: _, result } => match result {
            Ok(thread_id) => {
                // The fork lands as a new quest: reopen it so the user
                // continues there, and refresh the inventory behind it.
                state.quest.scenario = None;
                Task::batch([
                    commands::resume_thread(state, thread_id),
                    commands::load_sessions(state),
                ])
            }
            Err(error) => {
                state
                    .status_board
                    .raise_banner(format!("fork failed: {error}"));
                Task::none()
            }
        },
        Message::QuestRenameRequested(thread_id) => {
            state.quest.menu = QuestMenu::default();
            let current = state
                .sessions
                .threads()
                .iter()
                .find(|thread| thread.id == thread_id)
                .map_or(String::new(), |thread| String::from(thread.label()));
            state.quest.rename = Some(QuestRename {
                thread_id,
                draft: current,
            });
            operation::focus(iced::widget::Id::new(
                crate::quest_overlays::RENAME_INPUT_ID,
            ))
        }
        Message::QuestRenameDraftChanged(text) => {
            if let Some(rename) = state.quest.rename.as_mut() {
                rename.draft = text;
            }
            Task::none()
        }
        Message::QuestRenameConfirmed => {
            // The dialog input and the overlay subscription can both fire
            // this; only the first one (while still open) runs the rename.
            let Some(rename) = state.quest.rename.take() else {
                return Task::none();
            };
            let name = rename.draft.trim().to_string();
            if name.is_empty() {
                // An empty name would clear the label; keep the dialog open.
                state.quest.rename = Some(rename);
                return Task::none();
            }
            commands::set_thread_name(state, rename.thread_id, name)
        }
        Message::QuestRenameCancelled => {
            state.quest.rename = None;
            Task::none()
        }
        Message::QuestRenamed {
            thread_id,
            name,
            result,
        } => match result {
            Ok(_) => {
                state.sessions.apply_name(&thread_id, Some(name));
                Task::none()
            }
            Err(error) => {
                state
                    .status_board
                    .raise_banner(format!("rename failed: {error}"));
                Task::none()
            }
        },
        Message::QuestBoardToggled => {
            state.quest.board_open = !state.quest.board_open;
            if state.quest.board_open {
                state.quest.menu = QuestMenu::default();
            }
            Task::none()
        }
        Message::QuestWorkspaceToggled => {
            state.quest.workspace_menu = !state.quest.workspace_menu;
            Task::none()
        }
        Message::QuestListExpanded => {
            state.quest.list_expanded = !state.quest.list_expanded;
            Task::none()
        }
        Message::QuestBoardFilterChanged(text) => {
            state.quest.board_filter = text;
            Task::none()
        }
        Message::QuestBoardProjectPicked(project) => {
            state.quest.board_project = project;
            Task::none()
        }
        Message::QuestOverlaysClosed => {
            state.quest.close_transient();
            Task::none()
        }
        Message::ComposerCleared => {
            state.composer.clear();
            Task::none()
        }
        Message::GitRefreshRequested => state
            .status_board
            .cwd()
            .map(|cwd| commands::refresh_git(cwd.to_string()))
            .unwrap_or_else(Task::none),
    }
}

/// Records the server-confirmed working directory: status bar, recent
/// projects, and a fresh git probe.
fn confirm_cwd(state: &mut State, cwd: String) -> Task<Message> {
    state.status_board.apply_cwd(cwd.clone());
    state.recents.remember(&cwd);
    state.recents.save();
    commands::refresh_git(cwd)
}

/// Timestamps user messages the update just surfaced; existing stamps are
/// kept, so resumed history keeps its first-seen arrival time.
fn stamp_user_times(state: &mut State) {
    let now = now_secs();
    for entry in state.transcript.entries() {
        if let codex_gui_core::Entry::UserMessage { id, .. } = entry {
            state.user_times.entry(id.clone()).or_insert(now);
        }
    }
}

/// Unix seconds via the std clock; no chrono dependency.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| 0, |d| d.as_secs() as i64)
}

/// Connection lifecycle events.
fn on_event(state: &mut State, event: GuiEvent) -> Task<Message> {
    match event {
        GuiEvent::Connected(client) => {
            state.client = Some(client);
            commands::initialize(state)
        }
        GuiEvent::Notification(notification) => {
            if let ServerNotification::ThreadArchived(archived) = &notification {
                state.sessions.remove(&archived.thread_id);
            }
            if let ServerNotification::ThreadDeleted(deleted) = &notification {
                state.sessions.remove(&deleted.thread_id);
            }
            if let ServerNotification::ThreadStatusChanged(changed) = &notification {
                state
                    .sessions
                    .apply_status(&changed.thread_id, QuestStatus::from(&changed.status));
            }
            if let ServerNotification::ThreadNameUpdated(updated) = &notification {
                state
                    .sessions
                    .apply_name(&updated.thread_id, updated.thread_name.clone());
            }
            if let ServerNotification::Error(error) = &notification {
                state.status_board.raise_error(error);
            }
            if let ServerNotification::SkillsChanged(_) = &notification {
                // An invalidation signal: refresh the inventory instead of
                // feeding the notification to the transcript.
                return commands::load_skills(state);
            }
            // The throwaway commit-draft thread lives outside the main
            // conversation: capture its agent message (or its silent
            // failure) and archive it, keeping all other draft traffic
            // out of the transcript and the status bar.
            if let Some(draft_thread) = state.git_overlay.draft_thread.clone()
                && notification_thread_id(&notification) == Some(&draft_thread)
            {
                if let ServerNotification::ItemCompleted(completed) = &notification
                    && let ThreadItem::AgentMessage { text, .. } = &completed.item
                {
                    state.git_overlay.message = text.clone();
                    state.git_overlay.draft_diff = None;
                    state.git_overlay.draft_thread = None;
                    state.git_overlay.status = GitOverlayStatus::Idle;
                    return commands::archive_thread(state, draft_thread);
                }
                if let ServerNotification::TurnCompleted(turn) = &notification
                    && turn.thread_id == draft_thread
                {
                    state.git_overlay.draft_diff = None;
                    state.git_overlay.draft_thread = None;
                    state.git_overlay.status =
                        GitOverlayStatus::Failed(String::from("draft turn produced no message"));
                }
                return Task::none();
            }
            if let ServerNotification::ThreadTokenUsageUpdated(usage) = &notification {
                // Only the live thread's usage drives the context chip.
                if Some(&usage.thread_id) == state.thread_id.as_ref() {
                    state.token_usage = Some(usage.token_usage.clone());
                }
            }
            // A finished turn reopens the composer no matter how it ended;
            // `submit` closed it with `Status::Thinking` and only this
            // notification marks the end of the turn lifecycle.
            if let ServerNotification::TurnCompleted(completed) = &notification
                && Some(&completed.thread_id) == state.thread_id.as_ref()
                && completed.turn.status != TurnStatus::InProgress
            {
                state.status = Status::Ready;
            }
            if let ServerNotification::TurnDiffUpdated(diff) = &notification {
                // Same thread filter: stale turns must not paint the chip.
                if Some(&diff.thread_id) == state.thread_id.as_ref() {
                    state.turn_diff = Some(diff.diff.clone());
                }
            }
            let _status_applied = status_notifications::apply(state, &notification);
            sync_markdown(state, &notification);
            if let ServerNotification::ServerRequestResolved(resolved) = &notification {
                // The server closed the request itself (turn cancelled, ...);
                // keep the dialog queue in sync.
                let _removed = state.approvals.resolved(&resolved.request_id);
                return Task::none();
            }
            state.transcript.apply(&notification);
            Task::none()
        }
        GuiEvent::ServerRequest(request) => {
            // Escalate to a dialog; the payload answer goes out once the
            // user decides.
            if let Some(pending) = state.approvals.offered(&request) {
                tracing::info!(request = %pending.request_id, "approval requested");
            } else {
                tracing::info!("server request ignored; no dialog kind");
            }
            Task::none()
        }
        GuiEvent::Undecodable(raw) => {
            tracing::warn!(%raw, "undecodable frame reached the UI");
            Task::none()
        }
        GuiEvent::Disconnected(reason) => {
            state.status = Status::Disconnected(reason);
            Task::none()
        }
    }
}

/// Bootstrap chain: `initialize` -> `thread/start` -> [`Status::Ready`];
/// the sidebar list loads in parallel.
fn on_bootstrap(state: &mut State, step: Bootstrap) -> Task<Message> {
    match step {
        Bootstrap::Initialized(Ok(_)) => Task::batch([
            commands::start_thread(state),
            commands::load_sessions(state),
            commands::load_models(state),
            commands::load_account(state),
            // The status bar mirrors the configured model; load it eagerly
            // instead of waiting for the first settings-panel open.
            commands::load_config(state),
            // The skills inventory feeds the composer picker and toggles.
            commands::load_skills(state),
        ]),
        Bootstrap::Initialized(Err(error)) => {
            state.status = Status::Disconnected(error.to_string());
            Task::none()
        }
        Bootstrap::ThreadStarted(Ok(result)) => {
            state.thread_id = result["thread"]["id"].as_str().map(ToString::to_string);
            let confirmed = result["cwd"]
                .as_str()
                .map(|cwd| confirm_cwd(state, cwd.to_string()))
                .unwrap_or_else(Task::none);
            state.status = Status::Ready;
            confirmed
        }
        Bootstrap::ThreadStarted(Err(error)) => {
            state.status = Status::Disconnected(error.to_string());
            Task::none()
        }
    }
}

/// Relaunches the app-server after a disconnect: bumping the epoch changes
/// the subscription identity, so iced drops the dead connection stream and
/// starts a fresh one; the bootstrap chain reruns from `Connected`.
fn reconnect(state: &mut State) -> Task<Message> {
    state.connection_epoch += 1;
    state.client = None;
    state.thread_id = None;
    state.transcript = codex_gui_core::Transcript::default();
    state.markdowns.clear();
    state.stream = MarkdownStream::default();
    state.approvals = codex_gui_core::Approvals::default();
    state.sessions = codex_gui_core::Sessions::default();
    state.skills = codex_gui_core::SkillsBoard::default();
    state.attachments = crate::attachments::Attachments::default();
    state.status = Status::Bootstrapping;
    Task::none()
}

/// Keeps the parsed-markdown sidecar aligned with the transcript before the
/// reducer runs. Deltas only buffer here; the render tick flushes them into
/// [`markdown::Content`] so long replies parse at most once per tick.
fn sync_markdown(state: &mut State, notification: &ServerNotification) {
    match notification {
        ServerNotification::ItemStarted(started) => {
            if let ThreadItem::AgentMessage { id, text, .. } = &started.item {
                state
                    .markdowns
                    .insert(id.clone(), markdown::Content::parse(text));
            }
        }
        ServerNotification::ItemCompleted(completed) => {
            if let ThreadItem::AgentMessage { id, text, .. } = &completed.item {
                // The completed snapshot supersedes any in-flight deltas.
                state.stream.cancel(id);
                state
                    .markdowns
                    .insert(id.clone(), markdown::Content::parse(text));
            }
        }
        ServerNotification::AgentMessageDelta(delta) => {
            state.stream.push(delta.item_id.clone(), &delta.delta);
        }
        _other => {}
    }
}

/// Moves buffered stream text into the parsed markdown cache and snaps the
/// transcript back to the bottom; runs at most once per tick.
fn flush_stream(state: &mut State) -> Task<Message> {
    let drained = state.stream.drain();
    if drained.is_empty() {
        return Task::none();
    }

    for (item_id, text) in drained {
        state.markdowns.entry(item_id).or_default().push_str(&text);
    }

    operation::snap_to_end(chat::TRANSCRIPT_ID)
}

/// Re-parses markdown for replayed agent messages after a resume.
fn rebuild_markdowns(state: &mut State, turns: &[Turn]) {
    for turn in turns {
        for item in &turn.items {
            if let ThreadItem::AgentMessage { id, text, .. } = item {
                state
                    .markdowns
                    .insert(id.clone(), markdown::Content::parse(text));
            }
        }
    }
}

/// The thread a notification belongs to, for draft-thread isolation; the
/// threadless status notifications (warnings, reroutes) never match.
fn notification_thread_id(notification: &ServerNotification) -> Option<&String> {
    match notification {
        ServerNotification::ItemStarted(n) => Some(&n.thread_id),
        ServerNotification::ItemCompleted(n) => Some(&n.thread_id),
        ServerNotification::AgentMessageDelta(n) => Some(&n.thread_id),
        ServerNotification::CommandExecutionOutputDelta(n) => Some(&n.thread_id),
        ServerNotification::TurnStarted(n) => Some(&n.thread_id),
        ServerNotification::TurnCompleted(n) => Some(&n.thread_id),
        ServerNotification::TurnPlanUpdated(n) => Some(&n.thread_id),
        ServerNotification::TurnDiffUpdated(n) => Some(&n.thread_id),
        ServerNotification::ThreadTokenUsageUpdated(n) => Some(&n.thread_id),
        ServerNotification::ThreadArchived(n) => Some(&n.thread_id),
        _ => None,
    }
}
