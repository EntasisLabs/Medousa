//! Silent APNs pushes to refresh the Medousa Home iOS widget while backgrounded.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use once_cell::sync::OnceCell;
use serde::Serialize;

use crate::home_live_activity::compose_live_activity_state;
use crate::pairing::apns::{ensure_apns_client, shared_apns_client, ApnsConfig};
use crate::pairing::PairingService;
use crate::workspace::projector::WorkspaceReadSnapshot;

static HOME_WIDGET_PUSH: OnceCell<Arc<HomeWidgetPushService>> = OnceCell::new();

pub fn register_home_widget_push(service: Arc<HomeWidgetPushService>) {
    let _ = HOME_WIDGET_PUSH.set(service);
}

pub struct HomeWidgetPushService {
    pairing: Arc<PairingService>,
    apns_config: Option<ApnsConfig>,
    last_state_key: Mutex<String>,
    dedupe: Mutex<HashMap<String, Instant>>,
}

impl HomeWidgetPushService {
    pub fn new(pairing: Arc<PairingService>) -> Self {
        let (apns_config, _source) = ApnsConfig::load();
        Self {
            pairing,
            apns_config,
            last_state_key: Mutex::new(String::new()),
            dedupe: Mutex::new(HashMap::new()),
        }
    }

    fn should_send(&self, phone_id: &str) -> bool {
        let mut guard = self.dedupe.lock().expect("home widget push dedupe lock");
        let now = Instant::now();
        guard.retain(|_, at| now.duration_since(*at) < Duration::from_secs(120));
        if guard
            .get(phone_id)
            .is_some_and(|at| now.duration_since(*at) < Duration::from_secs(5))
        {
            return false;
        }
        guard.insert(phone_id.to_string(), now);
        true
    }

    pub async fn dispatch_snapshot(&self, snapshot: &WorkspaceReadSnapshot) {
        let pulse = compose_widget_pulse(snapshot);
        let state_key = widget_pulse_key(&pulse);
        {
            let mut guard = self
                .last_state_key
                .lock()
                .expect("home widget push state lock");
            if *guard == state_key {
                return;
            }
            *guard = state_key.clone();
        }

        let pulse_json = match serde_json::to_string(&pulse) {
            Ok(json) => json,
            Err(err) => {
                tracing::warn!(error = %err, "home widget push encode failed");
                return;
            }
        };

        let targets = match self.pairing.list_apns_targets() {
            Ok(targets) => targets,
            Err(err) => {
                tracing::warn!(error = %err, "home widget push target lookup failed");
                return;
            }
        };
        if targets.is_empty() {
            return;
        }

        if let Err(err) = self.send_to_targets(&pulse_json, &targets).await {
            tracing::warn!(error = %err, "home widget push dispatch failed");
        }
    }

    async fn send_to_targets(
        &self,
        pulse_json: &str,
        targets: &[crate::pairing::ApnsPushTarget],
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
            if !self.should_send(&target.phone_id) {
                continue;
            }
            if let Err(err) = apns
                .send_silent_widget_pulse(&target.device_token, pulse_json)
                .await
            {
                tracing::warn!(
                    error = %err,
                    phone_id = %target.phone_id,
                    "home widget silent push failed"
                );
            }
        }
        Ok(())
    }
}

pub fn notify_snapshot(snapshot: &Arc<WorkspaceReadSnapshot>) {
    let Some(service) = HOME_WIDGET_PUSH.get() else {
        return;
    };
    let service = service.clone();
    let snapshot = snapshot.clone();
    tokio::spawn(async move {
        service.dispatch_snapshot(&snapshot).await;
    });
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetPulsePushPayload {
    mood: String,
    workshop_name: String,
    eyebrow: String,
    headline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    motion_summary: Option<String>,
    blocked_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_card_id: Option<String>,
}

fn compose_widget_pulse(snapshot: &WorkspaceReadSnapshot) -> WidgetPulsePushPayload {
    if let Some(state) = compose_live_activity_state(snapshot) {
        return WidgetPulsePushPayload {
            mood: state.mood,
            workshop_name: "Workshop".to_string(),
            eyebrow: state.eyebrow,
            headline: state.headline,
            subline: state.subline,
            motion_summary: state.motion_summary,
            blocked_count: state.blocked_count,
            primary_card_id: state.primary_card_id,
        };
    }

    WidgetPulsePushPayload {
        mood: "quiet".to_string(),
        workshop_name: "Workshop".to_string(),
        eyebrow: "Quiet".to_string(),
        headline: "Nothing in motion".to_string(),
        subline: Some("Open Medousa to check in".to_string()),
        motion_summary: None,
        blocked_count: 0,
        primary_card_id: None,
    }
}

fn widget_pulse_key(pulse: &WidgetPulsePushPayload) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        pulse.mood,
        pulse.workshop_name,
        pulse.eyebrow,
        pulse.headline,
        pulse.subline.as_deref().unwrap_or(""),
        pulse.motion_summary.as_deref().unwrap_or(""),
        pulse.blocked_count,
        pulse.primary_card_id.as_deref().unwrap_or(""),
    )
}
