//! Process-wide `tracing` subscriber setup.
//!
//! ## Defaults
//!
//! * `RUST_LOG` / `MEDOUSA_LOG` env filter (default `medousa=info,stasis=warn`)
//! * Human-readable stderr output
//!
//! ## Optional OTLP export
//!
//! Build with `--features otel-export`, then set:
//!
//! ```text
//! MEDOUSA_OTEL_ENABLED=true
//! OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
//! OTEL_SERVICE_NAME=medousa-daemon
//! ```
//!
//! Stasis-internal spans continue to honor `STASIS_OTEL_ENABLED` via
//! [`crate::runtime::stasis_otel`].

use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

static INIT: std::sync::Once = std::sync::Once::new();
static STATUS: OnceLock<String> = OnceLock::new();

/// Initialize tracing once (idempotent). Safe to call from binaries and tests.
pub fn init_tracing() {
    INIT.call_once(|| {
        let status = init_tracing_inner();
        let _ = STATUS.set(status);
    });
}

/// Initialize tracing, reading `MEDOUSA_LOG` then `RUST_LOG`.
pub fn init_tracing_from_env() {
    init_tracing();
}

fn init_tracing_inner() -> String {
    let filter = std::env::var("MEDOUSA_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "medousa=info,stasis=warn".to_string());

    let env_filter =
        EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("medousa=info"));

    #[cfg(feature = "otel-export")]
    {
        if medousa_otel_enabled() {
            return init_with_otel(env_filter);
        }
    }

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();

    "tracing=fmt (stderr)".to_string()
}

#[cfg(feature = "otel-export")]
fn init_with_otel(env_filter: EnvFilter) -> String {
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::TracerProvider;
    use opentelemetry_sdk::Resource;

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .or_else(|_| std::env::var("STASIS_OTEL_SERVICE_NAME"))
        .unwrap_or_else(|_| "medousa".to_string());

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint.clone());

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            service_name.clone(),
        )]))
        .build();

    let tracer = provider.tracer("medousa");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(otel_layer)
        .init();

    opentelemetry::global::set_tracer_provider(provider);

    format!("tracing=fmt+otlp service={service_name} endpoint={endpoint}")
}

#[cfg(not(feature = "otel-export"))]
fn medousa_otel_enabled() -> bool {
    false
}

#[cfg(feature = "otel-export")]
fn medousa_otel_enabled() -> bool {
    matches!(
        std::env::var("MEDOUSA_OTEL_ENABLED")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

/// Human-readable tracing init status for startup logs / doctor.
pub fn tracing_status_line() -> String {
    STATUS
        .get()
        .cloned()
        .unwrap_or_else(|| "tracing=not_initialized".to_string())
}
