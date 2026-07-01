//! Apple Push Notification service (HTTP/2 + token auth).

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use a2::{
    Client, ClientConfig, DefaultNotificationBuilder, Endpoint, NotificationBuilder,
    NotificationOptions, Priority, PushType,
};
use anyhow::{Context, Result, bail};
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
