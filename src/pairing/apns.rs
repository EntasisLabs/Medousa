//! Apple Push Notification service (HTTP/2 + token auth).

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use a2::{
    Client, ClientConfig, DefaultNotificationBuilder, Endpoint, NotificationBuilder,
    NotificationOptions, Priority, PushType,
};
use a2::request::payload::PayloadLike;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ApnsConfig {
    pub team_id: String,
    pub key_id: String,
    pub key_pem: String,
    pub bundle_id: String,
    pub sandbox: bool,
}

impl ApnsConfig {
    /// Prefer env vars, then `{medousa_data_dir}/apns/config.json` + key file.
    pub fn load() -> (Option<Self>, super::apns_config::ApnsConfigSource) {
        super::apns_config::load_apns_config()
    }

    #[deprecated(note = "use ApnsConfig::load")]
    pub fn from_env() -> Option<Self> {
        Self::load().0
    }
}

pub struct ApnsClient {
    inner: Client,
    bundle_id: String,
    live_activity_topic: &'static str,
}

impl ApnsClient {
    pub fn new(config: &ApnsConfig) -> Result<Self> {
        let endpoint = if config.sandbox {
            Endpoint::Sandbox
        } else {
            Endpoint::Production
        };
        let client_config = ClientConfig {
            endpoint,
            ..ClientConfig::default()
        };
        let mut key_reader = Cursor::new(config.key_pem.as_bytes());
        let inner = Client::token(
            &mut key_reader,
            config.key_id.clone(),
            config.team_id.clone(),
            client_config,
        )
        .context("initialize APNs client")?;
        Ok(Self {
            inner,
            bundle_id: config.bundle_id.clone(),
            live_activity_topic: Box::leak(
                format!("{}.push-type.liveactivity", config.bundle_id).into_boxed_str(),
            ),
        })
    }

    pub async fn send_alert(
        &self,
        device_token: &str,
        title: &str,
        body: &str,
        badge: Option<u32>,
        custom: &HashMap<&str, String>,
    ) -> Result<()> {
        let token = device_token.trim();
        if token.is_empty() {
            bail!("empty APNs device token");
        }

        let mut builder = DefaultNotificationBuilder::new()
            .set_title(title)
            .set_body(body)
            .set_sound("default");
        if let Some(count) = badge {
            builder = builder.set_badge(count);
        }

        let options = NotificationOptions {
            apns_id: None,
            apns_expiration: None,
            apns_priority: Some(Priority::High),
            apns_topic: Some(self.bundle_id.as_str()),
            apns_collapse_id: None,
            apns_push_type: Some(PushType::Alert),
        };

        let mut payload = builder.build(token, options);
        for (key, value) in custom {
            payload
                .add_custom_data(*key, value)
                .with_context(|| format!("add custom data key {key}"))?;
        }

        self.inner
            .send(payload)
            .await
            .context("APNs send failed")?;
        Ok(())
    }

    pub async fn send_live_activity_update(
        &self,
        push_token: &str,
        content_state: &LiveActivityContentState,
    ) -> Result<()> {
        let token = push_token.trim();
        if token.is_empty() {
            bail!("empty Live Activity push token");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let stale_date = now.saturating_add(60 * 15);

        let payload = LiveActivityUpdatePayload {
            aps: LiveActivityApsBody {
                timestamp: now,
                event: "update",
                content_state: content_state.clone(),
                stale_date: Some(stale_date),
                dismissal_date: None,
            },
            device_token: token,
            options: self.live_activity_options(),
        };

        self.inner
            .send(payload)
            .await
            .context("APNs Live Activity update failed")?;
        Ok(())
    }

    pub async fn send_live_activity_end(
        &self,
        push_token: &str,
        content_state: Option<&LiveActivityContentState>,
    ) -> Result<()> {
        let token = push_token.trim();
        if token.is_empty() {
            bail!("empty Live Activity push token");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        // Dismiss immediately when work is idle — a future dismissal_date left the
        // Live Activity on the Lock Screen after workshop turns finished.
        let payload = LiveActivityUpdatePayload {
            aps: LiveActivityApsBody {
                timestamp: now,
                event: "end",
                content_state: content_state.cloned().unwrap_or(LiveActivityContentState {
                    mood: "quiet".to_string(),
                    eyebrow: "Quiet".to_string(),
                    headline: "Nothing needs you".to_string(),
                    subline: None,
                    motion_summary: None,
                    blocked_count: 0,
                    primary_card_id: None,
                }),
                stale_date: None,
                dismissal_date: Some(now),
            },
            device_token: token,
            options: self.live_activity_options(),
        };

        self.inner
            .send(payload)
            .await
            .context("APNs Live Activity end failed")?;
        Ok(())
    }

    /// Silent background push — updates the home-screen widget via App Group storage.
    pub async fn send_silent_widget_pulse(
        &self,
        device_token: &str,
        pulse_json: &str,
    ) -> Result<()> {
        let token = device_token.trim();
        if token.is_empty() {
            bail!("empty APNs device token");
        }

        let mut builder = DefaultNotificationBuilder::new().set_content_available();
        let options = NotificationOptions {
            apns_id: None,
            apns_expiration: None,
            apns_priority: Some(Priority::Normal),
            apns_topic: Some(self.bundle_id.as_str()),
            apns_collapse_id: None,
            apns_push_type: Some(PushType::Background),
        };

        let mut payload = builder.build(token, options);
        payload
            .add_custom_data("medousaType", &"pulse_snapshot")
            .context("add medousaType")?;
        payload
            .add_custom_data("medousaPulse", &pulse_json.to_string())
            .context("add medousaPulse")?;

        self.inner
            .send(payload)
            .await
            .context("APNs silent widget pulse failed")?;
        Ok(())
    }

    fn live_activity_options(&self) -> NotificationOptions<'static> {
        NotificationOptions {
            apns_id: None,
            apns_expiration: None,
            apns_priority: Some(Priority::High),
            apns_topic: Some(self.live_activity_topic),
            apns_collapse_id: None,
            apns_push_type: Some(PushType::LiveActivity),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveActivityContentState {
    pub mood: String,
    pub eyebrow: String,
    pub headline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_summary: Option<String>,
    pub blocked_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LiveActivityApsBody {
    timestamp: u64,
    event: &'static str,
    #[serde(rename = "content-state")]
    content_state: LiveActivityContentState,
    #[serde(rename = "stale-date", skip_serializing_if = "Option::is_none")]
    stale_date: Option<u64>,
    #[serde(rename = "dismissal-date", skip_serializing_if = "Option::is_none")]
    dismissal_date: Option<u64>,
}

#[derive(Debug, Serialize)]
struct LiveActivityUpdatePayload<'a> {
    aps: LiveActivityApsBody,
    #[serde(skip)]
    device_token: &'a str,
    #[serde(skip)]
    options: NotificationOptions<'a>,
}

impl<'a> PayloadLike for LiveActivityUpdatePayload<'a> {
    fn get_device_token(&self) -> &str {
        self.device_token
    }

    fn get_options(&self) -> &NotificationOptions<'_> {
        &self.options
    }
}

pub type SharedApnsClient = Arc<Mutex<Option<ApnsClient>>>;

pub fn shared_apns_client() -> SharedApnsClient {
    static CLIENT: once_cell::sync::OnceCell<SharedApnsClient> = once_cell::sync::OnceCell::new();
    CLIENT
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

pub async fn ensure_apns_client(config: &ApnsConfig) -> Result<()> {
    let shared = shared_apns_client();
    let mut guard = shared.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    *guard = Some(ApnsClient::new(config)?);
    Ok(())
}
