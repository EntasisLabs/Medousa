use super::daemon_commands::{handle_daemon_command, handle_watch_command};
use super::slash_command_artifact_services;
use super::slash_command_services;
use super::slash_command_stage_services;
use super::*;

pub(crate) async fn handle_slash_command(
    prompt: &str,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    let mut parts = prompt.split_whitespace();
    let cmd = parts.next().unwrap_or_default();

    match cmd {
        "/new" => {
            slash_command_services::handle_new_session_command(state, tui_rt, event_tx).await;
        }
        "/name" => {
            let label = parts.collect::<Vec<_>>().join(" ").trim().to_string();
            if label.is_empty() {
                session_name_services::refresh_session_display_name(state);
                match &state.session_display_name {
                    Some(name) => push_obs(
                        state,
                        format!(
                            "◈ session name: {} ({})",
                            name,
                            &state.session_id[..state.session_id.len().min(8)]
                        ),
                    ),
                    None => push_obs(
                        state,
                        format!(
                            "◈ no display name for session {}",
                            &state.session_id[..state.session_id.len().min(8)]
                        ),
                    ),
                }
            } else {
                match session_name_services::set_session_display_name_daemon_first(state, &label)
                    .await
                {
                    Ok(()) => push_obs(
                        state,
                        format!(
                            "✓ session name set to \"{}\" (global)",
                            state
                                .session_display_name
                                .as_deref()
                                .unwrap_or(label.as_str())
                        ),
                    ),
                    Err(err) => push_obs(state, format!("⚠ could not set session name: {err}")),
                }
            }
        }
        "/history" => {
            state.history_items = history_services::list_history_sessions_daemon_first(state, 200).await;
            state.history_selected = 0;
            state.history_scroll = 0;
            state.history_max_scroll = 0;
            state.history_show_verification_detail = false;
            state.mode = UiMode::History;
        }
        "/settings" => {
            state.mode = UiMode::Settings;
            state.settings_tab = 0;
            state.settings_selected = 0;
            state.settings_editing = false;
            state.settings_scroll = 0;
            state.settings_max_scroll = 0;
            state.routing_editor_role_idx = 0;
            state.settings_draft = state.settings.clone();
            state.stage_routing_draft = state.stage_routing.clone();
        }
        "/themes" => {
            open_theme_menu(state, UiMode::Chat);
        }
        "/theme" => {
            let requested = parts.collect::<Vec<_>>().join(" ").trim().to_string();
            if requested.is_empty() {
                push_obs(
                    state,
                    format!(
                        "◈ current theme: {} ({}) | available: {}",
                        ui_theme_display_name(&state.settings.theme_id),
                        state.settings.theme_id,
                        ui_theme_ids().join(", ")
                    ),
                );
            } else if let Some(theme_id) = ui_theme_ids()
                .iter()
                .find(|id| id.eq_ignore_ascii_case(&requested))
            {
                let selected = (*theme_id).to_string();
                state.settings.theme_id = selected.clone();
                state.settings_draft.theme_id = selected.clone();

                let mut defaults = load_tui_defaults();
                defaults.theme_id = Some(selected.clone());
                save_tui_defaults(&defaults);

                push_obs(
                    state,
                    format!(
                        "✓ theme applied: {} ({selected})",
                        ui_theme_display_name(&selected)
                    ),
                );
            } else {
                push_obs(
                    state,
                    format!(
                        "⚠ unknown theme '{}'. available: {}",
                        requested,
                        ui_theme_ids().join(", ")
                    ),
                );
            }
        }
        "/allowlist-preview" => {
            state.mode = UiMode::AllowlistPreview;
            state.allowlist_preview_source = parts.collect::<Vec<_>>().join(" ");
            if state.allowlist_preview_source.trim().is_empty() {
                state.allowlist_preview_source =
                    "query Run { websearch.search(query: \"\") { ok } }".to_string();
            }
        }
        "/edit" | "/open" => {
            let path_raw = parts.collect::<Vec<_>>().join(" ");
            if path_raw.trim().is_empty() {
                state.mode = UiMode::Editor;
                state.editor_status =
                    "Editor opened. Use /open <path> or /save <path> to persist.".to_string();
                state.editor_preferred_col = None;
                keep_editor_cursor_visible(state, 12);
            } else {
                let path = PathBuf::from(path_raw.trim());
                match load_editor_file(&path) {
                    Ok(Some(content)) => {
                        state.editor_buffer = TextBuffer::from_text(content);
                        state.editor_file_path = Some(path.clone());
                        state.editor_status = format!("Opened {}", path.display());
                        state.editor_dirty = false;
                        state.editor_preferred_col = None;
                        state.editor_scroll = 0;
                        keep_editor_cursor_visible(state, 12);
                        state.mode = UiMode::Editor;
                    }
                    Ok(None) => {
                        state.editor_buffer = TextBuffer::default();
                        state.editor_file_path = Some(path.clone());
                        state.editor_status =
                            format!("New file {} (not saved yet)", path.display());
                        state.editor_dirty = false;
                        state.editor_preferred_col = None;
                        state.editor_scroll = 0;
                        state.mode = UiMode::Editor;
                    }
                    Err(err) => {
                        push_obs(state, format!("⚠ open failed: {err}"));
                    }
                }
            }
        }
        "/artifact"
        | "/artifact-chunks"
        | "/artifact-list"
        | "/artifact-maintain"
        | "/artifact-extract"
        | "/artifact-extractions"
        | "/artifact-pack"
        | "/artifact-packs"
        | "/artifact-pack-use"
        | "/artifact-pack-auto"
        | "/artifact-verify"
        | "/artifact-verifications"
        | "/artifact-verification" => {
            let args = parts.collect::<Vec<_>>();
            return slash_command_artifact_services::handle_artifact_family_command(
                cmd, args, state,
            )
            .await;
        }
        "/verify-policy" => {
            let args = parts.collect::<Vec<_>>();
            return slash_command_artifact_services::handle_verify_policy_command(
                args, state, tui_rt, event_tx,
            )
            .await;
        }
        "/stage-routes" | "/stage-route-set" | "/stage-route-reset" => {
            let args = parts.collect::<Vec<_>>();
            return slash_command_stage_services::handle_stage_route_family_command(cmd, args, state)
                .await;
        }
        "/save" => {
            let path_raw = parts.collect::<Vec<_>>().join(" ");
            save_editor_buffer(state, Some(path_raw.as_str()));
        }
        "/run" => {
            let path_raw = parts.collect::<Vec<_>>().join(" ");
            let override_path = if path_raw.trim().is_empty() {
                None
            } else {
                Some(path_raw.as_str())
            };
            run_editor_source_via_runtime(state, tui_rt, event_tx, override_path).await;
        }
        "/run-current" => {
            let Some(path) = state.editor_file_path.clone() else {
                push_obs(
                    state,
                    "⚠ run-current failed: no editor file path set. use /open <path> or /run <path>"
                        .to_string(),
                );
                return EventOutcome::Continue;
            };

            let path_value = path.display().to_string();
            run_editor_source_via_runtime(state, tui_rt, event_tx, Some(path_value.as_str())).await;
        }
        "/close" => {
            push_obs(state, "✓ closing medousa_tui".to_string());
            return EventOutcome::Break;
        }
        "/clear-key" => {
            state.settings.api_key.clear();
            state.settings_draft.api_key.clear();
            save_tui_api_key(None);
            push_obs(state, "✓ api key cleared from secure storage".to_string());
        }
        "/rotate-key" => {
            let key = state.settings_draft.api_key.trim().to_string();
            if key.is_empty() {
                push_obs(
                    state,
                    "⚠ key rotation requires a non-empty draft API key".to_string(),
                );
                return EventOutcome::Continue;
            }

            save_tui_api_key(Some(&key));
            state.settings.api_key = key.clone();
            state.settings_draft.api_key = key;
            push_obs(state, "✓ api key rotated in secure storage".to_string());
        }
        "/model" => {
            let args = parts.collect::<Vec<_>>();
            return slash_command_services::handle_model_command(args, state, tui_rt, event_tx)
                .await;
        }
        "/depth" => {
            let mode = parts.next();
            return slash_command_services::handle_depth_command(mode, state, tui_rt, event_tx)
                .await;
        }
        "/stop" => {
            stop_active_generation(state);
        }
        "/regen" => {
            if state.is_processing {
                push_obs(state, "⚠ cannot regenerate while processing".to_string());
                return EventOutcome::Continue;
            }

            let last_user_prompt = state
                .conversation
                .iter()
                .rev()
                .find(|t| t.role == "user")
                .map(|t| t.content.clone());

            if let Some(prompt) = last_user_prompt {
                if matches!(state.conversation.last(), Some(turn) if turn.role == "agent") {
                    state.conversation.pop();
                }
                push_obs(state, "↻ regenerate last response".to_string());
                start_prompt_run(state, tui_rt, event_tx, prompt, false).await;
            } else {
                push_obs(
                    state,
                    "⚠ no user prompt available to regenerate".to_string(),
                );
            }
        }
        "/export" => {
            let format = parts.next().unwrap_or("md");
            match export_current_session(state, format) {
                Ok(path) => push_obs(state, format!("✓ exported {}", path.display())),
                Err(err) => push_obs(state, format!("⚠ export failed: {err}")),
            }
        }
        "/perf" => {
            let sub = parts.next().unwrap_or("report");
            let trailing_args = parts.collect::<Vec<_>>();
            return slash_command_services::handle_perf_command(sub, &trailing_args, state);
        }
        "/daemon" => {
            return handle_daemon_command(&mut parts, state);
        }
        "/watch" => {
            return handle_watch_command(&mut parts, state);
        }
        "/skills" => {
            match medousa::skill_ingest::format_skill_manuscripts_list() {
                Ok(list) => push_obs(state, list),
                Err(err) => push_obs(state, format!("⚠ could not list skills: {err:#}")),
            }
        }
        "/skill" => {
            let args = parts.collect::<Vec<_>>().join(" ");
            match medousa::skill_ingest::parse_skill_command_args(&args)
                .and_then(|parsed| medousa::skill_ingest::build_skill_run_ingest_prompt(&parsed))
            {
                Ok(prompt) => {
                    push_obs(
                        state,
                        format!("◈ skill run queued via research worker"),
                    );
                    start_prompt_run(state, tui_rt, event_tx, prompt, true).await;
                }
                Err(err) => push_obs(state, format!("⚠ skill run failed: {err:#}")),
            }
        }
        _ => {
            push_obs(
                state,
                "⚠ unknown command. try /new /history /settings /skills /skill /edit /open /save /run /run-current /artifact /artifact-chunks /artifact-list /artifact-maintain /artifact-extract /artifact-extractions /artifact-pack /artifact-packs /artifact-pack-use /artifact-pack-auto /artifact-verify /artifact-verifications /artifact-verification /verify-policy /stage-routes /stage-route-set /stage-route-reset /close /allowlist-preview /clear-key /rotate-key /model /depth /stop /regen /export /perf /daemon /watch"
                    .to_string(),
            );
        }
    }

    EventOutcome::Continue
}
