use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub use medousa_types::turn_ticket::{TurnTicket, TurnTicketConflict, TurnTicketMode, TurnTicketPhase};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnTicketResponse {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub accepted_at_utc: DateTime<Utc>,
    pub stream_url: String,
    pub stream_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionActiveTurnsResponse {
    pub session_id: String,
    pub turns: Vec<TurnTicket>,
}

/// Tier 1 compat — primary interactive turn for reconnect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSessionTurn {
    pub turn_id: String,
    pub session_id: String,
    pub stream_url: String,
    pub phase: String,
    pub composer_handoff: bool,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSessionTurnResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<ActiveSessionTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelActiveSessionTurnResponse {
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub message: String,
}

pub struct TurnTicketRegistryInner {
    by_id: HashMap<String, TurnTicket>,
    interactive_by_session: HashMap<String, String>,
}

pub type TurnTicketRegistry = Arc<RwLock<TurnTicketRegistryInner>>;

pub fn new_registry() -> TurnTicketRegistry {
    Arc::new(RwLock::new(TurnTicketRegistryInner {
        by_id: HashMap::new(),
        interactive_by_session: HashMap::new(),
    }))
}

pub fn prompt_preview(prompt: &str) -> String {
    let line = prompt.trim().lines().next().unwrap_or("").trim();
    if line.len() <= 96 {
        line.to_string()
    } else {
        format!("{}…", &line[..95])
    }
}

pub fn phase_from_stream_event(event_type: &str, terminal: bool) -> TurnTicketPhase {
    match event_type {
        "worker_ack" => TurnTicketPhase::WorkerHandoff,
        "budget_approval" => TurnTicketPhase::BudgetBlocked,
        "error" => TurnTicketPhase::Error,
        _ if terminal => TurnTicketPhase::Done,
        _ => TurnTicketPhase::Streaming,
    }
}

pub async fn register_turn(
    registry: &TurnTicketRegistry,
    ticket: TurnTicket,
) -> Result<(), TurnTicketConflict> {
    let mut guard = registry.write().await;
    if ticket.mode == TurnTicketMode::Interactive {
        if let Some(existing_id) = guard.interactive_by_session.get(&ticket.session_id) {
            let allow_fork = guard
                .by_id
                .get(existing_id)
                .is_some_and(|existing| existing.phase.composer_handoff());
            if !allow_fork {
                return Err(TurnTicketConflict {
                    message: "session already has an active interactive turn".to_string(),
                });
            }
        }
        guard
            .interactive_by_session
            .insert(ticket.session_id.clone(), ticket.turn_id.clone());
    }
    guard.by_id.insert(ticket.turn_id.clone(), ticket);
    Ok(())
}

pub async fn note_stream_event(
    registry: &TurnTicketRegistry,
    turn_id: &str,
    event_type: &str,
    _phase: &str,
    terminal: bool,
) {
    let next = phase_from_stream_event(event_type, terminal);
    let mut guard = registry.write().await;
    let Some(ticket) = guard.by_id.get_mut(turn_id) else {
        return;
    };
    ticket.phase = next;
    ticket.updated_at = Utc::now();
    if next.terminal() && ticket.mode == TurnTicketMode::Interactive {
        let session_id = ticket.session_id.clone();
        if guard
            .interactive_by_session
            .get(&session_id)
            .is_some_and(|active| active == turn_id)
        {
            guard.interactive_by_session.remove(&session_id);
        }
    }
}

pub async fn mark_cancelled(registry: &TurnTicketRegistry, turn_id: &str) {
    let mut guard = registry.write().await;
    let Some(ticket) = guard.by_id.get_mut(turn_id) else {
        return;
    };
    ticket.phase = TurnTicketPhase::Cancelled;
    ticket.updated_at = Utc::now();
    if ticket.mode == TurnTicketMode::Interactive {
        let session_id = ticket.session_id.clone();
        guard.interactive_by_session.remove(&session_id);
    }
}

pub async fn clear_turn(registry: &TurnTicketRegistry, turn_id: &str) {
    let mut guard = registry.write().await;
    if let Some(ticket) = guard.by_id.remove(turn_id) {
        if ticket.mode == TurnTicketMode::Interactive {
            guard.interactive_by_session.remove(&ticket.session_id);
        }
    }
}

/// Drop the turn ticket after the orchestrator returns unless handoff is still active.
pub async fn clear_turn_after_run(registry: &TurnTicketRegistry, turn_id: &str) {
    let keep = get_turn(registry, turn_id)
        .await
        .is_some_and(|ticket| {
            matches!(
                ticket.phase,
                TurnTicketPhase::WorkerHandoff | TurnTicketPhase::BudgetBlocked
            )
        });
    if !keep {
        clear_turn(registry, turn_id).await;
    }
}

pub async fn get_turn(
    registry: &TurnTicketRegistry,
    turn_id: &str,
) -> Option<TurnTicket> {
    registry.read().await.by_id.get(turn_id).cloned()
}

pub async fn list_active_for_session(
    registry: &TurnTicketRegistry,
    session_id: &str,
) -> Vec<TurnTicket> {
    registry
        .read()
        .await
        .by_id
        .values()
        .filter(|ticket| ticket.session_id == session_id && !ticket.phase.terminal())
        .cloned()
        .collect()
}

pub async fn get_active_interactive_turn(
    registry: &TurnTicketRegistry,
    session_id: &str,
) -> ActiveSessionTurnResponse {
    let guard = registry.read().await;
    let turn_id = guard.interactive_by_session.get(session_id);
    let Some(turn_id) = turn_id else {
        return ActiveSessionTurnResponse {
            active: false,
            turn: None,
        };
    };
    let Some(ticket) = guard.by_id.get(turn_id) else {
        return ActiveSessionTurnResponse {
            active: false,
            turn: None,
        };
    };
    ActiveSessionTurnResponse {
        active: true,
        turn: Some(active_session_turn_from_ticket(ticket)),
    }
}

pub async fn cancel_interactive_for_session(
    registry: &TurnTicketRegistry,
    session_id: &str,
) -> Option<TurnTicket> {
    let mut guard = registry.write().await;
    let turn_id = guard.interactive_by_session.remove(session_id)?;
    let ticket = guard.by_id.get_mut(&turn_id)?;
    ticket.phase = TurnTicketPhase::Cancelled;
    ticket.updated_at = Utc::now();
    Some(ticket.clone())
}

fn active_session_turn_from_ticket(ticket: &TurnTicket) -> ActiveSessionTurn {
    ActiveSessionTurn {
        turn_id: ticket.turn_id.clone(),
        session_id: ticket.session_id.clone(),
        stream_url: ticket.stream_url.clone(),
        phase: ticket.phase.as_str().to_string(),
        composer_handoff: ticket.composer_handoff(),
        started_at: ticket.started_at,
    }
}

// ── Tier 1 compat aliases ─────────────────────────────────────────────────────

pub type ActiveSessionTurnRegistry = TurnTicketRegistry;

pub async fn register_active_turn(
    registry: &TurnTicketRegistry,
    session_id: &str,
    turn_id: &str,
    stream_url: &str,
) {
    let _ = register_turn(
        registry,
        TurnTicket {
            turn_id: turn_id.to_string(),
            session_id: session_id.to_string(),
            mode: TurnTicketMode::Interactive,
            phase: TurnTicketPhase::Streaming,
            stream_url: stream_url.to_string(),
            prompt_preview: String::new(),
            workspace_card_id: None,
            started_at: Utc::now(),
            updated_at: Utc::now(),
        },
    )
    .await;
}

pub async fn clear_active_turn(registry: &TurnTicketRegistry, session_id: &str) {
    let turn_id = registry
        .read()
        .await
        .interactive_by_session
        .get(session_id)
        .cloned();
    if let Some(turn_id) = turn_id {
        clear_turn(registry, &turn_id).await;
    }
}

pub async fn clear_active_turn_by_turn_id(registry: &TurnTicketRegistry, turn_id: &str) {
    clear_turn(registry, turn_id).await;
}

pub async fn get_active_turn(
    registry: &TurnTicketRegistry,
    session_id: &str,
) -> ActiveSessionTurnResponse {
    get_active_interactive_turn(registry, session_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interactive_mutex_and_stream_phases() {
        let registry = new_registry();
        register_turn(
            &registry,
            TurnTicket {
                turn_id: "turn-1".to_string(),
                session_id: "session-a".to_string(),
                mode: TurnTicketMode::Interactive,
                phase: TurnTicketPhase::Streaming,
                stream_url: "http://localhost/stream".to_string(),
                prompt_preview: "hello".to_string(),
                workspace_card_id: None,
                started_at: Utc::now(),
                updated_at: Utc::now(),
            },
        )
        .await
        .expect("register");

        let conflict = register_turn(
            &registry,
            TurnTicket {
                turn_id: "turn-2".to_string(),
                session_id: "session-a".to_string(),
                mode: TurnTicketMode::Interactive,
                phase: TurnTicketPhase::Streaming,
                stream_url: "http://localhost/stream2".to_string(),
                prompt_preview: "again".to_string(),
                workspace_card_id: None,
                started_at: Utc::now(),
                updated_at: Utc::now(),
            },
        )
        .await;
        assert!(conflict.is_err());

        note_stream_event(&registry, "turn-1", "worker_ack", "worker_ack", false).await;
        let active = get_active_interactive_turn(&registry, "session-a").await;
        let turn = active.turn.expect("turn");
        assert!(turn.composer_handoff);

        register_turn(
            &registry,
            TurnTicket {
                turn_id: "turn-2".to_string(),
                session_id: "session-a".to_string(),
                mode: TurnTicketMode::Interactive,
                phase: TurnTicketPhase::Streaming,
                stream_url: "http://localhost/stream2".to_string(),
                prompt_preview: "fork while worker runs".to_string(),
                workspace_card_id: None,
                started_at: Utc::now(),
                updated_at: Utc::now(),
            },
        )
        .await
        .expect("fork during worker handoff");

        let forked = get_active_interactive_turn(&registry, "session-a").await;
        assert_eq!(forked.turn.as_ref().map(|t| t.turn_id.as_str()), Some("turn-2"));

        note_stream_event(&registry, "turn-1", "final", "final", true).await;
        assert!(get_active_interactive_turn(&registry, "session-a").await.active);
    }

    #[tokio::test]
    async fn background_turns_allow_multiple_per_session() {
        let registry = new_registry();
        for id in ["bg-1", "bg-2"] {
            register_turn(
                &registry,
                TurnTicket {
                    turn_id: id.to_string(),
                    session_id: "session-a".to_string(),
                    mode: TurnTicketMode::Background,
                    phase: TurnTicketPhase::Streaming,
                    stream_url: format!("http://localhost/{id}"),
                    prompt_preview: "ask".to_string(),
                    workspace_card_id: Some(id.to_string()),
                    started_at: Utc::now(),
                    updated_at: Utc::now(),
                },
            )
            .await
            .expect("register");
        }

        let active = list_active_for_session(&registry, "session-a").await;
        assert_eq!(active.len(), 2);
    }

    #[tokio::test]
    async fn worker_ack_stream_event_is_non_terminal_handoff() {
        let event = crate::interactive_turn_runtime::worker_ack_stream_event_with_tools(
            "turn-1",
            "On it.",
            vec!["cognition_spawn_turn_worker".to_string()],
            Some("work-1"),
        )
        .expect("event");
        assert_eq!(event.event_type, "worker_ack");
        assert_eq!(event.phase, "worker_ack");
        assert!(!event.terminal);
        assert_eq!(event.work_id.as_deref(), Some("work-1"));
    }

    #[tokio::test]
    async fn clear_turn_after_run_keeps_worker_handoff_ticket() {
        let registry = new_registry();
        register_turn(
            &registry,
            TurnTicket {
                turn_id: "turn-1".to_string(),
                session_id: "session-a".to_string(),
                mode: TurnTicketMode::Interactive,
                phase: TurnTicketPhase::Streaming,
                stream_url: "http://localhost/stream".to_string(),
                prompt_preview: "hello".to_string(),
                workspace_card_id: None,
                started_at: Utc::now(),
                updated_at: Utc::now(),
            },
        )
        .await
        .expect("register");

        note_stream_event(&registry, "turn-1", "worker_ack", "worker_ack", false).await;
        clear_turn_after_run(&registry, "turn-1").await;

        let active = get_active_interactive_turn(&registry, "session-a").await;
        assert!(active.active);
        let turn = active.turn.expect("turn");
        assert_eq!(turn.phase, "worker_handoff");
        assert!(turn.composer_handoff);
    }
}
