//! Unified web search orchestration: BrowserHost → lite → Grapheme fallback.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::RwLock;

use medousa_browser_lite::{search_ddg_html_cached_async, SearchResponse};

use crate::agent_runtime::stream_sink::SharedAgentStreamSink;
use crate::browser_host_client::{browser_host_healthy, browser_host_search};
use crate::browser_sessions::{
    complete_browser_session, create_browser_session, get_browser_session,
    mark_browser_challenge, BrowserSessionCompleteRequest, BrowserSessionCreateRequest,
    BrowserSessionStatus,
};
use crate::browser_tools::{is_client_executed_browser, surface_supports_browser_host};
use crate::turn_continuation::TurnContinuationScope;

const CLIENT_WAIT_SECS: u64 = 120;
const CLIENT_POLL_MS: u64 = 500;

pub fn is_discovery_binding(reference: &str) -> bool {
    matches!(reference, "web.providers" | "web.capabilities")
}

pub fn search_response_to_tool_json(
    query: &str,
    mode: &str,
    provider: Option<&str>,
    response: &SearchResponse,
    binding_used: &str,
) -> Value {
    if let Some(challenge) = response.challenge.as_deref() {
        return json!({
            "query": query,
            "mode": mode,
            "provider_requested": provider,
            "binding_used": binding_used,
            "decision": "challenge_required",
            "status": "challenge_required",
            "challenge": challenge,
            "result": {
                "results": response.results,
                "cached": response.cached,
            }
        });
    }
    json!({
        "query": query,
        "mode": mode,
        "provider_requested": provider,
        "binding_used": binding_used,
        "decision": "allow",
        "effect_class": "external_read",
        "result": {
            "ok": true,
            "results": response.results,
            "provider": response.provider,
            "cached": response.cached,
        }
    })
}

pub async fn resolve_browser_host_enabled(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
) -> (bool, Option<String>) {
    let scope = turn_scope.read().await.clone();
    let Some(scope) = scope else {
        return (false, None);
    };
    if !scope.supports_browser_host {
        return (false, scope.channel_surface);
    }
    let client_executed = scope
        .channel_surface
        .as_deref()
        .is_some_and(|label| label.starts_with("home-ios") || label.starts_with("home-android"));
    if client_executed {
        return (true, scope.channel_surface);
    }
    if browser_host_healthy().await {
        return (true, scope.channel_surface);
    }
    (false, scope.channel_surface)
}

pub async fn run_browser_backed_search(
    query: &str,
    max_results: usize,
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    turn_correlation_id: &str,
    chat_session_id: &str,
    sink: Option<SharedAgentStreamSink>,
) -> Result<SearchResponse, String> {
    let (enabled, channel) = resolve_browser_host_enabled(turn_scope).await;
    if !enabled {
        return search_ddg_html_cached_async(query, max_results).await;
    }

    let client_executed = channel
        .as_deref()
        .is_some_and(|label| label.starts_with("home-ios") || label.starts_with("home-android"));

    if client_executed {
        return run_client_executed_search(
            query,
            max_results,
            turn_correlation_id,
            chat_session_id,
            sink,
        )
        .await;
    }

    browser_host_search(query, max_results).await
}

async fn run_client_executed_search(
    query: &str,
    max_results: usize,
    turn_correlation_id: &str,
    chat_session_id: &str,
    sink: Option<SharedAgentStreamSink>,
) -> Result<SearchResponse, String> {
    let session = create_browser_session(BrowserSessionCreateRequest {
        turn_id: turn_correlation_id.to_string(),
        chat_session_id: chat_session_id.to_string(),
        query: query.to_string(),
        max_results,
        client_executed: true,
    });

    let navigate_url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding(query)
    );

    if let Some(sink) = sink {
        sink.browser_challenge_required(
            turn_correlation_id,
            session.session_id.clone(),
            navigate_url.clone(),
            "client_search".to_string(),
        )
        .await;
    }

    let deadline = Duration::from_secs(CLIENT_WAIT_SECS);
    let started = std::time::Instant::now();
    while started.elapsed() < deadline {
        if let Some(current) = get_browser_session(&session.session_id) {
            match current.status {
                BrowserSessionStatus::Completed => {
                    return current
                        .search_response
                        .ok_or_else(|| "browser session completed without results".to_string());
                }
                BrowserSessionStatus::Failed => {
                    return Err(current
                        .error
                        .unwrap_or_else(|| "browser session failed".to_string()));
                }
                BrowserSessionStatus::ChallengeRequired => {
                    // still waiting on user
                }
                BrowserSessionStatus::PendingClient => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(CLIENT_POLL_MS)).await;
    }
    Err("browser session timed out waiting for client".to_string())
}

pub async fn handle_search_challenge(
    session_id: &str,
    url: String,
    reason: String,
    sink: Option<SharedAgentStreamSink>,
    turn_correlation_id: &str,
) -> Value {
    let _ = mark_browser_challenge(session_id, url.clone(), reason.clone());
    if let Some(sink) = sink {
        sink.browser_challenge_required(turn_correlation_id, session_id.to_string(), url, reason)
            .await;
    }
    json!({
        "status": "challenge_required",
        "session_id": session_id,
        "decision": "challenge_required",
    })
}

pub fn complete_client_browser_session(
    session_id: &str,
    response: SearchResponse,
) -> Result<Value, String> {
    complete_browser_session(
        session_id,
        BrowserSessionCompleteRequest {
            search_response: Some(response),
            error: None,
        },
    )
    .ok_or_else(|| format!("browser session not found: {session_id}"))?;
    Ok(json!({ "ok": true, "session_id": session_id }))
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn surface_from_scope(scope: Option<&TurnContinuationScope>) -> Option<crate::daemon_api::TurnSurfaceContext> {
    scope.map(|scope| crate::daemon_api::TurnSurfaceContext {
        channel_surface: scope.channel_surface.clone(),
        channel_id: Some(scope.session_id.clone()),
        user_id: None,
        supports_ui_artifacts: scope.supports_ui_artifacts,
        supports_browser_host: scope.supports_browser_host,
    })
}

pub fn supports_browser(scope: Option<&TurnContinuationScope>) -> bool {
    surface_supports_browser_host(surface_from_scope(scope).as_ref())
}

pub fn client_executed(scope: Option<&TurnContinuationScope>) -> bool {
    is_client_executed_browser(surface_from_scope(scope).as_ref())
}
