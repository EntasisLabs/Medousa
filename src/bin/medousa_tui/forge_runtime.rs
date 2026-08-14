//! Code + Review (Forge) — undertakings desk over daemon `/v1/forge`.

use medousa::tui::editor_buffer::TextBuffer;
use medousa_sdk::transport::decode;
use serde::Deserialize;
use serde_json::json;

use super::daemon_commands::daemon_client;
use super::{EventOutcome, TuiState, UiMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForgePickerTarget {
    Code,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeFocus {
    Tree,
    Buffer,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ForgeItemHit {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub human_phase: String,
    #[serde(default)]
    pub brief: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CodeWorkspace {
    pub work_id: String,
    pub title: String,
    pub tree: Vec<String>,
    pub tree_selected: usize,
    pub tree_scroll: u16,
    pub open_path: Option<String>,
    pub buffer: TextBuffer,
    pub digest: String,
    pub dirty: bool,
    pub status: String,
    pub scroll: u16,
    pub preferred_col: Option<usize>,
    pub lease_id: Option<String>,
    pub lease_generation: Option<u64>,
    pub focus: CodeFocus,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewFileView {
    pub path: String,
    pub status: String,
    pub reviewed_oid: String,
    pub additions: usize,
    pub deletions: usize,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewWorkspace {
    #[allow(dead_code)]
    pub work_id: String,
    pub title: String,
    pub human_phase: String,
    pub synthesis_summary: String,
    /// Provenance chips: "You · Codex · Terminal" (Home DiffStack attribution).
    pub attribution_line: String,
    pub disposition: Option<String>,
    pub evidence_id: Option<String>,
    pub evidence_digest: Option<String>,
    pub decision_id: Option<String>,
    pub can_review: bool,
    pub can_apply: bool,
    pub files: Vec<ReviewFileView>,
    pub file_selected: usize,
    pub scroll: u16,
    pub status: String,
}

async fn forge_get<T: serde::de::DeserializeOwned>(
    daemon_url: &str,
    path: &str,
) -> Result<T, String> {
    let client = daemon_client(daemon_url).map_err(|error| error.to_string())?;
    let value = client
        .transport()
        .get_json(client.base_url(), path)
        .await
        .map_err(|e| e.to_string())?;
    decode(value).await.map_err(|e| e.to_string())
}

async fn forge_post<T: serde::de::DeserializeOwned>(
    daemon_url: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<T, String> {
    let client = daemon_client(daemon_url).map_err(|error| error.to_string())?;
    let value = client
        .transport()
        .post_json(client.base_url(), path, body)
        .await
        .map_err(|e| e.to_string())?;
    decode(value).await.map_err(|e| e.to_string())
}

async fn forge_put<T: serde::de::DeserializeOwned>(
    daemon_url: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<T, String> {
    let client = daemon_client(daemon_url).map_err(|error| error.to_string())?;
    let value = client
        .transport()
        .put_json(client.base_url(), path, body)
        .await
        .map_err(|e| e.to_string())?;
    decode(value).await.map_err(|e| e.to_string())
}

pub(crate) async fn open_forge_picker(state: &mut TuiState, target: ForgePickerTarget) {
    state.mode = UiMode::ForgePicker;
    state.forge_picker_target = target;
    state.forge_picker_query.clear();
    state.forge_picker_selected = 0;
    refresh_forge_picker(state).await;
}

pub(crate) async fn refresh_forge_picker(state: &mut TuiState) {
    match forge_get::<Vec<ForgeItemHit>>(&state.daemon_url, "/v1/forge/items").await {
        Ok(mut items) => {
            let q = state.forge_picker_query.trim().to_ascii_lowercase();
            if !q.is_empty() {
                items.retain(|item| {
                    item.title.to_ascii_lowercase().contains(&q)
                        || item.id.to_ascii_lowercase().contains(&q)
                        || item.brief.to_ascii_lowercase().contains(&q)
                        || item.human_phase.to_ascii_lowercase().contains(&q)
                });
            }
            state.forge_picker_hits = items;
            if state.forge_picker_selected >= state.forge_picker_hits.len() {
                state.forge_picker_selected = state.forge_picker_hits.len().saturating_sub(1);
            }
        }
        Err(err) => {
            state.forge_picker_hits.clear();
            super::push_obs(state, format!("⚠ forge list failed: {err}"));
        }
    }
}

pub(crate) async fn open_code_work(state: &mut TuiState, work_id: &str, title: &str) -> bool {
    #[derive(Deserialize)]
    struct TreeResp {
        files: Vec<TreeFile>,
    }
    #[derive(Deserialize)]
    struct TreeFile {
        path: String,
    }

    let tree_path = format!("/v1/forge/items/{}/tree", work_id);
    let tree = match forge_get::<TreeResp>(&state.daemon_url, &tree_path).await {
        Ok(resp) => resp.files.into_iter().map(|f| f.path).collect::<Vec<_>>(),
        Err(err) => {
            super::push_obs(state, format!("⚠ forge tree failed: {err}"));
            return false;
        }
    };

    let workspace = CodeWorkspace {
        work_id: work_id.to_string(),
        title: title.to_string(),
        tree,
        tree_selected: 0,
        tree_scroll: 0,
        open_path: None,
        buffer: TextBuffer::default(),
        digest: String::new(),
        dirty: false,
        status: "ready".to_string(),
        scroll: 0,
        preferred_col: None,
        lease_id: None,
        lease_generation: None,
        focus: CodeFocus::Tree,
    };
    state.code_workspaces.insert(work_id.to_string(), workspace);
    if state
        .workspace
        .open_code_tab_in_active(work_id, ".", title)
    {
        state.mode = UiMode::Code;
        super::workspace_runtime::persist_workspace(state);
        super::push_obs(state, format!("✓ code workspace {title}"));
        true
    } else {
        super::push_obs(state, "⚠ tab cap reached".to_string());
        false
    }
}

pub(crate) async fn open_review_work(state: &mut TuiState, work_id: &str, title: &str) -> bool {
    match load_review_workspace(&state.daemon_url, work_id, title).await {
        Ok(review) => {
            state.review_workspaces.insert(work_id.to_string(), review);
            if state.workspace.open_review_tab_in_active(work_id, title) {
                state.mode = UiMode::Review;
                super::workspace_runtime::persist_workspace(state);
                super::push_obs(state, format!("✓ review {title}"));
                true
            } else {
                super::push_obs(state, "⚠ tab cap reached".to_string());
                false
            }
        }
        Err(err) => {
            super::push_obs(state, format!("⚠ forge review failed: {err}"));
            false
        }
    }
}

async fn load_review_workspace(
    daemon_url: &str,
    work_id: &str,
    title: &str,
) -> Result<ReviewWorkspace, String> {
    #[derive(Deserialize, Default)]
    struct Affordance {
        #[serde(default)]
        allowed: bool,
    }
    #[derive(Deserialize, Default)]
    struct Allowed {
        #[serde(default)]
        review: Affordance,
        #[serde(default)]
        apply: Affordance,
    }
    #[derive(Deserialize)]
    struct ChangedFile {
        path: String,
        #[serde(default)]
        status: String,
    }
    #[derive(Deserialize)]
    struct Synthesis {
        #[serde(default)]
        status_summary: String,
        #[serde(default)]
        recommended_next_action: String,
    }
    #[derive(Deserialize)]
    struct Decision {
        #[serde(default)]
        id: Option<String>,
    }
    #[derive(Deserialize)]
    struct Attribution {
        #[serde(default)]
        kind: String,
        #[serde(default)]
        label: String,
    }
    #[derive(Deserialize)]
    struct ReviewProj {
        #[serde(default)]
        title: String,
        #[serde(default)]
        human_phase: String,
        #[serde(default)]
        allowed_actions: Allowed,
        #[serde(default)]
        evidence_id: Option<String>,
        #[serde(default)]
        evidence_digest: Option<String>,
        #[serde(default)]
        changed_files: Vec<ChangedFile>,
        #[serde(default)]
        synthesis: Option<Synthesis>,
        #[serde(default)]
        decision: Option<Decision>,
        #[serde(default)]
        attribution: Vec<Attribution>,
        #[serde(default)]
        disposition: Option<String>,
    }
    #[derive(Deserialize)]
    struct DiffLine {
        #[serde(default)]
        kind: String,
        #[serde(default)]
        content: String,
    }
    #[derive(Deserialize)]
    struct DiffHunk {
        #[serde(default)]
        old_start: usize,
        #[serde(default)]
        old_count: usize,
        #[serde(default)]
        new_start: usize,
        #[serde(default)]
        new_count: usize,
        #[serde(default)]
        lines: Vec<DiffLine>,
    }
    #[derive(Deserialize)]
    struct FileDiff {
        path: String,
        #[serde(default)]
        status: String,
        #[serde(default)]
        reviewed_oid: String,
        #[serde(default)]
        binary: bool,
        #[serde(default)]
        hunks: Vec<DiffHunk>,
    }

    let review_path = format!("/v1/forge/items/{work_id}/review");
    let proj: ReviewProj = forge_get(daemon_url, &review_path).await?;
    let attribution_line = if proj.attribution.is_empty() {
        String::new()
    } else {
        proj.attribution
            .iter()
            .map(|a| {
                if !a.label.trim().is_empty() {
                    a.label.clone()
                } else {
                    match a.kind.as_str() {
                        "human" => "You".to_string(),
                        "agent" => "Agent".to_string(),
                        "terminal" => "Terminal".to_string(),
                        other => other.to_string(),
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let mut files = Vec::new();
    for changed in proj.changed_files.iter().take(24) {
        let enc = urlencoding_path(&changed.path);
        let file_path =
            format!("/v1/forge/items/{work_id}/review/file?path={enc}");
        match forge_get::<FileDiff>(daemon_url, &file_path).await {
            Ok(diff) => {
                let mut additions = 0usize;
                let mut deletions = 0usize;
                let mut lines = Vec::new();
                lines.push(format!(
                    "── {} ({}) ──",
                    diff.path, diff.status
                ));
                if diff.binary {
                    lines.push("  (binary — no text preview)".to_string());
                } else {
                    for hunk in diff.hunks {
                        lines.push(format!(
                            "@@ -{},{} +{},{} @@",
                            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
                        ));
                        for line in hunk.lines {
                            let prefix = match line.kind.as_str() {
                                "addition" => {
                                    additions += 1;
                                    '+'
                                }
                                "deletion" => {
                                    deletions += 1;
                                    '-'
                                }
                                _ => ' ',
                            };
                            lines.push(format!("{prefix}{}", line.content));
                        }
                    }
                }
                if lines.len() == 1 {
                    lines.push("  (no textual hunks)".to_string());
                }
                files.push(ReviewFileView {
                    path: diff.path,
                    status: diff.status,
                    reviewed_oid: diff.reviewed_oid,
                    additions,
                    deletions,
                    lines,
                });
            }
            Err(err) => {
                files.push(ReviewFileView {
                    path: changed.path.clone(),
                    status: changed.status.clone(),
                    reviewed_oid: String::new(),
                    additions: 0,
                    deletions: 0,
                    lines: vec![
                        format!("── {} ──", changed.path),
                        format!("  (diff unavailable: {err})"),
                    ],
                });
            }
        }
    }

    let synthesis = proj.synthesis.as_ref();
    let summary = match synthesis {
        Some(s) if !s.status_summary.is_empty() => {
            format!("{} · {}", s.status_summary, s.recommended_next_action)
        }
        Some(s) => s.recommended_next_action.clone(),
        None => String::new(),
    };

    Ok(ReviewWorkspace {
        work_id: work_id.to_string(),
        title: if proj.title.is_empty() {
            title.to_string()
        } else {
            proj.title
        },
        human_phase: proj.human_phase,
        synthesis_summary: summary,
        attribution_line,
        disposition: proj.disposition,
        evidence_id: proj.evidence_id,
        evidence_digest: proj.evidence_digest,
        decision_id: proj.decision.and_then(|d| d.id),
        can_review: proj.allowed_actions.review.allowed,
        can_apply: proj.allowed_actions.apply.allowed,
        files,
        file_selected: 0,
        scroll: 0,
        status: "loaded".to_string(),
    })
}

fn urlencoding_path(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub(crate) async fn open_code_file(state: &mut TuiState, work_id: &str, path: &str) {
    #[derive(Deserialize)]
    struct SourceResp {
        content: String,
        digest: String,
    }
    let enc = urlencoding_path(path);
    let api = format!("/v1/forge/items/{work_id}/source?path={enc}");
    match forge_get::<SourceResp>(&state.daemon_url, &api).await {
        Ok(resp) => {
            if let Some(ws) = state.code_workspaces.get_mut(work_id) {
                ws.open_path = Some(path.to_string());
                ws.buffer = TextBuffer::from_text(resp.content);
                ws.digest = resp.digest;
                ws.dirty = false;
                ws.scroll = 0;
                ws.focus = CodeFocus::Buffer;
                ws.status = format!("opened {path}");
            }
        }
        Err(err) => super::push_obs(state, format!("⚠ source read failed: {err}")),
    }
}

async fn ensure_lease(state: &mut TuiState, work_id: &str) -> Result<(), String> {
    let needs = state
        .code_workspaces
        .get(work_id)
        .map(|ws| ws.lease_id.is_none())
        .unwrap_or(true);
    if !needs {
        return Ok(());
    }
    #[derive(Deserialize)]
    struct Lease {
        lease_id: String,
        generation: u64,
    }
    #[derive(Deserialize)]
    struct BeginResp {
        lease: Lease,
    }
    let api = format!("/v1/forge/items/{work_id}/attempts");
    let resp: BeginResp = forge_post(
        &state.daemon_url,
        &api,
        json!({ "executor": { "kind": "human", "detail": {} } }),
    )
    .await?;
    if let Some(ws) = state.code_workspaces.get_mut(work_id) {
        ws.lease_id = Some(resp.lease.lease_id);
        ws.lease_generation = Some(resp.lease.generation);
        ws.status = "lease acquired".to_string();
    }
    Ok(())
}

/// Seal the active code desk lease → evidence for Review (Home `sealLease` parity).
pub(crate) async fn seal_active_code(state: &mut TuiState) {
    let Some(work_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.code_work_id().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no code tab focused".to_string());
        return;
    };
    if state
        .code_workspaces
        .get(&work_id)
        .is_some_and(|ws| ws.dirty)
    {
        save_active_code(state).await;
        if state
            .code_workspaces
            .get(&work_id)
            .is_some_and(|ws| ws.dirty)
        {
            super::push_obs(state, "⚠ save before seal failed — fix and retry".to_string());
            return;
        }
    }
    if let Err(err) = ensure_lease(state, &work_id).await {
        super::push_obs(state, format!("⚠ begin attempt failed: {err}"));
        return;
    }
    let Some(ws) = state.code_workspaces.get(&work_id) else {
        return;
    };
    let (Some(lease_id), Some(generation)) = (ws.lease_id.clone(), ws.lease_generation) else {
        super::push_obs(state, "⚠ no lease to seal".to_string());
        return;
    };
    let title = ws.title.clone();
    let api = format!("/v1/forge/leases/{lease_id}/complete");
    match forge_post::<serde_json::Value>(
        &state.daemon_url,
        &api,
        json!({ "generation": generation }),
    )
    .await
    {
        Ok(_) => {
            if let Some(ws) = state.code_workspaces.get_mut(&work_id) {
                ws.lease_id = None;
                ws.lease_generation = None;
                ws.status = "sealed → review".to_string();
            }
            super::push_obs(state, format!("✓ sealed {title} — opening review"));
            let _ = open_review_work(state, &work_id, &title).await;
        }
        Err(err) => {
            // Retry once with ack_risks when policy demands it.
            if err.to_ascii_lowercase().contains("ack")
                || err.to_ascii_lowercase().contains("risk")
            {
                match forge_post::<serde_json::Value>(
                    &state.daemon_url,
                    &api,
                    json!({ "generation": generation, "ack_risks": true }),
                )
                .await
                {
                    Ok(_) => {
                        if let Some(ws) = state.code_workspaces.get_mut(&work_id) {
                            ws.lease_id = None;
                            ws.lease_generation = None;
                            ws.status = "sealed → review".to_string();
                        }
                        super::push_obs(
                            state,
                            format!("✓ sealed {title} (acked risks) — opening review"),
                        );
                        let _ = open_review_work(state, &work_id, &title).await;
                        return;
                    }
                    Err(err2) => {
                        super::push_obs(state, format!("⚠ seal failed: {err2}"));
                        return;
                    }
                }
            }
            super::push_obs(state, format!("⚠ seal failed: {err}"));
        }
    }
}

pub(crate) async fn save_active_code(state: &mut TuiState) {
    let Some(work_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.code_work_id().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no code tab focused".to_string());
        return;
    };
    let Some(ws) = state.code_workspaces.get(&work_id) else {
        return;
    };
    if !ws.dirty {
        super::push_obs(state, "code unchanged".to_string());
        return;
    }
    let Some(path) = ws.open_path.clone() else {
        super::push_obs(state, "⚠ no file open".to_string());
        return;
    };
    if let Err(err) = ensure_lease(state, &work_id).await {
        super::push_obs(state, format!("⚠ begin attempt failed: {err}"));
        return;
    }
    let Some(ws) = state.code_workspaces.get(&work_id) else {
        return;
    };
    let (Some(lease_id), Some(generation)) = (ws.lease_id.clone(), ws.lease_generation) else {
        super::push_obs(state, "⚠ no lease".to_string());
        return;
    };
    let content = ws.buffer.as_text().to_string();
    let digest = ws.digest.clone();
    #[derive(Deserialize)]
    struct SourceResp {
        digest: String,
    }
    let api = format!("/v1/forge/items/{work_id}/source");
    match forge_put::<SourceResp>(
        &state.daemon_url,
        &api,
        json!({
            "path": path,
            "content": content,
            "lease_id": lease_id,
            "generation": generation,
            "expected_digest": digest,
        }),
    )
    .await
    {
        Ok(resp) => {
            if let Some(ws) = state.code_workspaces.get_mut(&work_id) {
                ws.digest = resp.digest;
                ws.dirty = false;
                ws.status = format!("saved {path}");
            }
            super::push_obs(state, format!("✓ saved {path}"));
        }
        Err(err) => {
            if let Some(ws) = state.code_workspaces.get_mut(&work_id) {
                ws.status = format!("save failed: {err}");
            }
            super::push_obs(state, format!("⚠ forge save failed: {err}"));
        }
    }
}

pub(crate) async fn approve_active_review(state: &mut TuiState) {
    let Some(work_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.review_work_id().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no review tab focused".to_string());
        return;
    };
    let Some(review) = state.review_workspaces.get(&work_id) else {
        return;
    };
    let (Some(evidence_id), Some(evidence_digest)) =
        (review.evidence_id.clone(), review.evidence_digest.clone())
    else {
        super::push_obs(state, "⚠ review has no sealed evidence yet".to_string());
        return;
    };
    if !review.can_review {
        super::push_obs(state, "⚠ approve not allowed in this phase".to_string());
        return;
    }
    let api = format!("/v1/forge/items/{work_id}/decisions");
    match forge_post::<serde_json::Value>(
        &state.daemon_url,
        &api,
        json!({
            "evidence_id": evidence_id,
            "evidence_digest": evidence_digest,
            "strategy": "preserve_branch",
            "rationale": null,
            "acknowledged_violations": [],
        }),
    )
    .await
    {
        Ok(_) => {
            let title = state
                .review_workspaces
                .get(&work_id)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| work_id.clone());
            let _ = open_review_work(state, &work_id, &title).await;
            super::push_obs(state, "✓ review intent recorded (approve)".to_string());
        }
        Err(err) => super::push_obs(state, format!("⚠ approve failed: {err}")),
    }
}

pub(crate) async fn finish_active_review(state: &mut TuiState) {
    let Some(work_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.review_work_id().map(str::to_string))
    else {
        super::push_obs(state, "⚠ no review tab focused".to_string());
        return;
    };
    let Some(decision_id) = state
        .review_workspaces
        .get(&work_id)
        .and_then(|r| r.decision_id.clone())
    else {
        super::push_obs(
            state,
            "⚠ no decision yet — press a to approve first".to_string(),
        );
        return;
    };
    let api = format!("/v1/forge/items/{work_id}/apply");
    match forge_post::<serde_json::Value>(
        &state.daemon_url,
        &api,
        json!({ "decision_id": decision_id }),
    )
    .await
    {
        Ok(_) => {
            if let Some(r) = state.review_workspaces.get_mut(&work_id) {
                r.status = "finished".to_string();
                r.can_apply = false;
                r.can_review = false;
            }
            super::push_obs(state, "✓ decision applied (finish)".to_string());
        }
        Err(err) => super::push_obs(state, format!("⚠ finish failed: {err}")),
    }
}

pub(crate) async fn restore_active_review_file(state: &mut TuiState) {
    let Some(work_id) = state
        .workspace
        .active_tab()
        .and_then(|t| t.review_work_id().map(str::to_string))
    else {
        return;
    };
    let Some(review) = state.review_workspaces.get(&work_id) else {
        return;
    };
    let Some(file) = review.files.get(review.file_selected).cloned() else {
        return;
    };
    if file.reviewed_oid.is_empty() {
        super::push_obs(state, "⚠ cannot restore — missing reviewed oid".to_string());
        return;
    }
    let api = format!("/v1/forge/items/{work_id}/review/file");
    match forge_post::<serde_json::Value>(
        &state.daemon_url,
        &api,
        json!({
            "path": file.path,
            "expected_reviewed_oid": file.reviewed_oid,
        }),
    )
    .await
    {
        Ok(_) => {
            let title = state
                .review_workspaces
                .get(&work_id)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| work_id.clone());
            let _ = open_review_work(state, &work_id, &title).await;
            let _ = open_code_work(state, &work_id, &title).await;
            super::push_obs(
                state,
                format!("✓ restored {} (request changes)", file.path),
            );
        }
        Err(err) => super::push_obs(state, format!("⚠ restore failed: {err}")),
    }
}

pub(crate) fn focused_code_mut(state: &mut TuiState) -> Option<&mut CodeWorkspace> {
    let work_id = state
        .workspace
        .active_tab()
        .and_then(|t| t.code_work_id().map(str::to_string))?;
    state.code_workspaces.get_mut(&work_id)
}

pub(crate) fn focused_review_mut(state: &mut TuiState) -> Option<&mut ReviewWorkspace> {
    let work_id = state
        .workspace
        .active_tab()
        .and_then(|t| t.review_work_id().map(str::to_string))?;
    state.review_workspaces.get_mut(&work_id)
}

pub(crate) async fn handle_forge_picker_key(
    key: crossterm::event::KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Esc => {
            state.mode = UiMode::Chat;
            EventOutcome::Continue
        }
        KeyCode::Up => {
            state.forge_picker_selected = state.forge_picker_selected.saturating_sub(1);
            EventOutcome::Continue
        }
        KeyCode::Down => {
            if !state.forge_picker_hits.is_empty() {
                state.forge_picker_selected = (state.forge_picker_selected + 1)
                    .min(state.forge_picker_hits.len().saturating_sub(1));
            }
            EventOutcome::Continue
        }
        KeyCode::Enter => {
            if let Some(hit) = state.forge_picker_hits.get(state.forge_picker_selected).cloned()
            {
                let title = if hit.title.is_empty() {
                    hit.id.clone()
                } else {
                    hit.title.clone()
                };
                match state.forge_picker_target {
                    ForgePickerTarget::Code => {
                        let _ = open_code_work(state, &hit.id, &title).await;
                    }
                    ForgePickerTarget::Review => {
                        let _ = open_review_work(state, &hit.id, &title).await;
                    }
                }
            }
            EventOutcome::Continue
        }
        KeyCode::Backspace => {
            state.forge_picker_query.pop();
            refresh_forge_picker(state).await;
            EventOutcome::Continue
        }
        KeyCode::Char(c) => {
            state.forge_picker_query.push(c);
            refresh_forge_picker(state).await;
            EventOutcome::Continue
        }
        _ => EventOutcome::Continue,
    }
}

pub(crate) async fn handle_code_key(
    key: crossterm::event::KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    use crossterm::event::{KeyCode, KeyModifiers};

    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        save_active_code(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Char('e') && key.modifiers.contains(KeyModifiers::CONTROL) {
        // Seal lease → evidence (Home seal path). Avoids Ctrl+S clash.
        seal_active_code(state).await;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if let Some(work_id) = state
            .workspace
            .active_tab()
            .and_then(|t| t.code_work_id().map(str::to_string))
        {
            let title = state
                .code_workspaces
                .get(&work_id)
                .map(|w| w.title.clone())
                .unwrap_or_else(|| work_id.clone());
            let _ = open_review_work(state, &work_id, &title).await;
        }
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Esc {
        if let Some(ws) = focused_code_mut(state)
            && ws.focus == CodeFocus::Buffer
        {
            ws.focus = CodeFocus::Tree;
            return EventOutcome::Continue;
        }
        state.mode = UiMode::Chat;
        return EventOutcome::Continue;
    }
    if key.code == KeyCode::Tab {
        if let Some(ws) = focused_code_mut(state) {
            ws.focus = match ws.focus {
                CodeFocus::Tree => CodeFocus::Buffer,
                CodeFocus::Buffer => CodeFocus::Tree,
            };
        }
        return EventOutcome::Continue;
    }

    let focus = focused_code_mut(state).map(|ws| ws.focus);
    let Some(focus) = focus else {
        return EventOutcome::Continue;
    };

    if focus == CodeFocus::Tree && key.code == KeyCode::Enter {
        let (work_id, path) = focused_code_mut(state)
            .map(|ws| {
                (
                    ws.work_id.clone(),
                    ws.tree.get(ws.tree_selected).cloned(),
                )
            })
            .unwrap_or_default();
        if let Some(path) = path {
            open_code_file(state, &work_id, &path).await;
        }
        return EventOutcome::Continue;
    }

    let Some(ws) = focused_code_mut(state) else {
        return EventOutcome::Continue;
    };
    match focus {
        CodeFocus::Tree => match key.code {
            KeyCode::Up => {
                ws.tree_selected = ws.tree_selected.saturating_sub(1);
                ws.tree_scroll = ws.tree_scroll.min(ws.tree_selected as u16);
            }
            KeyCode::Down if !ws.tree.is_empty() => {
                ws.tree_selected =
                    (ws.tree_selected + 1).min(ws.tree.len().saturating_sub(1));
            }
            _ => {}
        },
        CodeFocus::Buffer => match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                ws.buffer.insert_char(c);
                ws.dirty = true;
                ws.preferred_col = None;
            }
            KeyCode::Enter => {
                ws.buffer.insert_newline();
                ws.dirty = true;
            }
            KeyCode::Backspace => {
                ws.buffer.backspace();
                ws.dirty = true;
            }
            KeyCode::Left => ws.buffer.move_left(),
            KeyCode::Right => ws.buffer.move_right(),
            KeyCode::Up => {
                let col = ws.preferred_col.unwrap_or_else(|| ws.buffer.line_col().1);
                ws.preferred_col = Some(col);
                ws.buffer.move_up(col);
                ws.scroll = ws.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                let col = ws.preferred_col.unwrap_or_else(|| ws.buffer.line_col().1);
                ws.preferred_col = Some(col);
                ws.buffer.move_down(col);
                ws.scroll = ws.scroll.saturating_add(1);
            }
            KeyCode::PageUp => ws.scroll = ws.scroll.saturating_sub(10),
            KeyCode::PageDown => ws.scroll = ws.scroll.saturating_add(10),
            KeyCode::Home => ws.buffer.move_line_start(),
            KeyCode::End => ws.buffer.move_line_end(),
            _ => {}
        },
    }
    EventOutcome::Continue
}

pub(crate) async fn handle_review_key(
    key: crossterm::event::KeyEvent,
    state: &mut TuiState,
) -> EventOutcome {
    use crossterm::event::KeyCode;

    match key.code {
        KeyCode::Esc => {
            state.mode = UiMode::Chat;
            return EventOutcome::Continue;
        }
        KeyCode::Char('a') => {
            approve_active_review(state).await;
            return EventOutcome::Continue;
        }
        KeyCode::Char('f') => {
            finish_active_review(state).await;
            return EventOutcome::Continue;
        }
        KeyCode::Char('u') => {
            restore_active_review_file(state).await;
            return EventOutcome::Continue;
        }
        KeyCode::Char('c') => {
            if let Some(work_id) = state
                .workspace
                .active_tab()
                .and_then(|t| t.review_work_id().map(str::to_string))
            {
                let title = state
                    .review_workspaces
                    .get(&work_id)
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| work_id.clone());
                let _ = open_code_work(state, &work_id, &title).await;
            }
            return EventOutcome::Continue;
        }
        _ => {}
    }

    let Some(review) = focused_review_mut(state) else {
        return EventOutcome::Continue;
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('[') => {
            review.file_selected = review.file_selected.saturating_sub(1);
            review.scroll = 0;
        }
        KeyCode::Right | KeyCode::Char(']') => {
            if !review.files.is_empty() {
                review.file_selected =
                    (review.file_selected + 1).min(review.files.len().saturating_sub(1));
                review.scroll = 0;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            review.scroll = review.scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            review.scroll = review.scroll.saturating_add(1);
        }
        KeyCode::PageUp => review.scroll = review.scroll.saturating_sub(10),
        KeyCode::PageDown => review.scroll = review.scroll.saturating_add(10),
        _ => {}
    }
    EventOutcome::Continue
}
