use super::daemon_commands::{handle_daemon_command, handle_watch_command};
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
            stop_active_generation(state);
            state.session_id = Uuid::new_v4().simple().to_string();
            state.selected_context_pack_query = None;
            state.conversation.clear();
            invalidate_markdown_cache(state);
            state.active_agent_stream_turn = None;
            state.thinking_trace.clear();
            state.thinking_scroll = 0;
            state.thinking_max_scroll = 0;
            state.in_thinking_tag = false;
            state.stream_tag_tail.clear();
            state.is_processing = false;
            state.open_stream_turn_id = None;
            state.pending_agent_chunk_delta.clear();
            state.pending_agent_chunk_count = 0;
            state.auto_scroll = true;
            state.conv_scroll = 0;
            save_last_session_id(&state.session_id);
            push_obs(state, format!("✓ new session {}", &state.session_id[..8]));

            if let Ok(new_rt) = build_tui_runtime(
                parse_backend(Some(&state.settings.backend)),
                Some(&state.settings.provider),
                Some(&state.settings.model),
                if state.settings.base_url.trim().is_empty() {
                    None
                } else {
                    Some(state.settings.base_url.as_str())
                },
                parse_allowed_modules(&state.settings.allowed_modules),
                &state.session_id,
                event_tx.clone(),
            )
            .await
            {
                *tui_rt = new_rt;
            } else {
                push_obs(state, "⚠ new session runtime rebind failed".to_string());
            }
        }
        "/history" => {
            state.history_items = list_history_sessions(200);
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
        "/artifact" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            match medousa::artifact_store::find_artifact(
                &state.session_id,
                if query.trim().is_empty() {
                    None
                } else {
                    Some(query.trim())
                },
            ) {
                Some(found) => {
                    let payload = serde_json::to_string_pretty(&found.payload)
                        .unwrap_or_else(|_| found.payload.to_string());
                    let preview = payload.chars().take(600).collect::<String>();
                    push_obs(
                        state,
                        format!(
                            "◈ artifact lookup {} tool={} dir={} bytes={}\n{}{}",
                            found.record.artifact_id,
                            found.record.tool_name,
                            found.record.direction,
                            found.record.byte_size,
                            preview,
                            if payload.chars().count() > 600 {
                                "\n..."
                            } else {
                                ""
                            }
                        ),
                    );
                }
                None => {
                    push_obs(
                        state,
                        "⚠ artifact lookup found no match in this session".to_string(),
                    );
                }
            }
        }
        "/artifact-chunks" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            match medousa::artifact_store::find_artifact(
                &state.session_id,
                if query.trim().is_empty() {
                    None
                } else {
                    Some(query.trim())
                },
            ) {
                Some(found) => {
                    let refs = medousa::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );
                    let refs_preview = serde_json::to_string_pretty(
                        &refs.iter().take(8).cloned().collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "[]".to_string());
                    push_obs(
                        state,
                        format!(
                            "◈ artifact chunks {} total={}\n{}",
                            found.record.artifact_id,
                            refs.len(),
                            refs_preview
                        ),
                    );
                }
                None => {
                    push_obs(
                        state,
                        "⚠ artifact chunking found no match in this session".to_string(),
                    );
                }
            }
        }
        "/artifact-list" => {
            let args = parts.collect::<Vec<_>>();
            let limit = args
                .first()
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(20)
                .max(1)
                .min(200);
            let records = medousa::artifact_store::list_artifact_records(&state.session_id, limit);
            let stats = medousa::artifact_store::artifact_index_stats(&state.session_id);

            if records.is_empty() {
                push_obs(
                    state,
                    "◈ artifact list empty for current session".to_string(),
                );
            } else {
                let mut out = format!(
                    "◈ artifact list count={} unique={} bytes={}\n",
                    stats.records, stats.unique_hashes, stats.total_bytes
                );
                for record in records {
                    out.push_str(&format!(
                        "{}  {}  {}  {} bytes  {}\n",
                        record.artifact_id,
                        record.tool_name,
                        record.direction,
                        record.byte_size,
                        record.stored_at_utc.to_rfc3339()
                    ));
                }
                push_obs(state, out.trim_end().to_string());
            }
        }
        "/artifact-maintain" => {
            let args = parts.collect::<Vec<_>>();
            let max_per_session = args
                .first()
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(200)
                .max(1)
                .min(10_000);
            let max_age_days = args
                .get(1)
                .and_then(|raw| raw.parse::<i64>().ok())
                .unwrap_or(14)
                .max(1)
                .min(3650);

            match medousa::artifact_store::run_artifact_maintenance(max_per_session, max_age_days) {
                Ok(report) => push_obs(
                    state,
                    format!(
                        "◈ artifact maintenance before={} after={} missing_pruned={} deduped_pruned={} retention_pruned={} files_deleted={}",
                        report.records_before,
                        report.records_after,
                        report.missing_payload_pruned,
                        report.deduped_records_pruned,
                        report.retention_pruned,
                        report.payload_files_deleted
                    ),
                ),
                Err(err) => push_obs(state, format!("⚠ artifact maintenance failed: {err}")),
            }
        }
        "/artifact-extract" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            match medousa::artifact_store::find_artifact(
                &state.session_id,
                if query.trim().is_empty() {
                    None
                } else {
                    Some(query.trim())
                },
            ) {
                Some(found) => {
                    let chunk_refs = medousa::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );
                    let claims = medousa::artifact_extraction::extract_claims_from_chunks(
                        &found.record.artifact_id,
                        &found.payload,
                        &chunk_refs,
                    );
                    match medousa::artifact_extraction::persist_extraction_run(
                        &state.session_id,
                        &found.record.artifact_id,
                        &claims,
                    ) {
                        Ok(record) => {
                            let preview = serde_json::to_string_pretty(
                                &claims.iter().take(8).cloned().collect::<Vec<_>>(),
                            )
                            .unwrap_or_else(|_| "[]".to_string());
                            push_obs(
                                state,
                                format!(
                                    "◈ extraction {} artifact={} claims={}\n{}",
                                    record.extraction_id,
                                    record.artifact_id,
                                    record.claim_count,
                                    preview
                                ),
                            );
                        }
                        Err(err) => push_obs(state, format!("⚠ extraction persist failed: {err}")),
                    }
                }
                None => push_obs(
                    state,
                    "⚠ extraction failed: no artifact found in this session".to_string(),
                ),
            }
        }
        "/artifact-extractions" => {
            let args = parts.collect::<Vec<_>>();
            let limit = args
                .first()
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(20)
                .max(1)
                .min(200);
            let runs = medousa::artifact_extraction::list_extraction_runs(&state.session_id, limit);
            if runs.is_empty() {
                push_obs(
                    state,
                    "◈ extraction list empty for current session".to_string(),
                );
            } else {
                let mut out = format!("◈ extraction runs {}\n", runs.len());
                for run in runs {
                    out.push_str(&format!(
                        "{}  artifact={}  claims={}  {}\n",
                        run.extraction_id,
                        run.artifact_id,
                        run.claim_count,
                        run.created_at_utc.to_rfc3339()
                    ));
                }
                push_obs(state, out.trim_end().to_string());
            }
        }
        "/artifact-pack" => {
            let args = parts.collect::<Vec<_>>();
            let artifact_query = args.first().copied().unwrap_or("last");
            let max_tokens = args
                .get(1)
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(3200)
                .max(256)
                .min(200_000);
            let max_claims = args
                .get(2)
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(6)
                .max(1)
                .min(64);
            let max_chunks = args
                .get(3)
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(12)
                .max(1)
                .min(512);

            match medousa::artifact_store::find_artifact(&state.session_id, Some(artifact_query)) {
                Some(found) => {
                    let chunk_refs = medousa::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );

                    let extraction = medousa::artifact_extraction::find_extraction(
                        &state.session_id,
                        Some(&found.record.artifact_id),
                    );
                    let claims = extraction.map(|run| run.claims).unwrap_or_else(|| {
                        medousa::artifact_extraction::extract_claims_from_chunks(
                            &found.record.artifact_id,
                            &found.payload,
                            &chunk_refs,
                        )
                    });

                    let pack = medousa::context_pack::build_context_pack(
                        medousa::context_pack::BuildContextPackInput {
                            session_id: state.session_id.clone(),
                            artifact_id: found.record.artifact_id.clone(),
                            claims,
                            chunk_refs,
                            budget_profile: medousa::context_pack::ContextPackBudgetProfile {
                                max_tokens,
                                max_claims,
                                max_chunks,
                            },
                        },
                    );

                    match medousa::context_pack::persist_context_pack(&pack) {
                        Ok(()) => {
                            let preview = serde_json::to_string_pretty(&pack)
                                .unwrap_or_else(|_| "{}".to_string());
                            let preview = preview.chars().take(800).collect::<String>();
                            push_obs(
                                state,
                                format!(
                                    "◈ context pack {} artifact={} tokens={} claims={} chunks={}\n{}{}",
                                    pack.pack_id,
                                    pack.artifact_id,
                                    pack.total_token_estimate,
                                    pack.selected_claims.len(),
                                    pack.selected_chunk_refs.len(),
                                    preview,
                                    if preview.chars().count() >= 800 {
                                        "\n..."
                                    } else {
                                        ""
                                    }
                                ),
                            );
                        }
                        Err(err) => {
                            push_obs(state, format!("⚠ context pack persist failed: {err}"))
                        }
                    }
                }
                None => push_obs(
                    state,
                    "⚠ context pack failed: no artifact found in this session".to_string(),
                ),
            }
        }
        "/artifact-packs" => {
            let args = parts.collect::<Vec<_>>();
            let limit = args
                .first()
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(20)
                .max(1)
                .min(200);
            let packs = medousa::context_pack::list_context_packs(&state.session_id, limit);
            if packs.is_empty() {
                push_obs(
                    state,
                    "◈ context pack list empty for current session".to_string(),
                );
            } else {
                let mut out = format!("◈ context packs {}\n", packs.len());
                for pack in packs {
                    out.push_str(&format!(
                        "{}  artifact={}  tokens={}  {}\n",
                        pack.pack_id,
                        pack.artifact_id,
                        pack.total_token_estimate,
                        pack.created_at_utc.to_rfc3339()
                    ));
                }
                push_obs(state, out.trim_end().to_string());
            }
        }
        "/artifact-pack-use" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            let query = query.trim();
            if query.is_empty() {
                let mode = state
                    .selected_context_pack_query
                    .as_deref()
                    .unwrap_or("last");
                push_obs(state, format!("◈ context pack selector {mode}"));
            } else {
                match medousa::context_pack::find_context_pack(&state.session_id, Some(query)) {
                    Some(pack) => {
                        state.selected_context_pack_query = Some(query.to_string());
                        push_obs(
                            state,
                            format!(
                                "◈ context pack selector set query={} pack={} artifact={}",
                                query, pack.pack_id, pack.artifact_id
                            ),
                        );
                    }
                    None => push_obs(
                        state,
                        format!(
                            "⚠ context pack selector not set: no pack matched '{}' in this session",
                            query
                        ),
                    ),
                }
            }
        }
        "/artifact-pack-auto" => {
            state.selected_context_pack_query = None;
            push_obs(state, "◈ context pack selector set to last".to_string());
        }
        "/artifact-verify" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            let query = query.trim();
            let selector = if query.is_empty() {
                state
                    .selected_context_pack_query
                    .as_deref()
                    .unwrap_or("last")
            } else {
                query
            };

            match medousa::context_pack::find_context_pack(&state.session_id, Some(selector)) {
                Some(pack) => {
                    let verifier_route = state.stage_routing.get("verifier").cloned();
                    let policy = super::agent_runtime::verifier_policy_from_settings_and_route(
                        &state.settings,
                        verifier_route.as_ref(),
                    );
                    let report = medousa::verifier::verify_context_pack(&pack, &policy);
                    let verification_id = medousa::verification_store::persist_verification(
                        &state.session_id,
                        selector,
                        "slash_verify",
                        &policy,
                        &report,
                    )
                    .ok()
                    .map(|record| record.verification_id)
                    .unwrap_or_else(|| "(not persisted)".to_string());
                    let unsupported = if report.unsupported_claim_ids.is_empty() {
                        "none".to_string()
                    } else {
                        report
                            .unsupported_claim_ids
                            .iter()
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    push_obs(
                        state,
                        format!(
                            "◈ verification id={} pack={} selector={} artifact={} verified={} confidence={:.2} coverage={:.2} avg_support={:.2} support_ratio={:.2} supported={}/{} unsupported={} route={}",
                            verification_id,
                            report.pack_id,
                            selector,
                            report.artifact_id,
                            report.is_verified,
                            report.confidence_score,
                            report.citation_coverage,
                            report.avg_support_strength,
                            report.supported_claim_ratio,
                            report.supported_claims,
                            report.total_claims,
                            unsupported,
                            verifier_route
                                .as_ref()
                                .map(|route| format!(
                                    "{}:{} policy={}",
                                    route.provider, route.model, route.policy_profile
                                ))
                                .unwrap_or_else(|| "default".to_string()),
                        ),
                    );
                }
                None => push_obs(
                    state,
                    format!(
                        "⚠ verification failed: no context pack matched '{}' in this session",
                        selector
                    ),
                ),
            }
        }
        "/artifact-verifications" => {
            let args = parts.collect::<Vec<_>>();
            let limit = args
                .first()
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(20)
                .max(1)
                .min(200);
            let records = medousa::verification_store::list_verifications(&state.session_id, limit);
            if records.is_empty() {
                push_obs(
                    state,
                    "◈ verification history empty for current session".to_string(),
                );
            } else {
                let mut out = format!("◈ verification history {}\n", records.len());
                for record in records {
                    out.push_str(&format!(
                        "{}  pack={}  verified={}  confidence={:.2}  source={}  {}\n",
                        record.verification_id,
                        record.pack_id,
                        record.is_verified,
                        record.confidence_score,
                        record.source,
                        record.created_at_utc.to_rfc3339(),
                    ));
                }
                push_obs(state, out.trim_end().to_string());
            }
        }
        "/artifact-verification" => {
            let query = parts.collect::<Vec<_>>().join(" ");
            let query = if query.trim().is_empty() {
                None
            } else {
                Some(query.trim())
            };
            match medousa::verification_store::find_verification(&state.session_id, query) {
                Some(run) => {
                    let preview =
                        serde_json::to_string_pretty(&run).unwrap_or_else(|_| "{}".to_string());
                    let preview = preview.chars().take(1200).collect::<String>();
                    push_obs(
                        state,
                        format!(
                            "◈ verification detail {}\n{}{}",
                            run.record.verification_id,
                            preview,
                            if preview.chars().count() >= 1200 {
                                "\n..."
                            } else {
                                ""
                            }
                        ),
                    );
                }
                None => push_obs(
                    state,
                    "⚠ verification lookup found no match in this session".to_string(),
                ),
            }
        }
        "/verify-policy" => {
            let args = parts.collect::<Vec<_>>();
            if args.is_empty() {
                push_obs(
                    state,
                    format!(
                        "◈ verify policy citation={} avg_support={} supported_ratio={} claim_support={}",
                        state.settings.verifier_min_citation_coverage,
                        state.settings.verifier_min_avg_support_strength,
                        state.settings.verifier_min_supported_claim_ratio,
                        state.settings.verifier_min_claim_support_strength,
                    ),
                );
                return EventOutcome::Continue;
            }

            if args.len() != 4 {
                push_obs(
                    state,
                    "⚠ usage: /verify-policy <min_citation_coverage> <min_avg_support_strength> <min_supported_claim_ratio> <min_claim_support_strength>"
                        .to_string(),
                );
                return EventOutcome::Continue;
            }

            let normalize = |raw: &str, default: f32| -> String {
                let parsed = super::parse_f32_with_bounds(raw, default, 0.0, 1.0);
                format!("{parsed:.2}")
            };

            state.settings_draft.verifier_min_citation_coverage = normalize(args[0], 0.60);
            state.settings_draft.verifier_min_avg_support_strength = normalize(args[1], 0.70);
            state.settings_draft.verifier_min_supported_claim_ratio = normalize(args[2], 0.60);
            state.settings_draft.verifier_min_claim_support_strength = normalize(args[3], 0.65);

            apply_settings(state, tui_rt, event_tx).await;
        }
        "/stage-routes" => {
            let role = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(role) = role {
                match state.stage_routing.get(role) {
                    Some(route) => {
                        let rendered = serde_json::to_string_pretty(route)
                            .unwrap_or_else(|_| "{}".to_string());
                        push_obs(state, format!("◈ stage route {}\n{}", route.role, rendered));
                    }
                    None => push_obs(
                        state,
                        format!(
                            "⚠ unknown stage role '{}'. roles={}",
                            role,
                            medousa::stage_routing::StageRoutingMatrix::roles().join(",")
                        ),
                    ),
                }
            } else {
                let rendered = serde_json::to_string_pretty(&state.stage_routing)
                    .unwrap_or_else(|_| "{}".to_string());
                push_obs(state, format!("◈ stage routing matrix\n{}", rendered));
            }
        }
        "/stage-route-set" => {
            let args = parts.collect::<Vec<_>>();
            if args.len() < 2 {
                push_obs(
                    state,
                    "⚠ usage: /stage-route-set <role> <provider:model|model> [policy_profile] [fallback_csv]"
                        .to_string(),
                );
                return EventOutcome::Continue;
            }

            let role = args[0].trim();
            let (route_role, route_provider, route_model, route_policy, route_fallback) = {
                let Some(route) = state.stage_routing.get_mut(role) else {
                    push_obs(
                        state,
                        format!(
                            "⚠ unknown stage role '{}'. roles={}",
                            role,
                            medousa::stage_routing::StageRoutingMatrix::roles().join(",")
                        ),
                    );
                    return EventOutcome::Continue;
                };

                let target = args[1].trim();
                if let Some((provider, model)) = target.split_once(':') {
                    route.provider = provider.trim().to_string();
                    route.model = model.trim().to_string();
                } else {
                    route.model = target.to_string();
                }
                if let Some(policy) = args.get(2) {
                    route.policy_profile = policy.trim().to_string();
                }
                if let Some(fallback_csv) = args.get(3) {
                    route.fallback_chain = fallback_csv
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                }

                (
                    route.role.clone(),
                    route.provider.clone(),
                    route.model.clone(),
                    route.policy_profile.clone(),
                    route.fallback_chain.join(","),
                )
            };

            persist_stage_routing_defaults(state);
            push_obs(
                state,
                format!(
                    "◈ stage route updated role={} target={}:{} policy={} fallback={}",
                    route_role, route_provider, route_model, route_policy, route_fallback,
                ),
            );
        }
        "/stage-route-reset" => {
            state.stage_routing = medousa::stage_routing::StageRoutingMatrix::default_for(
                &state.settings.provider,
                &state.settings.model,
            );
            persist_stage_routing_defaults(state);
            push_obs(
                state,
                format!(
                    "◈ stage routing reset to provider={} model={} defaults",
                    state.settings.provider, state.settings.model
                ),
            );
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
            if args.is_empty() {
                push_obs(
                    state,
                    format!("model {}:{}", state.settings.provider, state.settings.model),
                );
                return EventOutcome::Continue;
            }

            let mut draft = state.settings_draft.clone();
            if args.len() == 1 {
                if let Some((provider, model)) = args[0].split_once(':') {
                    draft.provider = provider.trim().to_string();
                    draft.model = model.trim().to_string();
                } else {
                    draft.model = args[0].trim().to_string();
                }
            } else {
                draft.provider = args[0].trim().to_string();
                draft.model = args[1].trim().to_string();
            }

            state.settings_draft = draft;

            apply_settings(state, tui_rt, event_tx).await;
        }
        "/depth" => {
            let mode = parts.next();
            if mode.is_none() {
                let hint = depth_mode_hint(&state.response_depth_mode);
                push_obs(
                    state,
                    format!(
                        "◈ response depth mode={} ({hint}) options: concise | standard | deep",
                        state.response_depth_mode,
                    ),
                );
                return EventOutcome::Continue;
            }

            let normalized = super::normalize_response_depth_mode(mode.unwrap_or("standard"));
            state.response_depth_mode = normalized.clone();
            persist_response_depth_defaults(state);
            let hint = depth_mode_hint(&normalized);
            push_obs(
                state,
                format!("✓ response depth mode set to {} ({hint})", normalized),
            );
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
                start_prompt_run(state, tui_rt, event_tx, prompt, false);
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
            match sub {
                "baseline" => {
                    let label = parts.collect::<Vec<_>>().join(" ");
                    let label = if label.trim().is_empty() {
                        "baseline".to_string()
                    } else {
                        label.trim().to_string()
                    };
                    let snapshot = capture_perf_snapshot(state, label.clone());
                    state.perf_baseline = Some(snapshot.clone());
                    push_obs(
                        state,
                        format!("✓ perf baseline set: {}", format_perf_snapshot(&snapshot)),
                    );
                }
                "reset" => {
                    state.perf = UiPerfStats::default();
                    state.perf_baseline = None;
                    push_obs(state, "✓ perf counters and baseline reset".to_string());
                }
                _ => {
                    let label = if sub == "report" {
                        "report".to_string()
                    } else {
                        sub.to_string()
                    };
                    let current = capture_perf_snapshot(state, label);
                    let mut line = format!("perf {}", format_perf_snapshot(&current));
                    if let Some(baseline) = &state.perf_baseline {
                        line.push_str(" | ");
                        line.push_str(&format_perf_delta(&current, baseline));
                    }
                    push_obs(state, line);
                }
            }
        }
        "/daemon" => {
            return handle_daemon_command(&mut parts, state);
        }
        "/watch" => {
            return handle_watch_command(&mut parts, state);
        }
        _ => {
            push_obs(
                state,
                "⚠ unknown command. try /new /history /settings /edit /open /save /run /run-current /artifact /artifact-chunks /artifact-list /artifact-maintain /artifact-extract /artifact-extractions /artifact-pack /artifact-packs /artifact-pack-use /artifact-pack-auto /artifact-verify /artifact-verifications /artifact-verification /verify-policy /stage-routes /stage-route-set /stage-route-reset /close /allowlist-preview /clear-key /rotate-key /model /depth /stop /regen /export /perf /daemon /watch"
                    .to_string(),
            );
        }
    }

    EventOutcome::Continue
}

fn persist_stage_routing_defaults(state: &TuiState) {
    let mut defaults = load_tui_defaults();
    defaults.stage_routing = Some(state.stage_routing.clone());
    save_tui_defaults(&defaults);
}

fn persist_response_depth_defaults(state: &TuiState) {
    let mut defaults = load_tui_defaults();
    defaults.response_depth_mode = Some(state.response_depth_mode.clone());
    save_tui_defaults(&defaults);
}

fn depth_mode_hint(mode: &str) -> &'static str {
    match mode {
        "concise" => "short direct answers",
        "deep" => "detailed evidence-forward answers",
        _ => "balanced answer depth",
    }
}
