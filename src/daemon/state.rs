use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::prelude::RuntimeComposition;
use tokio::sync::RwLock;

use crate::browser_handlers::ClientRegistry;
use crate::MedousaPlatformRuntime;
use crate::channel_delivery;
use crate::daemon::heartbeat::{
    HeartbeatDeliveryMetrics, HeartbeatDeliveryPolicy, HeartbeatNotifyConfig, TickReport,
};
use crate::engine_context::HeartbeatLanePolicy;
use crate::session_mapping;
use crate::turn_ticket::TurnTicketRegistry;
use crate::user_profiles::UserProfileRegistry;

#[derive(Debug, Clone)]
pub struct AgentTurnJobRecord {
    pub status: String,
    pub output_text: Option<String>,
    pub error: Option<String>,
}

impl AgentTurnJobRecord {
    pub fn pending() -> Self {
        Self {
            status: "pending".to_string(),
            output_text: None,
            error: None,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub platform: Arc<MedousaPlatformRuntime>,
    pub daemon_base_url: String,
    pub interactive_turn_streams:
        Arc<RwLock<HashMap<String, Arc<crate::daemon::turn_event_channel::TurnEventChannel>>>>,
    pub active_ingest_jobs: Arc<RwLock<HashMap<String, session_mapping::ActiveIngestJob>>>,
    pub channel_deliveries: Arc<RwLock<HashMap<String, channel_delivery::ChannelDeliveryTarget>>>,
    pub job_delivery_records: Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    pub delivered_outbox_events: Arc<RwLock<HashSet<String>>>,
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
    pub cancelled_ingest_streams: Arc<RwLock<HashSet<String>>>,
    pub cancelled_interactive_turns: Arc<RwLock<HashSet<String>>>,
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
}
