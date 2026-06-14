//! Thin WhatsApp adapter — whatsapp-rust client + local deliver endpoint for daemon outbox push.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use wacore::proto_helpers::MessageExt;
use wacore::store::traits::Backend;
use wacore::types::events::Event;
use waproto::whatsapp as wa;
use whatsapp_rust::Jid;
use whatsapp_rust::TokioRuntime;
use whatsapp_rust::bot::{Bot, MessageContext};
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
use whatsapp_rust_ureq_http_client::UreqHttpClient;

const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";
const DEFAULT_DELIVER_BIND: &str = "127.0.0.1:7422";
const DEFAULT_DELIVERY_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(700);
const ADAPTER_COMMAND_HINT: &str =
    "Commands: /new /help /history /model /depth /stop /regen /health /heartbeat — or send a message to chat.";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IngestRequest {
    channel: String,
    user_id: String,
    channel_id: String,
    text: String,
    #[serde(default)]
    attachments: Vec<IngestAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IngestAttachment {
    kind: String,
    url: Option<String>,
    mime_type: Option<String>,
    filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IngestResponse {
    reply: String,
    session_id: String,
    is_new_session: bool,
    stream_ready: bool,
    job_id: Option<String>,
    stream_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeliverRequest {
    channel_id: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DeliverPollResponse {
    status: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JobResultResponse {
    status: String,
    output_text: Option<String>,
    latest_outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AdapterDeliveryOutcome {
    PushDelivered,
    StreamError { message: String },
    Fallback { text: String },
}

#[derive(Clone)]
struct WhatsAppAdapterState {
    client: Arc<whatsapp_rust::Client>,
    daemon_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        print_usage();
        return Ok(());
    }

    let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
    let deliver_bind = resolve_deliver_bind(find_arg_value(&args, "--deliver-bind"));
    let session_db = resolve_session_db_path(find_arg_value(&args, "--session-db"));

    let backend = build_sqlite_backend(&session_db).await?;
    let mut transport_factory = TokioWebSocketTransportFactory::new();
    if let Ok(ws_url) = std::env::var("WHATSAPP_WS_URL") {
        transport_factory = transport_factory.with_url(ws_url);
    }

    let http_client = Client::new();
    let daemon_url_for_events = daemon_url.clone();
    let http_client_for_events = http_client.clone();

    let mut bot = Bot::builder()
        .with_backend(backend)
        .with_transport_factory(transport_factory)
        .with_http_client(UreqHttpClient::new())
        .with_runtime(TokioRuntime)
        .on_event(move |event, client| {
            let daemon_url = daemon_url_for_events.clone();
            let http_client = http_client_for_events.clone();
            async move {
                if let Err(err) = handle_event(event, client, http_client, daemon_url).await {
                    eprintln!("medousa_whatsapp event handling error: {err:#}");
                }
            }
        })
        .build()
        .await
        .context("build whatsapp bot")?;

    let wa_client = bot.client();
    let adapter_state = Arc::new(WhatsAppAdapterState {
        client: wa_client,
        daemon_url,
    });

    println!(
        "medousa_whatsapp thin adapter started — forwarding to daemon at {}",
        adapter_state.daemon_url
    );
    println!(
        "medousa_whatsapp deliver endpoint on http://{deliver_bind}/v1/deliver"
    );
    println!("medousa_whatsapp session db: {}", session_db.display());

    let deliver_state = adapter_state.clone();
    tokio::spawn(async move {
        if let Err(err) = serve_deliver_endpoint(deliver_bind, deliver_state).await {
            eprintln!("medousa_whatsapp deliver server error: {err:#}");
        }
    });

    let bot_handle = bot.run().await.context("start whatsapp bot")?;
    bot_handle.await.ok();
    Ok(())
}

async fn serve_deliver_endpoint(bind: String, state: Arc<WhatsAppAdapterState>) -> Result<()> {
    let app = Router::new()
        .route("/v1/deliver", post(deliver_message))
        .with_state(state);

    let listener = TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind whatsapp deliver endpoint on {bind}"))?;
    axum::serve(listener, app)
        .await
        .context("serve whatsapp deliver endpoint")?;
    Ok(())
}

async fn deliver_message(
    State(state): State<Arc<WhatsAppAdapterState>>,
    Json(body): Json<DeliverRequest>,
) -> Result<StatusCode, StatusCode> {
    match deliver_whatsapp_text(&state, &body.channel_id, &body.text).await {
        Ok(()) => Ok(StatusCode::OK),
        Err(err) => {
            eprintln!("medousa_whatsapp deliver error: {err:#}");
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn build_sqlite_backend(session_db: &Path) -> Result<Arc<dyn Backend>> {
    if let Some(parent) = session_db.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "create whatsapp session directory {}",
                parent.display()
            )
        })?;
    }

    let backend = SqliteStore::new(session_db.to_string_lossy().as_ref())
        .await
        .with_context(|| format!("open whatsapp sqlite store at {}", session_db.display()))?;

    Ok(Arc::new(backend))
}

async fn deliver_whatsapp_text(
    state: &WhatsAppAdapterState,
    channel_id: &str,
    text: &str,
) -> Result<()> {
    let jid = parse_whatsapp_chat_jid(channel_id)?;
    let message = wa::Message {
        conversation: Some(truncate_for_whatsapp(text)),
        ..Default::default()
    };
    state
        .client
        .send_message(jid, message)
        .await
        .context("whatsapp send_message failed")?;
    Ok(())
}

async fn handle_event(
    event: Arc<Event>,
    client: Arc<whatsapp_rust::Client>,
    http_client: Client,
    daemon_url: String,
) -> Result<()> {
    match &*event {
        Event::PairingQrCode { code, timeout } => {
            println!("Scan WhatsApp QR in Linked Devices (valid ~{}s):", timeout.as_secs());
            println!("{code}");
        }
        Event::PairingCode { code, timeout } => {
            println!(
                "Enter WhatsApp pairing code on phone (valid ~{}s): {code}",
                timeout.as_secs()
            );
        }
        Event::Connected(_) => {
            println!("medousa_whatsapp connected");
        }
        Event::LoggedOut(_) => {
            eprintln!("medousa_whatsapp logged out — restart adapter to re-pair");
        }
        Event::Message(msg, info) => {
            if info.source.is_from_me {
                return Ok(());
            }

            let Some(text) = msg.text_content().map(str::trim).filter(|value| !value.is_empty())
            else {
                return Ok(());
            };

            let ctx = MessageContext::from_parts(msg, info, client);
            handle_inbound_message(&http_client, &daemon_url, &ctx, text).await?;
        }
        _ => {}
    }

    Ok(())
}

async fn handle_inbound_message(
    http_client: &Client,
    daemon_url: &str,
    ctx: &MessageContext,
    text: &str,
) -> Result<()> {
    let chat_jid = ctx.info.source.chat.to_string();
    let sender_jid = ctx.info.source.sender.to_string();
    let request = IngestRequest {
        channel: "whatsapp".to_string(),
        user_id: format!("whatsapp:user:{sender_jid}"),
        channel_id: format!("whatsapp:chat:{chat_jid}"),
        text: text.to_string(),
        attachments: Vec::new(),
    };

    let daemon_url = daemon_url.trim_end_matches('/');
    let response = http_client
        .post(format!("{daemon_url}/v1/ingest"))
        .json(&request)
        .send()
        .await
        .context("ingest request failed")?
        .error_for_status()
        .context("ingest returned error status")?
        .json::<IngestResponse>()
        .await
        .context("decode ingest response")?;

    if !response.stream_ready {
        let ack = format_ingest_ack(&response);
        if !ack.trim().is_empty() {
            send_whatsapp_reply(ctx, &ack).await?;
        }
        return Ok(());
    }

    match wait_for_ask_delivery(http_client, daemon_url, &response).await? {
        AdapterDeliveryOutcome::PushDelivered => {}
        AdapterDeliveryOutcome::Fallback { text } => {
            send_whatsapp_reply(ctx, &truncate_for_whatsapp(&text)).await?;
        }
        AdapterDeliveryOutcome::StreamError { message } => {
            send_whatsapp_reply(ctx, &truncate_for_whatsapp(&message)).await?;
        }
    }

    Ok(())
}

async fn send_whatsapp_reply(ctx: &MessageContext, text: &str) -> Result<()> {
    let message = wa::Message {
        conversation: Some(truncate_for_whatsapp(text)),
        ..Default::default()
    };
    ctx.send_message(message)
        .await
        .context("whatsapp reply send failed")?;
    Ok(())
}

async fn wait_for_ask_delivery(
    client: &Client,
    daemon_url: &str,
    response: &IngestResponse,
) -> Result<AdapterDeliveryOutcome> {
    let job_id = response
        .job_id
        .as_deref()
        .ok_or_else(|| anyhow!("missing job_id on stream_ready ingest response"))?;

    let deadline = tokio::time::Instant::now() + DEFAULT_DELIVERY_TIMEOUT;
    loop {
        let poll = poll_delivery_status(client, daemon_url, job_id).await?;
        match poll.status.as_str() {
            "delivered" => return Ok(AdapterDeliveryOutcome::PushDelivered),
            "failed" => {
                let message = poll
                    .error
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "request failed".to_string());
                return Ok(AdapterDeliveryOutcome::StreamError { message });
            }
            _ if tokio::time::Instant::now() >= deadline => break,
            _ => tokio::time::sleep(DEFAULT_POLL_INTERVAL).await,
        }
    }

    let result = fetch_job_result(client, daemon_url, job_id).await?;
    if result.status == "succeeded" {
        let text = result
            .output_text
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "(empty response)".to_string());
        return Ok(AdapterDeliveryOutcome::Fallback { text });
    }

    Ok(AdapterDeliveryOutcome::StreamError {
        message: result
            .latest_outcome
            .unwrap_or_else(|| format!("job ended with status={}", result.status)),
    })
}

async fn poll_delivery_status(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<DeliverPollResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/deliver/poll/{job_id}"))
        .send()
        .await
        .context("failed to reach deliver poll endpoint")?
        .error_for_status()
        .context("deliver poll endpoint returned error")?;

    response
        .json::<DeliverPollResponse>()
        .await
        .context("failed to decode deliver poll response")
}

async fn fetch_job_result(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<JobResultResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/jobs/{job_id}/result"))
        .send()
        .await
        .context("failed to reach job result endpoint")?
        .error_for_status()
        .context("job result endpoint returned error")?;

    response
        .json::<JobResultResponse>()
        .await
        .context("failed to decode job result response")
}

fn format_ingest_ack(response: &IngestResponse) -> String {
    if response.is_new_session {
        format!("🆕 {}\n\n{}", response.reply, ADAPTER_COMMAND_HINT)
    } else {
        response.reply.clone()
    }
}

fn truncate_for_whatsapp(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

fn parse_whatsapp_chat_jid(channel_id: &str) -> Result<Jid> {
    let jid = channel_id
        .strip_prefix("whatsapp:chat:")
        .or_else(|| channel_id.strip_prefix("whatsapp:"))
        .context("whatsapp channel_id must be whatsapp:chat:<jid>")?;
    Jid::from_str(jid).with_context(|| format!("invalid whatsapp jid: {jid}"))
}

fn resolve_daemon_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DAEMON_URL"))
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

fn default_session_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("whatsapp")
        .join("session.db")
}

fn resolve_session_db_path(explicit: Option<&str>) -> PathBuf {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| non_empty_env("MEDOUSA_WHATSAPP_SESSION_DB").map(PathBuf::from))
        .unwrap_or_else(default_session_db_path)
}

fn resolve_deliver_bind(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_WHATSAPP_DELIVER_BIND"))
        .unwrap_or_else(|| DEFAULT_DELIVER_BIND.to_string())
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn find_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn print_usage() {
    println!("medousa_whatsapp — thin WhatsApp adapter (whatsapp-rust)");
    println!();
    println!("USAGE:");
    println!("  medousa_whatsapp [--daemon-url <url>] [--deliver-bind <host:port>] [--session-db <path>]");
    println!();
    println!("ENV:");
    println!("  MEDOUSA_DAEMON_URL");
    println!("  MEDOUSA_WHATSAPP_DELIVER_BIND (default 127.0.0.1:7422)");
    println!("  MEDOUSA_WHATSAPP_SESSION_DB (default ~/.local/share/medousa/whatsapp/session.db)");
    println!("  WHATSAPP_WS_URL (optional transport override)");
    println!();
    println!("NOTE: First run prints a QR code for WhatsApp Linked Devices pairing.");
    println!("WhatsApp Web clients are unofficial — review Meta ToS before production use.");
}
