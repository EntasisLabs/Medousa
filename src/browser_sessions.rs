//! In-process browser sessions for client-executed search (home-ios) and CAPTCHA handoff.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use medousa_browser_lite::SearchResponse;

const SESSION_TTL: Duration = Duration::from_secs(15 * 60);

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
    pub query: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    pub status: BrowserSessionStatus,
    pub challenge_url: Option<String>,
    pub challenge_reason: Option<String>,
    pub search_response: Option<SearchResponse>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionCreateRequest {
    pub turn_id: String,
    pub chat_session_id: String,
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
    let status = if request.client_executed {
        BrowserSessionStatus::PendingClient
    } else {
        BrowserSessionStatus::PendingClient
    };
    let session = BrowserSession {
        session_id: session_id.clone(),
        turn_id: request.turn_id,
        chat_session_id: request.chat_session_id,
        query: request.query,
        max_results: request.max_results,
        status,
        challenge_url: None,
        challenge_reason: None,
        search_response: None,
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

fn purge_expired() {
    let mut guard = SESSIONS.lock().expect("browser sessions");
    guard.retain(|_, record| record.inserted.elapsed() <= SESSION_TTL);
}
