//! Remote push to paired Medousa Home iOS devices (APNs).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use once_cell::sync::OnceCell;

use crate::channel_delivery::work_deep_link_url;
use crate::daemon_api::{HomeNotificationIntent, HomeNotificationKind};
use crate::pairing::apns::{ApnsConfig, ensure_apns_client, shared_apns_client};
use crate::pairing::{ApnsConfigSource, PairingService};

static HOME_PUSH: OnceCell<Arc<HomePushService>> = OnceCell::new();

pub fn register_home_push(service: Arc<HomePushService>) {
    let _ = HOME_PUSH.set(service);
}

pub struct HomePushService {
    pairing: Arc<PairingService>,
    apns_config: Option<ApnsConfig>,
    dedupe: Mutex<HashMap<String, Instant>>,
}

impl HomePushService {
    pub fn new(pairing: Arc<PairingService>) -> Self {
        let (apns_config, source) = ApnsConfig::load();
        match (&apns_config, source) {
            (Some(_), ApnsConfigSource::Environment) => {
                tracing::info!("home push: APNs configured (environment)");
            }
            (Some(_), ApnsConfigSource::DataDirFile) => {
                tracing::info!(
                    path = %crate::pairing::apns_config_file_path().display(),
                    "home push: APNs configured (data dir file)"
                );
            }
            (Some(_), ApnsConfigSource::DataDirKeychain) => {
                tracing::info!(
                    path = %crate::pairing::apns_config_file_path().display(),
                    "home push: APNs configured (keychain)"
                );
            }
            (None, ApnsConfigSource::None) => {
                let partial_env = std::env::var("MEDOUSA_APNS_TEAM_ID")
                    .ok()
                    .is_some_and(|value| !value.trim().is_empty());
                if partial_env {
                    tracing::info!(
                        dir = %crate::pairing::apns_config_dir().display(),
                        "home push: APNs not configured — MEDOUSA_APNS_TEAM_ID is set but config failed (check KEY_PATH / KEY_ID); restart daemon after fixing"
                    );
                } else {
                    tracing::info!(
                        dir = %crate::pairing::apns_config_dir().display(),
                        "home push: APNs not configured — install config for official builds or set MEDOUSA_APNS_* for dev"
                    );
                }
            }
            _ => {}
        }
        Self {
            pairing,
            apns_config,
            dedupe: Mutex::new(HashMap::new()),
        }
    }

    fn should_send(&self, key: &str) -> bool {
        let mut guard = self.dedupe.lock().expect("home push dedupe lock");
        let now = Instant::now();
        guard.retain(|_, at| now.duration_since(*at) < Duration::from_secs(120));
        if guard
            .get(key)
            .is_some_and(|at| now.duration_since(*at) < Duration::from_secs(30))
        {
            return false;
        }
        guard.insert(key.to_string(), now);
        true
    }

    pub async fn dispatch_intent(&self, intent: &HomeNotificationIntent) {
        if !self.should_send(&intent.notification_id) {
            return;
        }
        let card_id = intent
            .card_id
            .as_deref()
            .unwrap_or(intent.subject_id.as_str())
            .to_string();
        let message = HomePushMessage {
            title: intent.title.clone(),
            body: intent.body.clone(),
            card_id,
            kind: match intent.kind {
                HomeNotificationKind::FatalTurn => "fatal_turn",
                HomeNotificationKind::TurnUpdate => "turn_update",
                HomeNotificationKind::NeedsInput => "needs_input",
                HomeNotificationKind::ScheduledDelivery => "scheduled_delivery",
            }
            .to_string(),
            badge: Some(1),
        };
        if let Err(err) = self.send_to_paired_devices(&message).await {
            tracing::warn!(
                error = %err,
                notification_id = %intent.notification_id,
                "home notification APNs dispatch failed"
            );
        }
    }

    async fn send_to_paired_devices(&self, message: &HomePushMessage) -> Result<()> {
        let Some(config) = self.apns_config.as_ref() else {
            return Ok(());
        };
        ensure_apns_client(config).await?;

        let devices = self.pairing.list_apns_targets()?;
        if devices.is_empty() {
            return Ok(());
        }

        let client = shared_apns_client();
        let guard = client.lock().await;
        let Some(apns) = guard.as_ref() else {
            return Ok(());
        };

        let mut custom = HashMap::new();
        custom.insert("cardId", message.card_id.clone());
        custom.insert("kind", message.kind.clone());
        custom.insert("url", work_deep_link_url(&message.card_id));

        for target in devices {
            if !target_allows_kind(&target, &message.kind) {
                continue;
            }
            if let Err(err) = apns
                .send_alert(
                    &target.device_token,
                    &message.title,
                    &message.body,
                    message.badge,
                    &custom,
                )
                .await
            {
                tracing::warn!(
                    error = %err,
                    phone_id = %target.phone_id,
                    "APNs delivery failed"
                );
            }
        }
        Ok(())
    }
}

fn target_allows_kind(target: &crate::pairing::ApnsPushTarget, kind: &str) -> bool {
    match kind {
        "fatal_turn" | "scheduled_delivery" => true,
        "turn_update" => target.turn_updates_enabled,
        "needs_input" => target.needs_input_enabled,
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct HomePushMessage {
    title: String,
    body: String,
    card_id: String,
    kind: String,
    badge: Option<u32>,
}

pub fn notify_intent(intent: HomeNotificationIntent) {
    let Some(service) = HOME_PUSH.get() else {
        return;
    };
    let service = service.clone();
    tokio::spawn(async move {
        service.dispatch_intent(&intent).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apns_categories_follow_the_shared_policy() {
        let target = crate::pairing::ApnsPushTarget {
            phone_id: "phone-1".into(),
            device_token: "token".into(),
            turn_updates_enabled: false,
            needs_input_enabled: false,
            peer_messages_enabled: true,
            reminders_enabled: true,
        };
        assert!(target_allows_kind(&target, "fatal_turn"));
        assert!(target_allows_kind(&target, "scheduled_delivery"));
        assert!(!target_allows_kind(&target, "turn_update"));
        assert!(!target_allows_kind(&target, "needs_input"));
        assert!(!target_allows_kind(&target, "worker_started"));
    }
}
