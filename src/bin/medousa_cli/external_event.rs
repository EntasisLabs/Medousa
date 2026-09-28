//! Provider callbacks over an explicit HTTP URL or an authenticated Iroh tunnel.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;

use super::cli::DaemonExternalEventArgs;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// No Debug: worker records and request headers contain credentials.
enum Destination {
    Iroh(String),
    Http(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerCredentials {
    #[serde(flatten)]
    summary: medousa::daemon_worker::DaemonWorkerConnection,
    session_token: String,
}

#[derive(Deserialize)]
struct WorkerStore {
    #[serde(default = "store_version")]
    version: u32,
    connections: Vec<WorkerCredentials>,
}

fn store_version() -> u32 {
    1
}

fn nonempty(value: String, name: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(value.to_owned())
}

fn credential(name: &str) -> Result<String> {
    nonempty(
        optional_environment(name)?.with_context(|| format!("{name} must be set privately"))?,
        name,
    )
}

fn optional_environment(name: &str) -> Result<Option<String>> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("{name} must contain valid Unicode"))
        })
        .transpose()
}

async fn worker_destination(path: &Path, selector: &str) -> Result<(Destination, String)> {
    let bytes = tokio::fs::read(path)
        .await
        .context("read saved workers; use `medousa pair workers` to check this VM's pairings")?;
    let store: WorkerStore = serde_json::from_slice(&bytes).context("decode saved workers")?;
    if store.version != 1 {
        bail!("unsupported worker store version");
    }
    select_worker(store.connections, selector)
}

fn select_worker(workers: Vec<WorkerCredentials>, selector: &str) -> Result<(Destination, String)> {
    let selector = selector.trim();
    if selector.is_empty() {
        bail!("worker ID or label must not be empty");
    }
    // Exact IDs win over labels; never silently pick connections[0].
    let exact_id = workers.iter().any(|worker| worker.summary.id == selector);
    let mut matches = workers.into_iter().filter(|worker| {
        if exact_id {
            worker.summary.id == selector
        } else {
            worker.summary.label.eq_ignore_ascii_case(selector)
        }
    });
    let worker = matches
        .next()
        .context("worker not found; use `medousa pair workers` for its ID or label")?;
    if matches.next().is_some() {
        bail!("worker label is ambiguous; select its exact ID from `medousa pair workers`");
    }
    let ticket = worker
        .summary
        .iroh_ticket
        .context("saved worker has no Iroh ticket; pair using a full Iroh invite")?;
    Ok((
        Destination::Iroh(nonempty(ticket, "saved Iroh ticket")?),
        nonempty(worker.session_token, "saved workshop bearer")?,
    ))
}

fn explicit_destination(ticket: Option<String>, daemon_url: &str) -> Result<Destination> {
    match ticket {
        Some(ticket) => Ok(Destination::Iroh(nonempty(ticket, "Iroh ticket")?)),
        None => Ok(Destination::Http(
            daemon_url.trim_end_matches('/').to_string(),
        )),
    }
}

fn callback_headers(bearer: &str, callback_key: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        (AUTHORIZATION, format!("Bearer {bearer}")),
        (
            reqwest::header::HeaderName::from_static("x-medousa-bridge-key"),
            callback_key.to_string(),
        ),
    ] {
        let mut value =
            HeaderValue::from_str(&value).context("invalid callback credential header")?;
        // The Iroh client serializes HTTP/1.1 itself. Validate before either
        // transport so newlines or non-ASCII values never reach its wire format.
        value
            .to_str()
            .context("callback credentials must be ASCII")?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

async fn post_event(
    destination: &Destination,
    path: &str,
    headers: HeaderMap,
    body: &[u8],
) -> Result<()> {
    let status = match destination {
        Destination::Iroh(ticket) => post_iroh(ticket, path, &headers, body).await?,
        Destination::Http(base) => {
            // A POST must not redirect credentials or be replayed on a different
            // transport after an uncertain delivery.
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(REQUEST_TIMEOUT)
                .build()?
                .post(format!("{base}{path}"))
                .headers(headers)
                .body(body.to_vec())
                .send()
                .await
                .context("HTTP callback delivery failed; retry with the same event ID if its outcome is uncertain")?
                .status()
                .as_u16()
        }
    };
    match status {
        200..=299 => Ok(()),
        401 => bail!("callback returned HTTP 401; renew the paired workshop bearer"),
        403 => bail!(
            "callback returned HTTP 403; check portal permissions and the conversation callback key"
        ),
        _ => bail!("callback returned HTTP {status}"),
    }
}

#[cfg(feature = "iroh-transport")]
async fn post_iroh(ticket: &str, path: &str, headers: &HeaderMap, body: &[u8]) -> Result<u16> {
    let headers = headers
        .iter()
        .map(|(name, value)| Ok((name.as_str(), value.to_str()?)))
        .collect::<Result<Vec<_>>>()?;
    let response = tokio::time::timeout(
        REQUEST_TIMEOUT,
        medousa::iroh_transport::iroh_http_request(ticket, "POST", path, &headers, Some(body)),
    )
    .await
    .context("Iroh callback timed out; delivery may be uncertain, so retry with the same event ID")?
    .context("Iroh callback failed; no HTTP fallback was attempted")?;
    Ok(response.status)
}

#[cfg(not(feature = "iroh-transport"))]
async fn post_iroh(_: &str, _: &str, _: &HeaderMap, _: &[u8]) -> Result<u16> {
    bail!(
        "this CLI was built without Iroh support; install an Iroh-enabled build or build with --features iroh-transport"
    )
}

pub async fn run(args: DaemonExternalEventArgs) -> Result<()> {
    let id = uuid::Uuid::parse_str(&args.conversation_id).context("invalid conversation ID")?;
    let callback_key = credential("MEDOUSA_BRIDGE_KEY")?;
    let (destination, bearer) = if let Some(worker) = args.worker.as_deref() {
        // Ticket and bearer come from the same record. Environment overrides
        // must not accidentally mix credentials from two different workshops.
        worker_destination(
            &medousa::paths::medousa_data_dir().join("delegation/workers.json"),
            worker,
        )
        .await?
    } else {
        let ticket = match args.iroh_ticket {
            Some(ticket) => Some(ticket),
            None => optional_environment("MEDOUSA_BRIDGE_IROH_TICKET")?,
        };
        let daemon_url = medousa::resolve_daemon_url(args.daemon_url.as_deref());
        (
            explicit_destination(ticket, &daemon_url)?,
            credential("MEDOUSA_BRIDGE_BEARER")?,
        )
    };
    let body = serde_json::to_vec(&serde_json::json!({
        "event_id": args.event_id, "request_id": args.request_id,
        "kind": args.kind, "text": args.text,
    }))?;
    post_event(
        &destination,
        &format!("/v1/external-conversations/{id}/events"),
        callback_headers(&bearer, &callback_key)?,
        &body,
    )
    .await?;
    let transport = match destination {
        Destination::Iroh(_) => "Iroh",
        Destination::Http(_) => "HTTP",
    };
    println!("event accepted via {transport}");
    Ok(())
}

#[cfg(test)]
#[path = "external_event_tests.rs"]
mod tests;
