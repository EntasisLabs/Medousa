use anyhow::{Result, anyhow};

use crate::context_pack::{BuildContextPackInput, ContextPackBudgetProfile};
use crate::daemon_api::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactCommandSpec,
    ArtifactVerificationPolicyInput,
};

pub fn execute_artifact_command(request: ArtifactCommandRequest) -> Result<ArtifactCommandResponse> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(anyhow!("session_id is required"));
    }

    let mut selected_context_pack_query = normalize_optional_query(request.selected_context_pack_query);
    let rendered_output = match request.command {
        ArtifactCommandSpec::Lookup { query } => {
            let query = normalize_optional_query(query);
            match crate::artifact_store::find_artifact(&session_id, query.as_deref()) {
                Some(found) => {
                    let payload = serde_json::to_string_pretty(&found.payload)
                        .unwrap_or_else(|_| found.payload.to_string());
                    let preview = payload.chars().take(600).collect::<String>();
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
                    )
                }
                None => "⚠ artifact lookup found no match in this session".to_string(),
            }
        }
        ArtifactCommandSpec::Chunks { query } => {
            let query = normalize_optional_query(query);
            match crate::artifact_store::find_artifact(&session_id, query.as_deref()) {
                Some(found) => {
                    let refs = crate::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );
                    let refs_preview = serde_json::to_string_pretty(
                        &refs.iter().take(8).cloned().collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "[]".to_string());
                    format!(
                        "◈ artifact chunks {} total={}\n{}",
                        found.record.artifact_id,
                        refs.len(),
                        refs_preview
                    )
                }
                None => "⚠ artifact chunking found no match in this session".to_string(),
            }
        }
        ArtifactCommandSpec::List { limit } => {
            let limit = limit.clamp(1, 200);
            let records = crate::artifact_store::list_artifact_records(&session_id, limit);
            let stats = crate::artifact_store::artifact_index_stats(&session_id);

            if records.is_empty() {
                "◈ artifact list empty for current session".to_string()
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
                out.trim_end().to_string()
            }
        }
        ArtifactCommandSpec::Maintain {
            max_per_session,
            max_age_days,
        } => {
            let max_per_session = max_per_session.clamp(1, 10_000);
            let max_age_days = max_age_days.clamp(1, 3650);
            match crate::artifact_store::run_artifact_maintenance(max_per_session, max_age_days) {
                Ok(report) => format!(
                    "◈ artifact maintenance before={} after={} missing_pruned={} deduped_pruned={} retention_pruned={} files_deleted={}",
                    report.records_before,
                    report.records_after,
                    report.missing_payload_pruned,
                    report.deduped_records_pruned,
                    report.retention_pruned,
                    report.payload_files_deleted
                ),
                Err(err) => format!("⚠ artifact maintenance failed: {err}"),
            }
        }
        ArtifactCommandSpec::Extract { query } => {
            let query = normalize_optional_query(query);
            match crate::artifact_store::find_artifact(&session_id, query.as_deref()) {
                Some(found) => {
                    let chunk_refs = crate::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );
                    let claims = crate::artifact_extraction::extract_claims_from_chunks(
                        &found.record.artifact_id,
                        &found.payload,
                        &chunk_refs,
                    );
                    match crate::artifact_extraction::persist_extraction_run(
                        &session_id,
                        &found.record.artifact_id,
                        &claims,
                    ) {
                        Ok(record) => {
                            let preview = serde_json::to_string_pretty(
                                &claims.iter().take(8).cloned().collect::<Vec<_>>(),
                            )
                            .unwrap_or_else(|_| "[]".to_string());
                            format!(
                                "◈ extraction {} artifact={} claims={}\n{}",
                                record.extraction_id, record.artifact_id, record.claim_count, preview
                            )
                        }
                        Err(err) => format!("⚠ extraction persist failed: {err}"),
                    }
                }
                None => "⚠ extraction failed: no artifact found in this session".to_string(),
            }
        }
        ArtifactCommandSpec::Extractions { limit } => {
            let limit = limit.clamp(1, 200);
            let runs = crate::artifact_extraction::list_extraction_runs(&session_id, limit);
            if runs.is_empty() {
                "◈ extraction list empty for current session".to_string()
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
                out.trim_end().to_string()
            }
        }
        ArtifactCommandSpec::Pack {
            artifact_query,
            max_tokens,
            max_claims,
            max_chunks,
        } => {
            let artifact_query = artifact_query.trim();
            let artifact_query = if artifact_query.is_empty() {
                "last"
            } else {
                artifact_query
            };
            let max_tokens = max_tokens.clamp(256, 200_000);
            let max_claims = max_claims.clamp(1, 64);
            let max_chunks = max_chunks.clamp(1, 512);

            match crate::artifact_store::find_artifact(&session_id, Some(artifact_query)) {
                Some(found) => {
                    let chunk_refs = crate::artifact_chunking::chunk_json_payload(
                        &found.record.artifact_id,
                        &found.payload,
                        2400,
                        240,
                    );

                    let extraction =
                        crate::artifact_extraction::find_extraction(
                            &session_id,
                            Some(&found.record.artifact_id),
                        );
                    let claims = extraction.map(|run| run.claims).unwrap_or_else(|| {
                        crate::artifact_extraction::extract_claims_from_chunks(
                            &found.record.artifact_id,
                            &found.payload,
                            &chunk_refs,
                        )
                    });

                    let pack = crate::context_pack::build_context_pack(BuildContextPackInput {
                        session_id: session_id.clone(),
                        artifact_id: found.record.artifact_id.clone(),
                        claims,
                        chunk_refs,
                        budget_profile: ContextPackBudgetProfile {
                            max_tokens,
                            max_claims,
                            max_chunks,
                        },
                    });

                    match crate::context_pack::persist_context_pack(&pack) {
                        Ok(()) => {
                            let preview = serde_json::to_string_pretty(&pack)
                                .unwrap_or_else(|_| "{}".to_string());
                            let preview = preview.chars().take(800).collect::<String>();
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
                            )
                        }
                        Err(err) => format!("⚠ context pack persist failed: {err}"),
                    }
                }
                None => "⚠ context pack failed: no artifact found in this session".to_string(),
            }
        }
        ArtifactCommandSpec::Packs { limit } => {
            let limit = limit.clamp(1, 200);
            let packs = crate::context_pack::list_context_packs(&session_id, limit);
            if packs.is_empty() {
                "◈ context pack list empty for current session".to_string()
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
                out.trim_end().to_string()
            }
        }
        ArtifactCommandSpec::PackUse { query } => {
            let query = normalize_optional_query(query);
            if query.is_none() {
                let mode = selected_context_pack_query.as_deref().unwrap_or("last");
                format!("◈ context pack selector {mode}")
            } else {
                let query = query.unwrap_or_default();
                match crate::context_pack::find_context_pack(&session_id, Some(query.as_str())) {
                    Some(pack) => {
                        selected_context_pack_query = Some(query.clone());
                        format!(
                            "◈ context pack selector set query={} pack={} artifact={}",
                            query, pack.pack_id, pack.artifact_id
                        )
                    }
                    None => format!(
                        "⚠ context pack selector not set: no pack matched '{}' in this session",
                        query
                    ),
                }
            }
        }
        ArtifactCommandSpec::PackAuto => {
            selected_context_pack_query = None;
            "◈ context pack selector set to last".to_string()
        }
        ArtifactCommandSpec::Verify { query } => {
            let query = normalize_optional_query(query);
            let selector = query
                .as_deref()
                .or(selected_context_pack_query.as_deref())
                .unwrap_or("last")
                .to_string();

            match crate::context_pack::find_context_pack(&session_id, Some(selector.as_str())) {
                Some(pack) => {
                    let policy = request
                        .verification_policy
                        .map(verification_policy_from_input)
                        .unwrap_or_default();
                    let report = crate::verifier::verify_context_pack(&pack, &policy);
                    let verification_id = crate::verification_store::persist_verification(
                        &session_id,
                        &selector,
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
                    let route_label = request
                        .verifier_route_label
                        .clone()
                        .unwrap_or_else(|| "default".to_string());
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
                        route_label,
                    )
                }
                None => format!(
                    "⚠ verification failed: no context pack matched '{}' in this session",
                    selector
                ),
            }
        }
        ArtifactCommandSpec::Verifications { limit } => {
            let limit = limit.clamp(1, 200);
            let records = crate::verification_store::list_verifications(&session_id, limit);
            if records.is_empty() {
                "◈ verification history empty for current session".to_string()
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
                out.trim_end().to_string()
            }
        }
        ArtifactCommandSpec::Verification { query } => {
            let query = normalize_optional_query(query);
            let query = query.as_deref();
            match crate::verification_store::find_verification(&session_id, query) {
                Some(run) => {
                    let preview =
                        serde_json::to_string_pretty(&run).unwrap_or_else(|_| "{}".to_string());
                    let preview = preview.chars().take(1200).collect::<String>();
                    format!(
                        "◈ verification detail {}\n{}{}",
                        run.record.verification_id,
                        preview,
                        if preview.chars().count() >= 1200 {
                            "\n..."
                        } else {
                            ""
                        }
                    )
                }
                None => "⚠ verification lookup found no match in this session".to_string(),
            }
        }
    };

    Ok(ArtifactCommandResponse {
        selected_context_pack_query,
        rendered_output,
    })
}

fn normalize_optional_query(input: Option<String>) -> Option<String> {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn verification_policy_from_input(input: ArtifactVerificationPolicyInput) -> crate::verifier::VerificationPolicy {
    crate::verifier::VerificationPolicy {
        min_citation_coverage: input.min_citation_coverage.clamp(0.0, 1.0),
        min_avg_support_strength: input.min_avg_support_strength.clamp(0.0, 1.0),
        min_supported_claim_ratio: input.min_supported_claim_ratio.clamp(0.0, 1.0),
        min_claim_support_strength: input.min_claim_support_strength.clamp(0.0, 1.0),
    }
}

pub fn execute_artifact_fetch(
    request: crate::daemon_api::ArtifactFetchRequest,
) -> Result<crate::daemon_api::ArtifactFetchResponse> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(anyhow!("session_id is required"));
    }
    let artifact_id = request.artifact_id.trim().to_string();
    if artifact_id.is_empty() {
        return Err(anyhow!("artifact_id is required"));
    }

    let fetched = crate::artifact_store::fetch_artifact(&session_id, &artifact_id)
        .ok_or_else(|| anyhow!("artifact not found for this session"))?;

    Ok(crate::daemon_api::ArtifactFetchResponse {
        artifact_id: fetched.record.artifact_id,
        mime: fetched.mime,
        label: fetched
            .record
            .label
            .unwrap_or_else(|| "Artifact".to_string()),
        body: fetched.body,
        byte_size: fetched.record.byte_size,
        presentation: fetched.record.presentation,
        height_px: fetched.record.height_px,
    })
}

pub fn execute_artifact_list_ui(
    request: crate::daemon_api::ArtifactListUiRequest,
) -> Result<crate::daemon_api::ArtifactListUiResponse> {
    let session_id = request
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let query = request
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let records = crate::artifact_store::list_ui_artifacts(session_id, request.limit, query);
    let artifacts = records
        .into_iter()
        .map(|record| crate::daemon_api::ArtifactSummary {
            artifact_id: record.artifact_id,
            session_id: record.session_id,
            label: record
                .label
                .unwrap_or_else(|| "Presentation".to_string()),
            presentation: record.presentation,
            byte_size: record.byte_size,
            stored_at_utc: record.stored_at_utc,
            root_artifact_id: record.root_artifact_id,
            supersedes_artifact_id: record.supersedes_artifact_id,
        })
        .collect();
    Ok(crate::daemon_api::ArtifactListUiResponse { artifacts })
}
