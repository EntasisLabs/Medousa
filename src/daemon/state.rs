use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::prelude::RuntimeComposition;
use tokio::sync::RwLock;

use crate::browser_handlers::ClientRegistry;
use crate::daemon_api::ContextUsageReport;
use crate::MedousaPlatformRuntime;
use crate::channel_delivery;
use crate::daemon::bounded_set::BoundedDedupSet;
use crate::daemon::heartbeat::{
    HeartbeatDeliveryMetrics, HeartbeatDeliveryPolicy, HeartbeatNotifyConfig, TickReport,
};
use crate::engine_context::HeartbeatLanePolicy;
use crate::daemon::turn_stream_registry::TurnStreamRegistry;
use crate::session_mapping;
use crate::turn_ticket::TurnTicketRegistry;
use crate::user_profiles::UserProfileRegistry;

#[derive(Debug, Clone)]
pub struct AgentTurnJobRecord {
    pub status: String,
    pub output_text: Option<String>,
    pub error: Option<String>,
    /// Set when the record reaches a terminal state (`succeeded`/`failed`).
    /// Drives TTL + capacity eviction so completed job results do not accumulate
    /// their (potentially large) `output_text` in memory forever.
    pub finished_at: Option<DateTime<Utc>>,
}

impl AgentTurnJobRecord {
    pub fn pending() -> Self {
        Self {
            status: "pending".to_string(),
            output_text: None,
            error: None,
            finished_at: None,
        }
    }

    /// A record still in flight has no terminal timestamp and must never be
    /// evicted (a client may still be polling for its result).
    pub fn is_terminal(&self) -> bool {
        self.finished_at.is_some()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub platform: Arc<MedousaPlatformRuntime>,
    pub daemon_base_url: String,
    pub interactive_turn_streams: TurnStreamRegistry,
    pub active_ingest_jobs: Arc<RwLock<HashMap<String, session_mapping::ActiveIngestJob>>>,
    pub channel_deliveries: Arc<RwLock<HashMap<String, channel_delivery::ChannelDeliveryTarget>>>,
    pub job_delivery_records: Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    pub delivered_outbox_events: Arc<RwLock<BoundedDedupSet>>,
    pub channel_dispatch_client: reqwest::Client,
    pub deliver_webhook_token: Option<String>,
    pub deliver_webhook_target: String,
    pub last_delivery_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_delivery_latency_ms: Arc<RwLock<Option<u64>>>,
    pub last_agent_turn_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_agent_turn_latency_ms: Arc<RwLock<Option<u64>>>,
    pub agent_tool_registry_count: usize,
    pub agent_turn_jobs: Arc<RwLock<HashMap<String, AgentTurnJobRecord>>>,
    pub default_runtime_config: session_mapping::IngestSessionRuntimeConfig,
    pub cancelled_ingest_streams: Arc<RwLock<BoundedDedupSet>>,
    pub cancelled_interactive_turns: Arc<RwLock<BoundedDedupSet>>,
    pub turn_tickets: TurnTicketRegistry,
    pub session_runtime_configs:
        Arc<RwLock<HashMap<String, session_mapping::IngestSessionRuntimeConfig>>>,
    pub backend: String,
    pub worker_id: String,
    pub identity_service: Arc<IdentityMemoryService>,
    pub profile_registry: Arc<std::sync::RwLock<UserProfileRegistry>>,
    pub last_tick_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_heartbeat_report: Arc<RwLock<Option<TickReport>>>,
    pub heartbeat_policy: HeartbeatLanePolicy,
    pub heartbeat_delivery_policy: HeartbeatDeliveryPolicy,
    pub heartbeat_metrics: Arc<RwLock<HeartbeatDeliveryMetrics>>,
    pub heartbeat_notify: HeartbeatNotifyConfig,
    pub webhook_client: Option<reqwest::Client>,
    pub retention_config: crate::session_retention::SessionRetentionConfig,
    pub last_retention_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Last time the bounded in-memory state caps were swept. Independent of
    /// `last_retention_at` because the in-memory prune runs regardless of the
    /// (env-gated) durable retention config.
    pub last_mem_prune_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Latest turn-start context budget per session (from `context_usage` stream events).
    pub last_context_usage_by_session: Arc<RwLock<HashMap<String, ContextUsageReport>>>,
    pub client_registry: ClientRegistry,
}

impl AppState {
    pub fn composition(&self) -> &RuntimeComposition {
        self.platform.composition()
    }

    pub fn workshop_identity_user_id(&self) -> String {
        self.profile_registry
            .read()
            .expect("profile registry lock")
            .resolve_active_user_id()
    }

    /// Bound the `agent_turn_jobs` map, which otherwise retains every completed
    /// job's full `output_text` for the daemon's lifetime.
    ///
    /// Terminal records older than `ttl` are dropped; if the map is still over
    /// `max_entries`, the oldest terminal records are evicted until it fits.
    /// `pending`/`running` records are never evicted so an in-flight turn's
    /// result is always available to a polling client. Returns the number of
    /// records removed.
    pub async fn prune_agent_turn_jobs(&self, ttl: ChronoDuration, max_entries: usize) -> usize {
        let cutoff = Utc::now() - ttl;
        let mut jobs = self.agent_turn_jobs.write().await;
        let before = jobs.len();

        jobs.retain(|_, record| match record.finished_at {
            Some(finished) => finished >= cutoff,
            None => true,
        });

        if jobs.len() > max_entries {
            let mut terminal: Vec<(String, DateTime<Utc>)> = jobs
                .iter()
                .filter_map(|(id, record)| record.finished_at.map(|ts| (id.clone(), ts)))
                .collect();
            terminal.sort_by_key(|(_, ts)| *ts);
            let overflow = jobs.len().saturating_sub(max_entries);
            for (id, _) in terminal.into_iter().take(overflow) {
                jobs.remove(&id);
            }
        }

        before.saturating_sub(jobs.len())
    }
}
