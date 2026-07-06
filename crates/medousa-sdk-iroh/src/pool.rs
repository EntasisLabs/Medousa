//! Pooled HTTP clients for workshop transport (standard + streaming).

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

static STANDARD: OnceLock<Client> = OnceLock::new();
static STREAMING: OnceLock<Client> = OnceLock::new();

pub fn standard_client() -> &'static Client {
    STANDARD.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .pool_max_idle_per_host(8)
            .build()
            .expect("workshop standard client")
    })
}

pub fn streaming_client() -> &'static Client {
    STREAMING.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(600))
            .pool_max_idle_per_host(4)
            .build()
            .expect("workshop streaming client")
    })
}
