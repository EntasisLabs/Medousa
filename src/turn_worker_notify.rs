//! External-channel notifications when a turn worker spawns or delivers synthesis.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use reqwest::Client;

use crate::agent_runtime::format_channel_delivery_text;
use crate::agent_runtime::turn_worker::TurnWorkRecord;
use crate::channel_delivery::{
    self, ChannelDeliveryTarget, JobDeliveryRecord, JobDeliveryState, is_external_push_channel,
    is_home_channel,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnWorkerSpawnNotifyPayload {
    pub work_id: String,
    pub user_ack: String,
    pub intent: Option<String>,
}

pub fn compose_turn_worker_spawn_text(payload: &TurnWorkerSpawnNotifyPayload) -> String {
    payload.user_ack.trim().to_string()
}

/// Push spawn acknowledgement to external messaging channels (Telegram/Discord/Slack/WhatsApp).
/// Home surfaces rely on SSE + workspace cards; TUI uses obs events.
pub async fn notify_turn_worker_spawned(
    client: &Client,
    delivery_target: &ChannelDeliveryTarget,
    payload: TurnWorkerSpawnNotifyPayload,
) -> Result<()> {
    if is_home_channel(&delivery_target.channel) {
        return Ok(());
    }
    if !is_external_push_channel(&delivery_target.channel) {
        return Ok(());
    }

    let text = compose_turn_worker_spawn_text(&payload);
    if text.is_empty() {
        return Ok(());
    }
    channel_delivery::dispatch_channel_message(client, delivery_target, &text).await
}

#[derive(Clone)]
pub struct IngestChannelDeliveryBridge {
    dispatch_client: Client,
    delivery_records: Arc<tokio::sync::RwLock<HashMap<String, JobDeliveryRecord>>>,
    channel_deliveries: Arc<tokio::sync::RwLock<HashMap<String, ChannelDeliveryTarget>>>,
    last_delivery_at: Arc<tokio::sync::RwLock<Option<DateTime<Utc>>>>,
    last_delivery_latency_ms: Arc<tokio::sync::RwLock<Option<u64>>>,
}

impl IngestChannelDeliveryBridge {
    pub fn new(
        dispatch_client: Client,
        delivery_records: Arc<tokio::sync::RwLock<HashMap<String, JobDeliveryRecord>>>,
        channel_deliveries: Arc<tokio::sync::RwLock<HashMap<String, ChannelDeliveryTarget>>>,
        last_delivery_at: Arc<tokio::sync::RwLock<Option<DateTime<Utc>>>>,
        last_delivery_latency_ms: Arc<tokio::sync::RwLock<Option<u64>>>,
    ) -> Self {
        Self {
            dispatch_client,
            delivery_records,
            channel_deliveries,
            last_delivery_at,
            last_delivery_latency_ms,
        }
    }
}

static INGEST_CHANNEL_DELIVERY_BRIDGE: Lazy<Mutex<Option<IngestChannelDeliveryBridge>>> =
    Lazy::new(|| Mutex::new(None));

pub fn register_ingest_channel_delivery_bridge(bridge: IngestChannelDeliveryBridge) {
    *INGEST_CHANNEL_DELIVERY_BRIDGE.lock().expect("ingest bridge lock") = Some(bridge);
}

fn ingest_channel_delivery_bridge() -> Option<IngestChannelDeliveryBridge> {
    INGEST_CHANNEL_DELIVERY_BRIDGE
        .lock()
        .expect("ingest bridge lock")
        .clone()
}

fn delivery_target_for_record(record: &TurnWorkRecord) -> Option<ChannelDeliveryTarget> {
    record
        .delivery_target
        .as_ref()
        .map(ChannelDeliveryTarget::from)
}

fn ingest_job_id_for_record(record: &TurnWorkRecord) -> String {
    record
        .parent_turn_correlation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| record.work_id.clone())
}

async fn mark_ingest_job_delivered(
    bridge: &IngestChannelDeliveryBridge,
    job_id: &str,
    latency_ms: u64,
    error: Option<String>,
) {
    let now = Utc::now();
    let failed = error.as_ref().is_some_and(|value| !value.trim().is_empty());
    bridge.delivery_records.write().await.insert(
        job_id.to_string(),
        JobDeliveryRecord {
            state: if failed {
                JobDeliveryState::Failed
            } else {
                JobDeliveryState::Delivered
            },
            delivered_at: Some(now),
            error,
            latency_ms: Some(latency_ms),
        },
    );
    *bridge.last_delivery_at.write().await = Some(now);
    *bridge.last_delivery_latency_ms.write().await = Some(latency_ms);
    bridge.channel_deliveries.write().await.remove(job_id);
}

/// Terminal worker synthesis (or failure notify) push for ingest-origin turns.
pub async fn deliver_worker_result_to_ingest_channel(
    record: &TurnWorkRecord,
    text: &str,
    tool_names: &[String],
) -> Result<()> {
    let Some(target) = delivery_target_for_record(record) else {
        return Ok(());
    };
    if is_home_channel(&target.channel) || !is_external_push_channel(&target.channel) {
        return Ok(());
    }

    let Some(bridge) = ingest_channel_delivery_bridge() else {
        return Ok(());
    };

    let delivery_text = format_channel_delivery_text(text, tool_names, &target.channel);
    let dispatch_result = channel_delivery::dispatch_channel_message(
        &bridge.dispatch_client,
        &target,
        &delivery_text,
    )
    .await;

    let latency_ms = record
        .created_at
        .signed_duration_since(Utc::now())
        .num_milliseconds()
        .unsigned_abs();

    let job_id = ingest_job_id_for_record(record);
    match dispatch_result {
        Ok(()) => {
            mark_ingest_job_delivered(&bridge, &job_id, latency_ms, None).await;
            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            mark_ingest_job_delivered(&bridge, &job_id, latency_ms, Some(message.clone())).await;
            Err(anyhow!(message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_spawn_text_uses_user_ack_only() {
        let text = compose_turn_worker_spawn_text(&TurnWorkerSpawnNotifyPayload {
            work_id: "work-abc".to_string(),
            user_ack: "On it — researching that now.".to_string(),
            intent: Some("research".to_string()),
        });
        assert_eq!(text, "On it — researching that now.");
    }

    #[test]
    fn compose_spawn_text_empty_when_ack_empty() {
        let text = compose_turn_worker_spawn_text(&TurnWorkerSpawnNotifyPayload {
            work_id: "work-abc".to_string(),
            user_ack: "   ".to_string(),
            intent: None,
        });
        assert!(text.is_empty());
    }

    #[test]
    fn external_channel_guard_matches_budget_notify() {
        assert!(is_home_channel("home-ios"));
        assert!(!is_external_push_channel("home-desktop"));
        assert!(is_external_push_channel("whatsapp"));
    }

    #[test]
    fn ingest_job_id_prefers_parent_correlation() {
        let record = TurnWorkRecord {
            work_id: "work-1".to_string(),
            session_id: "s1".to_string(),
            parent_turn_correlation_id: Some("ingest-job-99".to_string()),
            parent_stream_turn_id: 0,
            intent: "general".to_string(),
            task_prompt: "task".to_string(),
            status: crate::agent_runtime::turn_worker::TurnWorkStatus::Pending,
            result_text: None,
            tool_names: vec![],
            termination_reason: None,
            error: None,
            user_ack: "On it".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "normal".to_string(),
            max_tool_rounds: 8,
            delivery_target: Some(StoredDeliveryTarget {
                channel: "whatsapp".to_string(),
                user_id: "u".to_string(),
                channel_id: "c".to_string(),
                session_id: "s1".to_string(),
                stream_id: None,
            }),
            parent_user_prompt: None,
            handoff_capsule: None,
            worker_scratch: None,
            synthesis_delivered: false,
            stasis_job_id: None,
            thread_id: None,
            stage_role: None,
            model_hint: None,
            manuscript_id: None,
            branch_group_id: None,
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(ingest_job_id_for_record(&record), "ingest-job-99");
    }
}
