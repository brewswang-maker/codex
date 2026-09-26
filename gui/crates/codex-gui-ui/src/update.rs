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
use crate::state::PendingModelApply;
use crate::state::QuestMenu;
use crate::state::QuestRename;
use crate::state::State;
use crate::state::Status;
use crate::status_notifications;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequest;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;
use codex_gui_bridge::GuiEvent;
use codex_gui_core::Billing;
use codex_gui_core::MarkdownStream;
use codex_gui_core::QuestStatus;
use codex_gui_core::SkillNotice;
use codex_gui_core::SkillSource;
use codex_gui_core::SkillsTab;
use codex_gui_core::elicitation_default_drafts;
use codex_gui_core::elicitation_form_content;
use codex_gui_core::elicitation_response_payload;
use codex_gui_core::question_response_payload;
use iced::Task;
use iced::widget::markdown;
use iced::widget::operation;
use iced_swdir_tree::DirectoryTree;
use iced_swdir_tree::DirectoryTreeEvent;
use std::collections::HashMap;
use std::path::PathBuf;

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
            match state.mentions.refresh(&state.composer) {
                Some(query) => commands::search_files(state, query),
                None => Task::none(),
            }
        }
        Message::Submit => Task::batch([submit_composer(state), refocus_composer()]),
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
        Message::TurnInterruptRequested => {
            Task::batch([commands::interrupt_turn(state), refocus_composer()])
        }
        Message::TurnInterrupted(result) => {
            if let Err(error) = result {
                state
                    .status_board
                    .raise_banner(format!("turn interrupt failed: {error}"));
            }
            Task::none()
        }
        Message::ReviewStarted(result) => {
            if let Err(error) = result {
                state
                    .status_board
                    .raise_banner(format!("review failed to start: {error}"));
            } else {
                // The review runs as a turn; its completed notification
                // reopens the composer.
                state.status = Status::Thinking;
            }
            Task::none()
        }
        Message::CompactionStarted(result) => {
            if let Err(error) = result {
                state
                    .status_board
                    .raise_banner(format!("compaction failed to start: {error}"));
            } else {
                state.status = Status::Thinking;
            }
            Task::none()
        }
        Message::LinkClicked(uri) => {
            tracing::info!(%uri, "link click ignored; external opener lands later");
            Task::none()
        }
        Message::Tick => flush_stream(state),
        Message::ApprovalDecided {
            request_id,
            decision,
        } => commands::decide_approval(state, request_id, decision),
        Message::QuestionOptionPicked { question_id, index } => {
            state
                .question_drafts
                .entry(question_id)
                .or_default()
                .selected = Some(index);
            Task::none()
        }
        Message::QuestionNotesChanged { question_id, notes } => {
            state.question_drafts.entry(question_id).or_default().notes = notes;
            Task::none()
        }
        Message::QuestionAnswered => answer_questions(state),
        Message::ElicitationFieldTextChanged { key, text } => {
            state.elicitation_drafts.entry(key).or_default().text = text;
            state.elicitation_error = None;
            Task::none()
        }
        Message::ElicitationFieldToggled { key } => {
            let draft = state.elicitation_drafts.entry(key).or_default();
            draft.boolean = !draft.boolean;
            state.elicitation_error = None;
            Task::none()
        }
        Message::ElicitationFieldPicked { key, value } => {
            state.elicitation_drafts.entry(key).or_default().single = Some(value);
            state.elicitation_error = None;
            Task::none()
        }
        Message::ElicitationFieldValueToggled { key, value } => {
            let draft = state.elicitation_drafts.entry(key).or_default();
            if let Some(position) = draft.multi.iter().position(|selected| *selected == value) {
                draft.multi.remove(position);
            } else {
                draft.multi.push(value);
            }
            state.elicitation_error = None;
            Task::none()
        }
        Message::ElicitationAnswered(action) => answer_elicitation(state, action),
        Message::ElicitationLinkCopied(url) => iced::clipboard::write(url),
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
            // The resumed thread keeps its own model, which can differ from
            // the configured default; the status bar mirrors the thread.
            state.status_board.sync_model(&history.model);
            state.active_binding = Some((history.model_provider, history.model));
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
        Message::ModelAppliedInPlace(result) => match result {
            Ok(_response) => Task::none(),
            Err(error) => {
                tracing::warn!(%error, "thread/settings/update failed");
                state.settings.notice = Some(format!("model switch failed: {error}"));
                Task::none()
            }
        },
        Message::ModelSwitchForked(result) => match result {
            Ok(thread_id) => Task::batch([
                commands::resume_thread(state, thread_id),
                commands::load_sessions(state),
            ]),
            Err(error) => {
                // The config write already landed; the conversation stays
                // on its previous pair instead of silently restarting.
                tracing::warn!(%error, "thread/fork (model switch) failed");
                state.status_board.raise_banner(format!(
                    "model switch failed: {error}; the conversation stayed on its previous model"
                ));
                Task::none()
            }
        },
        Message::SessionArchived(thread_id) => {
            commands::archive_thread(state, thread_id).chain(commands::load_sessions(state))
        }
        Message::SessionUnarchived(thread_id) => commands::unarchive_thread(state, thread_id)
            .chain(commands::load_sessions(state))
            .chain(commands::load_archived_sessions(state)),
        Message::ArchivedToggled => {
            state.archived_open = !state.archived_open;
            if state.archived_open && !state.archived_loaded {
                commands::load_archived_sessions(state)
            } else {
                Task::none()
            }
        }
        Message::ArchivedSessionsLoaded(result) => match result {
            Ok(response) => {
                state.archived.apply_list(&response);
                state.archived_loaded = true;
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "thread/list (archived) failed");
                state.archived_loaded = true;
                Task::none()
            }
        },
        Message::ModelsLoaded(result) => match result {
            Ok(response) => {
                state.status_board.apply_models(&response);
                attach_vendor_catalog(state);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "model/list failed");
                Task::none()
            }
        },
        Message::ModelMenuToggled => {
            state.model_menu_open = !state.model_menu_open;
            if state.model_menu_open {
                // Refresh both inputs while opening: the config for the
                // vendor groups, the catalog for the active models.
                Task::batch([commands::load_config(state), commands::load_models(state)])
            } else {
                Task::none()
            }
        }
        Message::ModelMenuClosed => {
            state.model_menu_open = false;
            Task::none()
        }
        Message::VendorModelPicked { provider_id, slug } => {
            state.model_menu_open = false;
            let (current_provider, current_model) = state.vendors.current();
            let config_matches = current_provider == Some(provider_id.as_str())
                && current_model == Some(slug.as_str());
            // The live thread's own binding is authoritative for whether
            // the pick changes anything; without one (no thread open yet),
            // the config pair the menu was rendered from stands in.
            let binding_matches = match state.active_binding.as_ref() {
                Some((provider, model)) => provider == &provider_id && model == &slug,
                None => config_matches,
            };
            if config_matches && binding_matches {
                // Re-picking the live pair only dismisses the menu.
                return Task::none();
            }
            state.pending_model_apply = if binding_matches {
                // The thread already runs this pair; the write only
                // realigns the saved default with it.
                None
            } else {
                match state.thread_id.clone() {
                    None => None,
                    Some(thread_id) => {
                        let same_vendor = state
                            .active_binding
                            .as_ref()
                            .is_some_and(|(provider, _)| provider == &provider_id);
                        if same_vendor {
                            Some(PendingModelApply::InPlace {
                                thread_id,
                                model: slug.clone(),
                            })
                        } else {
                            // The provider is fixed per thread: only a
                            // forked or fresh thread can run the new
                            // vendor.
                            Some(PendingModelApply::Retarget {
                                provider_id: provider_id.clone(),
                                model: slug.clone(),
                            })
                        }
                    }
                }
            };
            state.model_switch_pending = true;
            commands::select_vendor_model(state, provider_id, slug)
        }
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
                // One full-screen surface at a time: the Skills page yields.
                state.skills.close_page();
                Task::batch([
                    commands::load_config(state),
                    commands::load_mcp_servers(state),
                ])
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
                apply_vendor_config(state, &value);
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "config/read failed");
                Task::none()
            }
        },
        Message::McpServersLoaded(result) => match result {
            Ok(response) => {
                state.mcp_servers = response.data;
                state.mcp_loaded = true;
                Task::none()
            }
            Err(error) => {
                tracing::warn!(%error, "mcpServerStatus/list failed");
                // Surface the failure as an empty inventory instead of a
                // perpetual loading state; reopening the panel retries.
                state.mcp_loaded = true;
                Task::none()
            }
        },
        Message::McpLoginRequested(name) => {
            state.settings.notice = Some(format!("requesting sign-in link for {name}…"));
            commands::mcp_login(state, name)
        }
        Message::McpLoginStarted { name, result } => match result {
            Ok(response) => {
                state.mcp_login = Some(crate::state::McpLogin {
                    name: name.clone(),
                    url: response.authorization_url,
                });
                state.settings.notice = Some(format!("open the sign-in link to connect {name}"));
                Task::none()
            }
            Err(error) => {
                state.settings.notice = Some(format!("sign-in for {name} failed: {error}"));
                Task::none()
            }
        },
        Message::McpLoginLinkCopied => {
            let Some(login) = state.mcp_login.as_ref() else {
                return Task::none();
            };
            state.settings.notice = Some(format!("sign-in link for {} copied", login.name));
            iced::clipboard::write(login.url.clone())
        }
        Message::McpLoginDismissed => {
            state.mcp_login = None;
            Task::none()
        }
        Message::SettingEdited { field, value } => {
            state.settings.set_field(field, value);
            Task::none()
        }
        Message::SettingsSave => commands::save_settings(state),
        Message::SettingsSaved(result) => {
            match result {
                Ok(_response) => {
                    let menu_pick = std::mem::take(&mut state.model_switch_pending);
                    let panel_edit = state.settings.touches_model_setup();
                    let pending = state.pending_model_apply.take();
                    state.settings.apply_saved(Ok(()));
                    let refresh = commands::load_config(state);
                    match pending {
                        Some(PendingModelApply::InPlace { thread_id, model }) => {
                            // Same vendor: flip the model on the running
                            // thread through `thread/settings/update`; the
                            // conversation and its history stay put.
                            state.settings.notice = Some(format!("Model switched to {model}"));
                            refresh.chain(commands::apply_model_in_place(state, thread_id, model))
                        }
                        Some(PendingModelApply::Retarget { provider_id, model }) => {
                            refresh.chain(retarget_thread(state, provider_id, model, false))
                        }
                        None if menu_pick => {
                            // No live thread to retarget, or it already
                            // runs the picked pair: the write only records
                            // the new default.
                            state.settings.notice = Some(
                                "Saved; new sessions will use the new model setup".to_string(),
                            );
                            refresh
                        }
                        None if panel_edit => {
                            // The provider form can rewrite the endpoint/key
                            // table as well as the pair; both need the
                            // retarget path.
                            let (provider_id, model, table_rewritten) =
                                saved_panel_model_setup(state);
                            let retarget =
                                retarget_thread(state, provider_id, model, table_rewritten);
                            refresh.chain(retarget)
                        }
                        None => refresh,
                    }
                }
                Err(error) => {
                    state.model_switch_pending = false;
                    state.pending_model_apply = None;
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
            refocus_composer()
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
        Message::SkillsPageToggled => {
            if !state.skills.toggle_page() {
                return Task::none();
            }
            // One full-screen surface at a time: the settings panel yields.
            if state.settings.open {
                state.settings.toggle();
            }
            // A fresh look reloads the inventory; the market loads lazily
            // the first time its tab comes up.
            let market = if state.skills.tab == SkillsTab::Market && !state.skills.market.loaded {
                commands::load_market(state)
            } else {
                Task::none()
            };
            Task::batch([commands::reload_skills(state), market])
        }
        Message::SkillsTabPicked(tab) => {
            state.skills.tab = tab;
            if tab == SkillsTab::Market && !state.skills.market.loaded {
                commands::load_market(state)
            } else {
                Task::none()
            }
        }
        Message::SkillAddOpened => {
            state.skills.open_add();
            Task::none()
        }
        Message::SkillAddNameChanged(text) => {
            if let Some(add) = state.skills.add.as_mut() {
                add.name = text;
            }
            Task::none()
        }
        Message::SkillAddDescriptionChanged(text) => {
            if let Some(add) = state.skills.add.as_mut() {
                add.description = text;
            }
            Task::none()
        }
        Message::SkillAddLinkChanged(text) => {
            if let Some(add) = state.skills.add.as_mut() {
                add.link = text;
            }
            Task::none()
        }
        Message::SkillAddImportRequested(target) => commands::pick_skill_source(target),
        Message::SkillAddSourcePicked(picked) => {
            if let Some(add) = state.skills.add.as_mut() {
                add.source = picked;
            }
            Task::none()
        }
        Message::SkillAddSubmitted => {
            let Some(add) = state.skills.add.clone() else {
                return Task::none();
            };
            let link = add.link.trim().to_string();
            if !link.is_empty() {
                if let Some(form) = state.skills.add.as_mut() {
                    form.busy = true;
                }
                return commands::import_skill_from_link(state, link);
            }
            match add.source {
                Some(source) => commands::import_skill(state, source),
                None => commands::create_skill(
                    state,
                    add.name.trim().to_string(),
                    add.description.trim().to_string(),
                ),
            }
        }
        Message::SkillAddCancelled => {
            state.skills.add = None;
            Task::none()
        }
        Message::SkillEditOpened(name) => {
            state.skills.open_edit(&name);
            Task::none()
        }
        Message::SkillEditNameChanged(text) => {
            if let Some(editor) = state.skills.editor.as_mut() {
                editor.name_draft = text;
            }
            Task::none()
        }
        Message::SkillEditDescriptionChanged(text) => {
            if let Some(editor) = state.skills.editor.as_mut() {
                editor.description_draft = text;
            }
            Task::none()
        }
        Message::SkillEditSubmitted => {
            let Some(editor) = state.skills.editor.clone() else {
                return Task::none();
            };
            commands::edit_skill(
                editor.path,
                editor.name_draft.trim().to_string(),
                editor.description_draft.trim().to_string(),
            )
        }
        Message::SkillEditCancelled => {
            state.skills.editor = None;
            Task::none()
        }
        Message::SkillDeleteArmed(name) => {
            state.skills.confirm_delete = Some(name);
            Task::none()
        }
        Message::SkillDeleteCancelled => {
            state.skills.confirm_delete = None;
            Task::none()
        }
        Message::SkillDeleteConfirmed(name) => {
            state.skills.confirm_delete = None;
            let Some(row) = state.skills.rows.iter().find(|row| row.name == name) else {
                return Task::none();
            };
            match row.source() {
                SkillSource::Local => commands::delete_skill_files(row.path.clone(), name),
                // A plugin-shipped skill leaves by uninstalling its plugin.
                SkillSource::Plugin => match row.plugin_id.clone() {
                    Some(plugin_id) => commands::uninstall_plugin(state, plugin_id),
                    None => Task::none(),
                },
                SkillSource::System => {
                    state.skills.notice =
                        Some(SkillNotice::failed("系统级 skill 受保护，无法删除"));
                    Task::none()
                }
            }
        }
        Message::SkillMutationSettled(result) => match result {
            Ok(text) => {
                // Success closes the transient forms and re-reads the disk.
                state.skills.add = None;
                state.skills.editor = None;
                state.skills.notice = Some(SkillNotice::ok(text));
                commands::reload_skills(state)
            }
            Err(reason) => {
                // Failure keeps the form open so the draft is not lost.
                if let Some(form) = state.skills.add.as_mut() {
                    form.busy = false;
                }
                state.skills.notice = Some(SkillNotice::failed(format!("操作失败：{reason}")));
                Task::none()
            }
        },
        Message::SkillMarketLoaded(result) => match result {
            Ok(response) => {
                state.skills.market.apply_list(&response);
                Task::none()
            }
            Err(error) => {
                // Keep the tab usable: the refresh action retries.
                state.skills.market.loaded = true;
                state.skills.notice = Some(SkillNotice::failed(format!("市场加载失败：{error}")));
                Task::none()
            }
        },
        Message::SkillMarketRefreshRequested => commands::load_market(state),
        Message::SkillMarketPluginToggled(plugin_id) => {
            if state.skills.market.detail_loading.as_deref() == Some(plugin_id.as_str()) {
                // A second press while the detail loads cancels it.
                state.skills.market.detail_loading = None;
                return Task::none();
            }
            let already_open = state
                .skills
                .market
                .detail
                .as_ref()
                .is_some_and(|detail| detail.plugin_id == plugin_id);
            state.skills.market.toggle_detail(&plugin_id);
            if already_open {
                Task::none()
            } else {
                commands::load_market_detail(state, plugin_id)
            }
        }
        Message::SkillMarketDetailLoaded(result) => match result {
            Ok(response) => {
                state.skills.market.apply_detail(&response);
                Task::none()
            }
            Err(error) => {
                state.skills.market.detail_loading = None;
                state.skills.notice =
                    Some(SkillNotice::failed(format!("插件详情加载失败：{error}")));
                Task::none()
            }
        },
        Message::SkillMarketInstallRequested(plugin_id) => {
            state.skills.market.busy_plugin = Some(plugin_id.clone());
            state.skills.notice = None;
            commands::install_market_plugin(state, plugin_id)
        }
        Message::SkillUninstallRequested(plugin_id) => {
            state.skills.market.busy_plugin = Some(plugin_id.clone());
            commands::uninstall_plugin(state, plugin_id)
        }
        Message::SkillInstalled { id, result } => {
            state.skills.market.busy_plugin = None;
            match result {
                Ok(value) => {
                    state.skills.market.apply_installed(&id);
                    let hint = match value["authPolicy"].as_str() {
                        Some("ON_USE") => "；首次使用时会请求授权",
                        _ => "",
                    };
                    state.skills.notice = Some(SkillNotice::ok(format!("已安装 `{id}`{hint}")));
                    // The install brings its bundled skills onto the page.
                    commands::reload_skills(state)
                }
                Err(error) => {
                    state.skills.notice = Some(SkillNotice::failed(format!("安装失败：{error}")));
                    Task::none()
                }
            }
        }
        Message::SkillUninstalled { id, result } => {
            state.skills.market.busy_plugin = None;
            match result {
                Ok(_) => {
                    state.skills.market.apply_uninstalled(&id);
                    // The uninstall drops every skill the plugin shipped.
                    let removed = state.skills.remove_plugin(&id);
                    state.skills.notice = Some(SkillNotice::ok(format!(
                        "已卸载 `{id}`（移除 {removed} 个 skill）"
                    )));
                    commands::reload_skills(state)
                }
                Err(error) => {
                    state.skills.notice = Some(SkillNotice::failed(format!("卸载失败：{error}")));
                    Task::none()
                }
            }
        }
        Message::SkillMarketSourceChanged(text) => {
            state.skills.market.source_draft = text;
            Task::none()
        }
        Message::SkillMarketSourceSubmitted => {
            let source = state.skills.market.source_draft.trim().to_string();
            if source.is_empty() {
                state.skills.notice = Some(SkillNotice::failed("请先填写市场来源"));
                return Task::none();
            }
            commands::add_marketplace(state, source)
        }
        Message::SkillMarketSourceAdded(result) => match result {
            Ok(value) => {
                let name = value["marketplaceName"].as_str().unwrap_or("marketplace");
                let already = value["alreadyAdded"].as_bool().unwrap_or(false);
                state.skills.market.source_draft.clear();
                state.skills.notice = Some(SkillNotice::ok(if already {
                    format!("市场 `{name}` 已存在，正在刷新列表")
                } else {
                    format!("已添加市场 `{name}`")
                }));
                commands::load_market(state)
            }
            Err(error) => {
                state.skills.notice = Some(SkillNotice::failed(format!("添加市场失败：{error}")));
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
            // Opening the source-control pane selects every change so a
            // drafted message commits the whole tree unless the user
            // unchecks rows.
            if tab == SidebarTab::SourceControl
                && state.git_overlay.selected.is_empty()
                && state.git.change_count() > 0
            {
                state.git_overlay.selected = (0..state.git.change_count()).collect();
            }
            // The remote pane probes its facts on first open only.
            let probe = if tab == SidebarTab::Remote && !state.remote.loaded {
                state.remote.loaded = true;
                commands::probe_remote()
            } else {
                Task::none()
            };
            // Leaving the workspace search restores the full tree view.
            if tab != SidebarTab::Search {
                state.tree.clear_search();
            }
            probe
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
        Message::ReasoningToggled(id) => {
            // Reasoning cards render expanded by default; the set holds
            // the collapsed ones.
            if !state.collapsed_reasoning.remove(&id) {
                state.collapsed_reasoning.insert(id);
            }
            Task::none()
        }
        Message::McpCardToggled(id) => {
            // MCP cards render collapsed by default; the set holds the
            // expanded ones.
            if !state.expanded_mcp.remove(&id) {
                state.expanded_mcp.insert(id);
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
            state.mentions.close();
            Task::none()
        }
        Message::MentionSearchLoaded { query, result } => {
            // Stale pages resolve after the token moved on; only the live
            // query may repaint the popup.
            if state.mentions.is_open() && state.mentions.query() == query {
                match result {
                    Ok(response) => {
                        let hits = response
                            .files
                            .into_iter()
                            .map(|file| codex_gui_core::MentionHit {
                                path: file.path,
                                file_name: file.file_name,
                            })
                            .collect();
                        state.mentions.apply_hits(hits);
                    }
                    Err(error) => tracing::warn!(%error, "fuzzyFileSearch failed"),
                }
            }
            Task::none()
        }
        Message::MentionPicked(index) => {
            state.mentions.select(index);
            state.mentions.insert_selected(&mut state.composer);
            refocus_composer()
        }
        Message::MentionMoved(delta) => {
            state.mentions.move_selection(delta);
            Task::none()
        }
        Message::MentionClosed => {
            state.mentions.close();
            Task::none()
        }
        Message::SlashPicked(command) => {
            state.composer = command;
            Task::batch([submit_composer(state), refocus_composer()])
        }
        Message::GitRefreshRequested => state
            .status_board
            .cwd()
            .map(|cwd| commands::refresh_git(cwd.to_string()))
            .unwrap_or_else(Task::none),
        Message::SearchQueryChanged(query) => {
            if query.trim().is_empty() {
                state.tree.clear_search();
            } else {
                state.tree.set_search_query(query);
            }
            Task::none()
        }
        Message::SearchReplaceChanged(replacement) => {
            state.search_replace = replacement;
            Task::none()
        }
        Message::ExtensionFilterChanged(filter) => {
            state.extension_filter = filter;
            Task::none()
        }
        Message::RemoteInfoArrived {
            targets,
            docker_available,
        } => {
            state.remote.targets = targets;
            state.remote.docker_available = docker_available;
            Task::none()
        }
    }
}

/// Returns the keyboard to the composer input. iced blurs a text input the
/// moment a press lands outside its bounds, so clicks on picker rows or the
/// send/stop buttons would otherwise silently swallow everything typed next.
fn refocus_composer() -> Task<Message> {
    // Focusing the input also parks the caret at the end of the text.
    operation::focus(crate::view::COMPOSER_INPUT_ID)
}

/// The composer's Enter behavior: an open mention popup takes the key to
/// insert its highlighted file; a slash command dispatches to its
/// request; anything else submits as a turn.
fn submit_composer(state: &mut State) -> Task<Message> {
    if state.mentions.is_open() {
        state.mentions.insert_selected(&mut state.composer);
        return Task::none();
    }
    if let Some(action) = commands::slash_action(&state.composer) {
        // A slash command needs the same live-session posture as a
        // regular submit (the composed text is non-empty by definition).
        if !state.can_submit() {
            return Task::none();
        }
        state.composer.clear();
        return match action {
            commands::SlashAction::Review => commands::start_review(state),
            commands::SlashAction::Compact => commands::compact_thread(state),
        };
    }
    commands::submit(state)
}

/// Records the server-confirmed working directory: status bar, recent
/// projects, and a fresh git probe.
fn confirm_cwd(state: &mut State, cwd: String) -> Task<Message> {
    state.status_board.apply_cwd(cwd.clone());
    state.recents.remember(&cwd);
    state.recents.save();
    commands::refresh_git(cwd)
}

/// Answers every question of the head request at once and drops its
/// drafts; empty answers are legal and let the server auto-resolve.
fn answer_questions(state: &mut State) -> Task<Message> {
    let Some(pending) = state.questions.pending().first().cloned() else {
        return Task::none();
    };
    let payload = question_response_payload(&pending.params.questions, &state.question_drafts);
    let request_id = pending.request_id.clone();
    let _removed = state.questions.take(&request_id.to_string());
    state.question_drafts.clear();
    commands::respond_to_request(state, request_id, payload)
}

/// Answers the head elicitation: accept validates the form draft (when the
/// mode carries one), decline and cancel go out without content.
fn answer_elicitation(state: &mut State, action: McpServerElicitationAction) -> Task<Message> {
    let Some(pending) = state.elicitations.pending().first().cloned() else {
        return Task::none();
    };
    let content = if action == McpServerElicitationAction::Accept {
        match &pending.params.request {
            McpServerElicitationRequest::Form {
                requested_schema, ..
            } => match elicitation_form_content(requested_schema, &state.elicitation_drafts) {
                Ok(content) => Some(content),
                Err(error) => {
                    state.elicitation_error = Some(error);
                    return Task::none();
                }
            },
            McpServerElicitationRequest::UserVerification { .. }
            | McpServerElicitationRequest::OpenAiForm { .. }
            | McpServerElicitationRequest::OpenAiElicitationForm { .. }
            | McpServerElicitationRequest::Url { .. } => None,
        }
    } else {
        None
    };
    let payload = elicitation_response_payload(action, content);
    let request_id = pending.request_id.clone();
    let _removed = state.elicitations.take(&request_id.to_string());
    reset_elicitation_drafts(state);
    commands::respond_to_request(state, request_id, payload)
}

/// Re-seeds the elicitation drafts for the dialog that is on top after a
/// resolution: schema defaults for a form, nothing otherwise.
fn reset_elicitation_drafts(state: &mut State) {
    state.elicitation_error = None;
    state.elicitation_drafts = match state.elicitations.pending().first() {
        Some(pending) => elicitation_default_drafts(pending),
        None => HashMap::new(),
    };
}

/// Merges one `config/read` result into the vendor catalog: which
/// providers exist, which pair is current, then the live models.
fn apply_vendor_config(state: &mut State, value: &serde_json::Value) {
    let config = &value["config"];
    let providers = parse_configured_providers(config);
    state.vendors.apply_config(
        config["model"].as_str(),
        config["model_provider"].as_str(),
        &providers,
    );
    attach_vendor_catalog(state);
}

/// Projects the loaded model catalog onto the active vendor group.
fn attach_vendor_catalog(state: &mut State) {
    let entries: Vec<(String, String, String)> = state
        .status_board
        .models()
        .iter()
        .map(|model| (model.id.clone(), model.display_name.clone(), String::new()))
        .collect();
    state.vendors.attach_catalog(&entries);
}

/// The `[model_providers.*]` tables as vendor-catalog rows.
fn parse_configured_providers(
    config: &serde_json::Value,
) -> Vec<codex_gui_core::ConfiguredProvider> {
    let Some(table) = config["model_providers"].as_object() else {
        return Vec::new();
    };
    table
        .iter()
        .map(|(id, entry)| codex_gui_core::ConfiguredProvider {
            id: id.clone(),
            name: entry["name"].as_str().map(str::to_string),
            base_url: entry["base_url"].as_str().unwrap_or_default().to_string(),
        })
        .collect()
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
            // List reloads triggered by notifications ride this arm's
            // return value; everything else resolves synchronously.
            let mut refresh = Task::none();
            if let ServerNotification::ThreadArchived(archived) = &notification {
                state.sessions.remove(&archived.thread_id);
                if state.archived_open && state.archived_loaded {
                    // The row moved into the archived partition, which is
                    // on screen.
                    refresh = commands::load_archived_sessions(state);
                }
            }
            if let ServerNotification::ThreadUnarchived(unarchived) = &notification {
                state.archived.remove(&unarchived.thread_id);
                // The row moved back into the live partition; it only
                // needs a reload when the restore ran elsewhere (the
                // sidebar button refreshes eagerly).
                let known = state
                    .sessions
                    .threads()
                    .iter()
                    .any(|thread| thread.id == unarchived.thread_id);
                if !known {
                    refresh = Task::batch([refresh, commands::load_sessions(state)]);
                }
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
            if let ServerNotification::McpServerOauthLoginCompleted(completed) = &notification {
                // The banner tracks the server it was started for; it only
                // goes away once that login settles.
                if state
                    .mcp_login
                    .as_ref()
                    .is_some_and(|login| login.name == completed.name)
                {
                    state.mcp_login = None;
                }
                let notice = if completed.success {
                    format!("{} signed in", completed.name)
                } else {
                    format!(
                        "sign-in for {} failed: {}",
                        completed.name,
                        completed.error.as_deref().unwrap_or("unknown error")
                    )
                };
                state.settings.notice = Some(notice);
                if state.settings.open {
                    refresh = Task::batch([refresh, commands::load_mcp_servers(state)]);
                }
            }
            if let ServerNotification::McpServerStatusUpdated(_updated) = &notification
                && state.settings.open
            {
                // Startup or authentication transitions invalidate the
                // inventory rows while the panel is on screen.
                refresh = Task::batch([refresh, commands::load_mcp_servers(state)]);
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
            if let ServerNotification::TurnStarted(started) = &notification
                && Some(&started.thread_id) == state.thread_id.as_ref()
            {
                // Remember the live turn so the stop button can interrupt it.
                state.active_turn = Some(started.turn.id.clone());
            }
            if let ServerNotification::TurnCompleted(completed) = &notification
                && Some(&completed.thread_id) == state.thread_id.as_ref()
                && completed.turn.status != TurnStatus::InProgress
            {
                state.status = Status::Ready;
                state.active_turn = None;
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
                // keep the dialog queues in sync.
                let _removed = state.approvals.resolved(&resolved.request_id);
                let _removed = state.questions.resolved(&resolved.request_id);
                if state.elicitations.resolved(&resolved.request_id) {
                    reset_elicitation_drafts(state);
                }
                return Task::none();
            }
            state.transcript.apply(&notification);
            refresh
        }
        GuiEvent::ServerRequest(request) => {
            // Escalate to a dialog; the payload answer goes out once the
            // user decides. A request with no dialog kind gets a
            // method-not-found reply so it never dangles until timeout.
            if let Some(pending) = state.approvals.offered(&request) {
                tracing::info!(request = %pending.request_id, "approval requested");
                Task::none()
            } else if let Some(pending) = state.questions.offered(&request) {
                tracing::info!(request = %pending.request_id, "questions requested");
                // A first request opens with a clean slate; later ones
                // stack behind it and keep their own drafts.
                if state.questions.pending().len() == 1 {
                    state.question_drafts.clear();
                }
                Task::none()
            } else if let Some(pending) = state.elicitations.offered(&request) {
                tracing::info!(
                    request = %pending.request_id,
                    server = %pending.server_name(),
                    "elicitation requested"
                );
                if state.elicitations.pending().len() == 1 {
                    reset_elicitation_drafts(state);
                }
                Task::none()
            } else if let Some((request_id, payload)) =
                codex_gui_core::auto_decline_elicitation(&request)
            {
                // Raw OpenAI form modes have no renderable schema; decline
                // them so the asking server hears back instead of timing out.
                tracing::info!(request = %request_id, "elicitation auto-declined");
                commands::respond_to_request(state, request_id, payload)
            } else {
                commands::reject_server_request(state, &request)
            }
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
        Bootstrap::Initialized(Ok(value)) => {
            // The Skills page's disk mutations root at the server-reported
            // `codexHome`.
            state.codex_home = value["codexHome"].as_str().map(PathBuf::from);
            Task::batch([
                commands::start_thread(state),
                commands::load_sessions(state),
                commands::load_models(state),
                commands::load_account(state),
                // The status bar mirrors the configured model; load it eagerly
                // instead of waiting for the first settings-panel open.
                commands::load_config(state),
                // The skills inventory feeds the composer picker and toggles.
                commands::load_skills(state),
            ])
        }
        Bootstrap::Initialized(Err(error)) => {
            state.status = Status::Disconnected(error.to_string());
            Task::none()
        }
        Bootstrap::ThreadStarted(Ok(result)) => {
            state.thread_id = result["thread"]["id"].as_str().map(ToString::to_string);
            state.active_binding =
                match (result["modelProvider"].as_str(), result["model"].as_str()) {
                    (Some(provider), Some(model)) => {
                        Some((provider.to_string(), model.to_string()))
                    }
                    _ => None,
                };
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
    state.questions = codex_gui_core::Questions::default();
    state.question_drafts.clear();
    state.elicitations = codex_gui_core::Elicitations::default();
    state.elicitation_drafts.clear();
    state.elicitation_error = None;
    state.sessions = codex_gui_core::Sessions::default();
    state.archived = codex_gui_core::Sessions::default();
    state.archived_open = false;
    state.archived_loaded = false;
    state.skills = codex_gui_core::SkillsBoard::default();
    state.codex_home = None;
    state.attachments = crate::attachments::Attachments::default();
    state.active_turn = None;
    state.active_binding = None;
    state.pending_model_apply = None;
    state.mentions = codex_gui_core::Mentions::default();
    state.mcp_servers.clear();
    state.mcp_loaded = false;
    state.mcp_login = None;
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

/// The `(provider, model)` a just-saved panel edit resolves to, plus
/// whether it rewrote the provider table (an endpoint or key change only
/// a new thread can pick up). Mirrors the batch write: the last value
/// written per key wins.
fn saved_panel_model_setup(state: &State) -> (String, String, bool) {
    let mut provider_id = state.settings.provider_current.id.clone();
    let mut model = state.settings.current.model.clone();
    let mut table_rewritten = false;
    for (key, value) in state.settings.edits() {
        match key.as_str() {
            "model_provider" => {
                provider_id = value.as_str().unwrap_or_default().to_string();
                table_rewritten = true;
            }
            "model" => model = value.as_str().unwrap_or_default().to_string(),
            key if key.starts_with("model_providers") => table_rewritten = true,
            _other => {}
        }
    }
    (provider_id, model, table_rewritten)
}

/// Retargets the running thread onto a freshly written model setup so the
/// conversation keeps its history: same-vendor model flips go through
/// `thread/settings/update`, vendor or provider-table changes fork the
/// thread (`thread/fork` carries the history), and a thread without
/// visible history simply reopens on the new setup.
fn retarget_thread(
    state: &mut State,
    provider_id: String,
    model: String,
    table_rewritten: bool,
) -> Task<Message> {
    let same_vendor = !table_rewritten
        && state
            .active_binding
            .as_ref()
            .is_some_and(|(provider, _)| provider == &provider_id);
    let Some(thread_id) = state.thread_id.clone() else {
        // No live thread: the write only records the new default.
        state.settings.notice =
            Some("Saved; new sessions will use the new model setup".to_string());
        return Task::none();
    };
    if same_vendor {
        state.settings.notice = Some(format!("Model switched to {model}"));
        return commands::apply_model_in_place(state, thread_id, model);
    }
    if state.transcript.entries().is_empty() {
        // Nothing on screen to carry: a fresh thread is equivalent.
        state.settings.notice =
            Some("Saved; fresh session started with the new model setup".to_string());
        return commands::restart_thread(state);
    }
    state.settings.notice = Some(format!("Saved; conversation moved to {model}"));
    commands::fork_thread_with_model(state, thread_id, provider_id, model)
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
