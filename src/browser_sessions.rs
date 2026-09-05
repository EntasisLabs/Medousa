//! In-process browser sessions for client-executed search (home-ios) and CAPTCHA handoff.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use medousa_browser_lite::SearchResponse;

const SESSION_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActOutcome {
    pub ok: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_max_results() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSessionStatus {
    PendingClient,
    ChallengeRequired,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub session_id: String,
    pub turn_id: String,
    pub chat_session_id: String,
    /// Exact browser driver that owns client-side completion for this session.
    /// Legacy daemon-owned challenge sessions leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_driver_id: Option<String>,
    pub query: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default)]
    pub act_request: Option<serde_json::Value>,
    pub status: BrowserSessionStatus,
    pub challenge_url: Option<String>,
    pub challenge_reason: Option<String>,
    pub search_response: Option<SearchResponse>,
    #[serde(default)]
    pub act_result: Option<BrowserActOutcome>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionCreateRequest {
    pub turn_id: String,
    pub chat_session_id: String,
    #[serde(default)]
    pub world_driver_id: Option<String>,
    pub query: String,
    pub max_results: usize,
    pub client_executed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionCompleteRequest {
    pub search_response: Option<SearchResponse>,
    pub error: Option<String>,
}

struct SessionRecord {
    session: BrowserSession,
    inserted: Instant,
}

static SESSIONS: Lazy<Mutex<HashMap<String, SessionRecord>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn create_browser_session(request: BrowserSessionCreateRequest) -> BrowserSession {
    purge_expired();
    let session_id = format!("bs-{}", Uuid::new_v4());
    let status = BrowserSessionStatus::PendingClient;
    let session = BrowserSession {
        session_id: session_id.clone(),
        turn_id: request.turn_id,
        chat_session_id: request.chat_session_id,
        world_driver_id: request.world_driver_id,
        query: request.query,
        max_results: request.max_results,
        status,
        challenge_url: None,
        challenge_reason: None,
        act_request: None,
        search_response: None,
        act_result: None,
        error: None,
        created_at: chrono::Utc::now(),
    };
    SESSIONS.lock().expect("browser sessions").insert(
        session_id,
        SessionRecord {
            session: session.clone(),
            inserted: Instant::now(),
        },
    );
    session
}

pub fn get_browser_session(session_id: &str) -> Option<BrowserSession> {
    purge_expired();
    SESSIONS
        .lock()
        .expect("browser sessions")
        .get(session_id)
        .map(|record| record.session.clone())
}

pub fn mark_browser_challenge(
    session_id: &str,
    url: String,
    reason: String,
) -> Option<BrowserSession> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let record = guard.get_mut(session_id)?;
    record.session.status = BrowserSessionStatus::ChallengeRequired;
    record.session.challenge_url = Some(url);
    record.session.challenge_reason = Some(reason);
    Some(record.session.clone())
}

pub fn attach_browser_act_request(
    session_id: &str,
    act_request: serde_json::Value,
) -> Option<BrowserSession> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let record = guard.get_mut(session_id)?;
    record.session.act_request = Some(act_request);
    Some(record.session.clone())
}

pub fn complete_browser_session(
    session_id: &str,
    request: BrowserSessionCompleteRequest,
) -> Option<BrowserSession> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let record = guard.get_mut(session_id)?;
    if let Some(response) = request.search_response {
        record.session.search_response = Some(response);
        record.session.status = BrowserSessionStatus::Completed;
    } else {
        record.session.error = request.error.or(Some("browser session failed".to_string()));
        record.session.status = BrowserSessionStatus::Failed;
    }
    Some(record.session.clone())
}

/// Complete a browser session from a concrete client driver. New sessions are
/// instance-addressed; sessions created by older clients remain compatible.
pub fn complete_browser_session_for_driver(
    session_id: &str,
    world_driver_id: Option<&str>,
    request: BrowserSessionCompleteRequest,
) -> Result<Option<BrowserSession>, String> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let Some(record) = guard.get_mut(session_id) else {
        return Ok(None);
    };
    verify_world_driver(&record.session, world_driver_id)?;
    if let Some(response) = request.search_response {
        record.session.search_response = Some(response);
        record.session.status = BrowserSessionStatus::Completed;
    } else {
        record.session.error = request.error.or(Some("browser session failed".to_string()));
        record.session.status = BrowserSessionStatus::Failed;
    }
    Ok(Some(record.session.clone()))
}

/// Client-executed act completion — used by Home mobile after running the act in its overlay webview.
pub fn complete_browser_act_session(
    session_id: &str,
    outcome: BrowserActOutcome,
) -> Option<BrowserSession> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let record = guard.get_mut(session_id)?;
    record.session.act_result = Some(outcome);
    record.session.status = BrowserSessionStatus::Completed;
    Some(record.session.clone())
}

pub fn complete_browser_act_session_for_driver(
    session_id: &str,
    world_driver_id: Option<&str>,
    outcome: BrowserActOutcome,
) -> Result<Option<BrowserSession>, String> {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    let Some(record) = guard.get_mut(session_id) else {
        return Ok(None);
    };
    verify_world_driver(&record.session, world_driver_id)?;
    record.session.act_result = Some(outcome);
    record.session.status = BrowserSessionStatus::Completed;
    Ok(Some(record.session.clone()))
}

fn verify_world_driver(
    session: &BrowserSession,
    world_driver_id: Option<&str>,
) -> Result<(), String> {
    let Some(expected) = session.world_driver_id.as_deref() else {
        return Ok(());
    };
    let provided = world_driver_id.map(str::trim).filter(|value| !value.is_empty());
    if provided == Some(expected) {
        return Ok(());
    }
    Err(format!(
        "browser session {} belongs to a different world driver",
        session.session_id
    ))
}

fn purge_expired() {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    guard.retain(|_, record| record.inserted.elapsed() <= SESSION_TTL);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_completion_is_bound_to_the_creating_driver() {
        let session = create_browser_session(BrowserSessionCreateRequest {
            turn_id: "turn-driver-bound".to_string(),
            chat_session_id: "chat-driver-bound".to_string(),
            world_driver_id: Some("driver:one".to_string()),
            query: "example".to_string(),
            max_results: 1,
            client_executed: true,
        });

        let error = complete_browser_session_for_driver(
            &session.session_id,
            Some("driver:two"),
            BrowserSessionCompleteRequest {
                search_response: None,
                error: Some("failed".to_string()),
            },
        )
        .unwrap_err();
        assert!(error.contains("different world driver"));
        assert_eq!(
            get_browser_session(&session.session_id).unwrap().status,
            BrowserSessionStatus::PendingClient
        );

        let completed = complete_browser_session_for_driver(
            &session.session_id,
            Some("driver:one"),
            BrowserSessionCompleteRequest {
                search_response: None,
                error: Some("failed".to_string()),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(completed.status, BrowserSessionStatus::Failed);
    }

    #[test]
    fn legacy_session_completion_remains_compatible() {
        let session = create_browser_session(BrowserSessionCreateRequest {
            turn_id: "turn-legacy".to_string(),
            chat_session_id: "chat-legacy".to_string(),
            world_driver_id: None,
            query: "example".to_string(),
            max_results: 1,
            client_executed: true,
        });
        let completed = complete_browser_session_for_driver(
            &session.session_id,
            None,
            BrowserSessionCompleteRequest {
                search_response: None,
                error: Some("failed".to_string()),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(completed.status, BrowserSessionStatus::Failed);
    }
}
