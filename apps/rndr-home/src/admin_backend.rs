//! Independent read worker: never accepts URLs, JSON commands or mutation requests.
use crate::admin_model::{ReadError, Record, Resource};
#[cfg(test)]
#[path = "admin_backend_tests.rs"]
mod tests;
use medousa_sdk::{MedousaClient, SdkError};
use rndr_core::{Event, feed};
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct ReadRequest {
    pub resource: Resource,
    pub serial: u64,
}
#[derive(Clone)]
pub struct ReadReply {
    pub resource: Resource,
    pub serial: u64,
    pub result: Result<Vec<Record>, ReadError>,
}
impl Event for ReadReply {}

pub fn run(
    origin: String,
    dir: PathBuf,
    mut requests: mpsc::UnboundedReceiver<ReadRequest>,
    out: feed::Sender<ReadReply>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("read runtime");
    runtime.block_on(async move {
        let client = crate::backend::client(&origin, &dir);
        while let Some(request) = requests.recv().await {
            let result = match &client {
                Ok(client) => match tokio::time::timeout(
                    std::time::Duration::from_secs(12),
                    read(client, request.resource),
                )
                .await
                {
                    Ok(result) => result.map_err(classify),
                    Err(_) => Err(ReadError::Unavailable),
                },
                Err(_) => Err(ReadError::Permission),
            };
            if out
                .send(ReadReply {
                    resource: request.resource,
                    serial: request.serial,
                    result,
                })
                .is_err()
            {
                break;
            }
        }
    });
}
fn classify(error: SdkError) -> ReadError {
    match error {
        SdkError::Compatibility(_) | SdkError::Serde(_) => ReadError::Incompatible,
        SdkError::Http(message) | SdkError::Transport(message) => {
            // SDK currently erases structured HTTP status. Never render raw errors
            // (they may contain service response bodies or configured addresses).
            if message.contains("401") || message.contains("403") {
                ReadError::Permission
            } else {
                ReadError::Unavailable
            }
        }
    }
}
fn yes(value: bool) -> String {
    if value { "Yes" } else { "No" }.into()
}

pub async fn read(client: &MedousaClient, resource: Resource) -> Result<Vec<Record>, SdkError> {
    let rows = match resource {
        Resource::Health => {
            let h = client.health().get().await?;
            vec![Record::new(
                "Active workshop / daemon",
                vec![
                    ("Status", h.status),
                    ("Observed at UTC", h.now_utc.to_rfc3339()),
                    ("Authority", h.runtime.authority_id.to_string()),
                    ("Version", h.runtime.product_version),
                    ("Build", h.runtime.build_revision),
                    ("Contract", h.runtime.contract_revision.to_string()),
                    ("Schema", h.runtime.base_schema_revision.to_string()),
                    ("Deployment", h.runtime.deployment_target),
                    ("Backend", h.backend),
                    ("Tool registry count", h.tool_registry_count.to_string()),
                ],
            )]
        }
        Resource::Models => {
            // Same read-only cached catalog used by existing preview and production Models.
            let value = client
                .http()
                .get::<serde_json::Value>("/v1/models/catalog")
                .await?;
            let models = value["models"]
                .as_array()
                .ok_or_else(|| SdkError::Compatibility("catalog missing models".into()))?;
            let mut rows = Vec::new();
            for model in models.iter().take(500) {
                let provider = model["provider"]
                    .as_str()
                    .ok_or_else(|| SdkError::Compatibility("model provider missing".into()))?;
                let id = model["model_id"]
                    .as_str()
                    .ok_or_else(|| SdkError::Compatibility("model ID missing".into()))?;
                rows.push(Record::new(
                    id,
                    vec![
                        ("Provider", provider.into()),
                        ("Model ID", id.into()),
                        (
                            "Availability",
                            "Catalog entry only; credentials and inference not tested".into(),
                        ),
                    ],
                ));
            }
            rows
        }
        Resource::Runtimes => client
            .agents()
            .list_runtimes()
            .await?
            .runtimes
            .into_iter()
            .take(500)
            .map(|r| {
                Record::new(
                    r.runtime,
                    vec![
                        ("Available", yes(r.available)),
                        ("Binary present", yes(r.binary_present)),
                        (
                            "Account status",
                            match r.auth_status.as_deref() {
                                Some("signed_in") => "signed_in",
                                Some("signed_out") => "signed_out",
                                _ => "unknown",
                            }
                            .into(),
                        ),
                        ("Native turns", yes(r.uses_native_turns)),
                        (
                            "Qualification",
                            "Daemon report only; no login, launch or inference performed".into(),
                        ),
                    ],
                )
            })
            .collect(),
        Resource::Modes => client
            .runtime()
            .agent_modes()
            .await?
            .modes
            .into_iter()
            .take(500)
            .map(|m| {
                Record::new(
                    m.label,
                    vec![
                        ("Mode ID", m.mode.as_str().into()),
                        ("Available", yes(m.available)),
                    ],
                )
            })
            .collect(),
        Resource::Mcp => {
            let m = client.mcp_gateway().status().await?;
            let mut rows = vec![Record::new(
                "Gateway",
                vec![
                    ("Reachable", yes(m.reachable)),
                    (
                        "Invokes enabled",
                        m.health
                            .as_ref()
                            .map(|h| yes(h.invokes_enabled))
                            .unwrap_or_else(|| "Unknown".into()),
                    ),
                    (
                        "Connected servers",
                        m.health
                            .as_ref()
                            .map(|h| h.connected_servers.to_string())
                            .unwrap_or_else(|| "Unknown".into()),
                    ),
                ],
            )];
            rows.extend(m.servers.into_iter().take(500).map(|s| {
                Record::new(
                    s.title,
                    vec![
                        ("Server ID", s.server_id),
                        ("Enabled", yes(s.enabled)),
                        ("Connected", yes(s.connected)),
                        ("Tools", s.tool_count.to_string()),
                    ],
                )
            }));
            rows
        }
        Resource::LocalEngine => {
            let e = client.local_models().engine_status().await?;
            vec![Record::new(
                "Local inference engine",
                vec![
                    ("Feature enabled", yes(e.feature_enabled)),
                    ("Loaded", yes(e.loaded)),
                    ("Phase", format!("{:?}", e.phase)),
                    (
                        "Model alias",
                        e.model_alias.unwrap_or_else(|| "Not reported".into()),
                    ),
                    (
                        "Backend",
                        e.inference_backend.unwrap_or_else(|| "Not reported".into()),
                    ),
                ],
            )]
        }
        Resource::Sharing => {
            let s: medousa_types::SharedModeStatusResponse =
                client.http().get("/v1/shared-mode").await?;
            vec![Record::new(
                "Shared workshop mode",
                vec![
                    ("Mode", s.mode),
                    (
                        "Enabled at UTC",
                        s.enabled_at
                            .map(|v| v.to_rfc3339())
                            .unwrap_or_else(|| "Not reported".into()),
                    ),
                    ("Root profile", s.root_profile_id),
                    ("General profile", s.general_profile_id),
                ],
            )]
        }
        Resource::Capacity => {
            let v = client
                .http()
                .get::<serde_json::Value>("/v1/runtime/workers")
                .await?;
            let mut fields = Vec::new();
            for (key, label) in [
                ("max_in_flight", "Maximum in flight"),
                ("agents", "Agents"),
                ("scheduled", "Scheduled"),
                ("delivery", "Delivery"),
                ("maintenance", "Maintenance"),
            ] {
                let count = v[key]
                    .as_u64()
                    .ok_or_else(|| SdkError::Compatibility("capacity field missing".into()))?;
                fields.push((label, count.to_string()));
            }
            vec![Record::new("Configured worker capacity", fields)]
        }
        Resource::ReviewProviders => {
            let p = client.reviews().providers().await?;
            let problems = p.problems.len();
            let mut rows: Vec<_> = p
                .providers
                .into_iter()
                .take(499)
                .map(|p| {
                    Record::new(
                        p.label,
                        vec![
                            ("Provider ID", p.id),
                            ("Protocol", p.protocol_version.to_string()),
                            (
                                "Authorization",
                                "Not queried; provider discovery is not account authentication"
                                    .into(),
                            ),
                        ],
                    )
                })
                .collect();
            if problems > 0 {
                rows.push(Record::new(
                    "Provider discovery incomplete",
                    vec![
                        ("Problem count", problems.to_string()),
                        (
                            "Diagnostics",
                            "Consult existing app; raw provider diagnostics are withheld".into(),
                        ),
                    ],
                ));
            }
            rows
        }
        Resource::Schedules => client
            .recurring()
            .list(&medousa_types::RecurringListQuery { enabled_only: None })
            .await?
            .recurring
            .into_iter()
            .take(500)
            .map(|r| {
                Record::new(
                    r.display_name.as_deref().unwrap_or(&r.recurring_id),
                    vec![
                        ("ID", r.recurring_id.clone()),
                        ("Enabled", yes(r.enabled)),
                        ("Schedule", r.cron_expr),
                        ("Timezone", r.timezone),
                        ("Job type", r.job_type),
                        ("Queue", r.queue),
                        ("Next run UTC", r.next_run_at_utc.to_rfc3339()),
                        (
                            "Last run status",
                            r.last_run_status.unwrap_or_else(|| "Not reported".into()),
                        ),
                    ],
                )
            })
            .collect(),
    };
    Ok(rows)
}
