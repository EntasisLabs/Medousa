//! Remote push to paired Medousa Home iOS devices (APNs).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use once_cell::sync::OnceCell;

use crate::channel_delivery::work_deep_link_url;
use crate::daemon_api::{WorkBoardColumn, WorkCardDetail, WorkCardKind};
use crate::pairing::apns::{ensure_apns_client, shared_apns_client, ApnsConfig};
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
                tracing::info!(
                    dir = %crate::pairing::apns_config_dir().display(),
                    "home push: APNs not configured — install config for official builds or set MEDOUSA_APNS_* for dev"
                );
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

    pub async fn dispatch_column_transition(
        &self,
        detail: &WorkCardDetail,
        previous: Option<WorkBoardColumn>,
        current: WorkBoardColumn,
    ) {
        let Some(message) = compose_column_push(detail, previous, current) else {
            return;
        };
        let dedupe_key = format!("{}:{}:{}", detail.card.id.0, message.kind, current as u8);
        if !self.should_send(&dedupe_key) {
            return;
        }
        if let Err(err) = self.send_to_paired_devices(&message).await {
            tracing::warn!(error = %err, card_id = %detail.card.id.0, "home push dispatch failed");
        }
    }

    pub async fn dispatch_budget_approval(
        &self,
        request_id: &str,
        title: &str,
        detail: &str,
    ) {
        let card_id = request_id.trim();
        if card_id.is_empty() {
            return;
        }
        let dedupe_key = format!("budget:{card_id}");
        if !self.should_send(&dedupe_key) {
            return;
        }
        let body = if detail.len() > 160 {
            format!("{}…", &detail[..157])
        } else {
            detail.to_string()
        };
        let message = HomePushMessage {
            title: "Medousa — approve more rounds?".to_string(),
            body: format!("{body} · tap to review"),
            card_id: card_id.to_string(),
            kind: "budget".to_string(),
            badge: Some(1),
        };
        if let Err(err) = self.send_to_paired_devices(&message).await {
            tracing::warn!(error = %err, request_id, "home push budget dispatch failed");
        }
        let _ = title;
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

#[derive(Debug, Clone)]
struct HomePushMessage {
    title: String,
    body: String,
    card_id: String,
    kind: String,
    badge: Option<u32>,
}

fn compose_column_push(
    detail: &WorkCardDetail,
    previous: Option<WorkBoardColumn>,
    current: WorkBoardColumn,
) -> Option<HomePushMessage> {
    let card_id = detail.card.id.0.clone();
    let line = crate::workspace::event::resolve_detail_line(detail);
    let status = detail.card.status_label.trim();

    match (detail.kind, previous, current) {
        (WorkCardKind::AskJob, _, WorkBoardColumn::Done) => Some(HomePushMessage {
            title: "Medousa — ask ready".to_string(),
            body: format!("{line} · tap to read the result"),
            card_id,
            kind: "work".to_string(),
            badge: Some(1),
        }),
        (WorkCardKind::TurnBudgetRequest, _, WorkBoardColumn::Blocked) => Some(HomePushMessage {
            title: "Medousa — approve more rounds?".to_string(),
            body: format!("{line} · tap to review"),
            card_id,
            kind: "budget".to_string(),
            badge: Some(1),
        }),
        (_, _, WorkBoardColumn::Done) if detail.terminal => Some(HomePushMessage {
            title: "Medousa — work finished".to_string(),
            body: format!("{line} · {status}"),
            card_id,
            kind: "work".to_string(),
            badge: Some(1),
        }),
        (_, _, WorkBoardColumn::Blocked) => Some(HomePushMessage {
            title: "Medousa — needs you".to_string(),
            body: format!("{line} · {status}"),
            card_id,
            kind: "work".to_string(),
            badge: Some(1),
        }),
        (WorkCardKind::TurnWorker, Some(WorkBoardColumn::Backlog), WorkBoardColumn::InFlight) => {
            Some(HomePushMessage {
                title: "Medousa — worker started".to_string(),
                body: line,
                card_id,
                kind: "work".to_string(),
                badge: None,
            })
        }
        _ => None,
    }
}

pub fn notify_column_transition(
    detail: &WorkCardDetail,
    previous: Option<WorkBoardColumn>,
    current: WorkBoardColumn,
) {
    let Some(service) = HOME_PUSH.get() else {
        return;
    };
    let service = service.clone();
    let detail = detail.clone();
    tokio::spawn(async move {
        service
            .dispatch_column_transition(&detail, previous, current)
            .await;
    });
}

pub async fn notify_budget_approval(request_id: &str, title: &str, detail: &str) {
    let Some(service) = HOME_PUSH.get() else {
        return;
    };
    service
        .dispatch_budget_approval(request_id, title, detail)
        .await;
}
