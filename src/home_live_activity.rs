//! Remote Live Activity updates to paired iOS devices while the app is backgrounded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use once_cell::sync::OnceCell;

use crate::daemon_api::{WorkBoardColumn, WorkCard};
use crate::pairing::apns::{
    ensure_apns_client, shared_apns_client, ApnsConfig, LiveActivityContentState,
};
use crate::pairing::PairingService;
use crate::workspace::projector::WorkspaceReadSnapshot;

static HOME_LIVE_ACTIVITY: OnceCell<Arc<HomeLiveActivityService>> = OnceCell::new();

pub fn register_home_live_activity(service: Arc<HomeLiveActivityService>) {
    let _ = HOME_LIVE_ACTIVITY.set(service);
}

pub struct HomeLiveActivityService {
    pairing: Arc<PairingService>,
    apns_config: Option<ApnsConfig>,
    last_state_key: Mutex<HashMap<String, String>>,
    dedupe: Mutex<HashMap<String, Instant>>,
}

impl HomeLiveActivityService {
    pub fn new(pairing: Arc<PairingService>) -> Self {
        let (apns_config, _source) = ApnsConfig::load();
        Self {
            pairing,
            apns_config,
            last_state_key: Mutex::new(HashMap::new()),
            dedupe: Mutex::new(HashMap::new()),
        }
    }

    fn should_send(&self, phone_id: &str, key: &str) -> bool {
        let mut guard = self.dedupe.lock().expect("home live activity dedupe lock");
        let now = Instant::now();
        guard.retain(|_, at| now.duration_since(*at) < Duration::from_secs(120));
        let dedupe_key = format!("{phone_id}:{key}");
        if guard
            .get(&dedupe_key)
            .is_some_and(|at| now.duration_since(*at) < Duration::from_secs(2))
        {
            return false;
        }
        guard.insert(dedupe_key, now);
        true
    }

    pub async fn dispatch_snapshot(&self, snapshot: &WorkspaceReadSnapshot) {
        let Some(state) = compose_live_activity_state(snapshot) else {
            self.dispatch_end().await;
            return;
        };
        let state_key = state_key(&state);
        let targets = match self.pairing.list_live_activity_targets() {
            Ok(targets) => targets,
            Err(err) => {
                tracing::warn!(error = %err, "home live activity target lookup failed");
                return;
            }
        };
        if targets.is_empty() {
            return;
        }

        {
            let mut guard = self
                .last_state_key
                .lock()
                .expect("home live activity state lock");
            let mut unchanged = true;
            for target in &targets {
                if guard.get(&target.phone_id) != Some(&state_key) {
                    unchanged = false;
                    break;
                }
            }
            if unchanged {
                return;
            }
        }

        if let Err(err) = self.send_update(&state, &targets).await {
            tracing::warn!(error = %err, "home live activity update failed");
            return;
        }

        let mut guard = self
            .last_state_key
            .lock()
            .expect("home live activity state lock");
        for target in targets {
            guard.insert(target.phone_id, state_key.clone());
        }
    }

    async fn dispatch_end(&self) {
        let targets = match self.pairing.list_live_activity_targets() {
            Ok(targets) => targets,
            Err(_) => return,
        };
        if targets.is_empty() {
            return;
        }
        if let Err(err) = self.send_end(None, &targets).await {
            tracing::warn!(error = %err, "home live activity end failed");
            return;
        }
        let mut guard = self
            .last_state_key
            .lock()
            .expect("home live activity state lock");
        for target in targets {
            guard.remove(&target.phone_id);
        }
    }

    async fn send_update(
        &self,
        state: &LiveActivityContentState,
        targets: &[crate::pairing::LiveActivityPushTarget],
    ) -> Result<()> {
        let Some(config) = self.apns_config.as_ref() else {
            return Ok(());
        };
        ensure_apns_client(config).await?;

        let client = shared_apns_client();
        let guard = client.lock().await;
        let Some(apns) = guard.as_ref() else {
            return Ok(());
        };

        for target in targets {
            if !self.should_send(&target.phone_id, &state_key(state)) {
                continue;
            }
            if let Err(err) = apns
                .send_live_activity_update(&target.push_token, state)
                .await
            {
                tracing::warn!(
                    error = %err,
                    phone_id = %target.phone_id,
                    "Live Activity APNs update failed"
                );
            }
        }
        Ok(())
    }

    async fn send_end(
        &self,
        state: Option<&LiveActivityContentState>,
        targets: &[crate::pairing::LiveActivityPushTarget],
    ) -> Result<()> {
        let Some(config) = self.apns_config.as_ref() else {
            return Ok(());
        };
        ensure_apns_client(config).await?;

        let client = shared_apns_client();
        let guard = client.lock().await;
        let Some(apns) = guard.as_ref() else {
            return Ok(());
        };

        for target in targets {
            if !self.should_send(&target.phone_id, "end") {
                continue;
            }
            if let Err(err) = apns
                .send_live_activity_end(&target.push_token, state)
                .await
            {
                tracing::warn!(
                    error = %err,
                    phone_id = %target.phone_id,
                    "Live Activity APNs end failed"
                );
            }
        }
        Ok(())
    }
}

pub fn notify_snapshot(snapshot: &Arc<WorkspaceReadSnapshot>) {
    let Some(service) = HOME_LIVE_ACTIVITY.get() else {
        return;
    };
    let service = service.clone();
    let snapshot = snapshot.clone();
    tokio::spawn(async move {
        service.dispatch_snapshot(&snapshot).await;
    });
}

fn compose_live_activity_state(
    snapshot: &WorkspaceReadSnapshot,
) -> Option<LiveActivityContentState> {
    let blocked = blocked_count(snapshot);
    let in_flight = column_count(snapshot, WorkBoardColumn::InFlight);
    let wrapping = column_count(snapshot, WorkBoardColumn::WrappingUp);
    let backlog = column_count(snapshot, WorkBoardColumn::Backlog);
    let in_motion = in_flight + wrapping + backlog;
    let motion_summary = build_motion_summary(in_flight, wrapping, backlog);
    let primary_card = primary_in_motion_card(snapshot);

    if blocked > 0 {
        let noun = if blocked == 1 { "decision" } else { "decisions" };
        return Some(LiveActivityContentState {
            mood: "waiting".to_string(),
            eyebrow: "Needs you".to_string(),
            headline: if blocked == 1 {
                "One thing needs a decision".to_string()
            } else {
                format!("{blocked} {noun} waiting")
            },
            subline: Some("Answer when you're ready — the rest keeps moving.".to_string()),
            motion_summary: motion_summary.clone(),
            blocked_count: blocked,
            primary_card_id: None,
        });
    }

    if let Some(card) = primary_card {
        return Some(LiveActivityContentState {
            mood: "working".to_string(),
            eyebrow: human_work_column(card.column).to_string(),
            headline: card_headline(card),
            subline: motion_summary.clone(),
            motion_summary,
            blocked_count: 0,
            primary_card_id: Some(card.id.0.clone()),
        });
    }

    if in_motion > 0 {
        return Some(LiveActivityContentState {
            mood: "working".to_string(),
            eyebrow: "In motion".to_string(),
            headline: if in_motion == 1 {
                "One job is running".to_string()
            } else {
                format!("{in_motion} jobs running")
            },
            subline: motion_summary.clone(),
            motion_summary,
            blocked_count: 0,
            primary_card_id: None,
        });
    }

    None
}

fn state_key(state: &LiveActivityContentState) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        state.mood,
        state.eyebrow,
        state.headline,
        state.subline.as_deref().unwrap_or(""),
        state.motion_summary.as_deref().unwrap_or(""),
        state.blocked_count,
        state.primary_card_id.as_deref().unwrap_or(""),
    )
}

fn column_count(snapshot: &WorkspaceReadSnapshot, column: WorkBoardColumn) -> u32 {
    let key = column_key(column);
    snapshot.counts_by_column.get(key).copied().unwrap_or(0)
}

fn column_key(column: WorkBoardColumn) -> &'static str {
    match column {
        WorkBoardColumn::Backlog => "backlog",
        WorkBoardColumn::InFlight => "in_flight",
        WorkBoardColumn::WrappingUp => "wrapping_up",
        WorkBoardColumn::Done => "done",
        WorkBoardColumn::Blocked => "blocked",
    }
}

fn blocked_count(snapshot: &WorkspaceReadSnapshot) -> u32 {
    snapshot
        .cards
        .iter()
        .filter(|card| card.column == WorkBoardColumn::Blocked && !is_terminal_status(card))
        .count() as u32
}

fn is_terminal_status(card: &WorkCard) -> bool {
    matches!(
        card.status_label.as_str(),
        "failed" | "canceled" | "dead_letter"
    )
}

fn primary_in_motion_card(snapshot: &WorkspaceReadSnapshot) -> Option<&WorkCard> {
    for column in [
        WorkBoardColumn::InFlight,
        WorkBoardColumn::WrappingUp,
        WorkBoardColumn::Backlog,
    ] {
        if let Some(card) = snapshot.cards.iter().find(|card| card.column == column) {
            return Some(card);
        }
    }
    None
}

fn build_motion_summary(in_flight: u32, wrapping: u32, backlog: u32) -> Option<String> {
    let mut parts = Vec::new();
    if in_flight > 0 {
        parts.push(format!("{in_flight} running"));
    }
    if wrapping > 0 {
        parts.push(format!("{wrapping} finishing"));
    }
    if backlog > 0 {
        parts.push(format!("{backlog} queued"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn human_work_column(column: WorkBoardColumn) -> &'static str {
    match column {
        WorkBoardColumn::InFlight => "Running",
        WorkBoardColumn::WrappingUp => "Finishing up",
        WorkBoardColumn::Backlog => "Queued",
        WorkBoardColumn::Blocked => "Needs you",
        WorkBoardColumn::Done => "Done",
    }
}

fn card_headline(card: &WorkCard) -> String {
    let title = card.title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    match card.column {
        WorkBoardColumn::InFlight => "Working on your request".to_string(),
        WorkBoardColumn::WrappingUp => "Pulling it together".to_string(),
        WorkBoardColumn::Backlog => "Queued up".to_string(),
        WorkBoardColumn::Blocked => "Waiting on you".to_string(),
        WorkBoardColumn::Done => "Done".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use chrono::Utc;

    fn sample_card(column: WorkBoardColumn, title: &str) -> WorkCard {
        WorkCard {
            id: crate::daemon_api::WorkCardId("card-1".to_string()),
            column,
            title: title.to_string(),
            status_label: "running".to_string(),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
        }
    }

    #[test]
    fn compose_state_for_running_card() {
        let cards = vec![sample_card(WorkBoardColumn::InFlight, "Fix deploy script")];
        let snapshot = WorkspaceReadSnapshot {
            revision: 1,
            items: Arc::new(HashMap::new()),
            cards: Arc::new(cards),
            counts_by_column: HashMap::from([
                ("in_flight".to_string(), 1),
                ("backlog".to_string(), 0),
            ]),
        };
        let state = compose_live_activity_state(&snapshot).expect("state");
        assert_eq!(state.mood, "working");
        assert_eq!(state.headline, "Fix deploy script");
        assert_eq!(state.primary_card_id.as_deref(), Some("card-1"));
    }

    #[test]
    fn compose_state_none_when_idle() {
        let snapshot = WorkspaceReadSnapshot {
            revision: 1,
            items: Arc::new(HashMap::new()),
            cards: Arc::new(Vec::new()),
            counts_by_column: HashMap::new(),
        };
        assert!(compose_live_activity_state(&snapshot).is_none());
    }
}
