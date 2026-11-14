//! Attach-only SDK transport for the native chat host.
//! Projection and execution state belong to app-core; this module only moves wire facts.
#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
use futures_util::StreamExt;
use medousa_sdk::{HttpTransport, MedousaClient};
use medousa_types::{
    ActiveSessionTurn, AgentModeId, AgentModeScope, CreateSessionRequest, InteractiveTurnRequest,
    SetSessionAgentModeRequest, TurnStreamEnvelopeV3,
};
use rndr_core::{Event, feed};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;

pub enum Command {
    New,
    List(String),
    More { query: String, cursor: String },
    Rename { session: String, title: String },
    Delete(String),
    Fork(String),
    Pin { session: String, pinned: bool },
    Open(String),
    Send(Box<InteractiveTurnRequest>),
    Cancel(String),
    SetMode(String, AgentModeId),
}
pub struct Request {
    pub generation: u64,
    pub command: Command,
}
#[derive(Clone)]
pub enum Update {
    History(
        String,
        String,
        Vec<serde_json::Value>,
        Option<ActiveSessionTurn>,
    ),
    SettledHistory(String, String, Vec<serde_json::Value>),
    Admitted(String, String),
    Stream(TurnStreamEnvelopeV3),
    Models(Vec<(String, String)>),
    Sessions(Vec<(String, String)>),
    SessionsPage {
        query: String,
        rows: Vec<(String, String)>,
        cursor: Option<String>,
        append: bool,
    },
    Renamed {
        session: String,
        title: String,
    },
    Deleted(String),
    Forked {
        source: String,
        session: String,
    },
    Mode(String),
    Modes(Vec<(String, String, bool)>),
    Notice(String),
    Failure(String),
    Unavailable(String),
}
#[derive(Clone)]
pub struct Reply {
    pub generation: u64,
    pub update: Update,
}
impl Event for Reply {}
fn emit(tx: &feed::Sender<Reply>, generation: u64, update: Update) -> bool {
    tx.send(Reply { generation, update }).is_ok()
}

pub fn client(origin: &str, data_dir: &std::path::Path) -> Result<MedousaClient, String> {
    let origin = medousa_desktop_host::connection::local_url(origin)?;
    // Same first-party local boundary as Tauri. Load only: never provision,
    // rotate, copy, or persist credentials. No pairing or remote fallback.
    let secret = medousa_local_credential::load_home_local_secret(data_dir)
        .map_err(|_| "Local credential unavailable for the selected workshop. Check its daemon startup/auth state and matching data directory; Medousa does not create or copy credentials.".to_string())?;
    let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", secret.token()))
        .map_err(|_| "Invalid local credential".to_string())?;
    auth.set_sensitive(true);
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::AUTHORIZATION, auth);
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5))
        .read_timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|_| "Could not initialize HTTP transport")?;
    Ok(MedousaClient::with_transport(
        Arc::new(HttpTransport::with_client(http)),
        origin.to_string(),
    ))
}

// SDK stream URLs are relative to the authenticated local origin. Reject
// traversal and URL forms that could escape it before the SDK sees them.
fn stream_path<'a>(path: &'a str, origin: &str) -> Result<&'a str, ()> {
    // Never follow a ticket's advertised host: a local daemon can advertise its
    // LAN listener even when this client attached on loopback. Normalize only
    // the canonical interactive route back onto our authenticated SDK origin.
    let path = if path.starts_with('/') {
        path
    } else if let Some(relative) = path.strip_prefix(origin.trim_end_matches('/')) {
        relative
    } else {
        let url = reqwest::Url::parse(path).map_err(|_| ())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(());
        }
        let (_, authority_and_path) = path.split_once("://").ok_or(())?;
        let start = authority_and_path.find('/').ok_or(())?;
        let relative = &authority_and_path[start..];
        let id = relative
            .strip_prefix("/v1/interactive/turn/")
            .and_then(|p| p.strip_suffix("/stream"))
            .ok_or(())?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err(());
        }
        relative
    };
    if !path.starts_with("/v1/")
        || path.contains(['\\', '#', '?'])
        || path.contains("://")
        || path.split('/').any(|part| matches!(part, "." | ".."))
        || path.contains('%')
    {
        return Err(());
    }
    Ok(path)
}

async fn sessions(
    client: &MedousaClient,
    query: &str,
    cursor: Option<&str>,
) -> Result<Update, medousa_sdk::SdkError> {
    let query = query.trim();
    let mut params = vec![("limit", "100".to_string())];
    if !query.is_empty() {
        params.push(("q", query.to_string()));
    }
    if let Some(cursor) = cursor {
        params.push(("cursor", cursor.to_string()));
    }
    // op_path_query is private to the SDK. Public get_query uses the same
    // SDK path_with_query encoder, with the canonical generated operation.
    let page: medousa_types::SessionHistoryListResponse = client
        .http()
        .get_query(medousa_sdk::operations::SESSIONS_GET.path, &params)
        .await?;
    let rows = page
        .sessions
        .into_iter()
        .map(|session| {
            let label = session
                .display_name
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| {
                    if session.preview.is_empty() {
                        session.session_id.clone()
                    } else {
                        session.preview
                    }
                });
            (session.session_id, label)
        })
        .collect();
    Ok(Update::SessionsPage {
        query: query.into(),
        rows,
        cursor: page.next_cursor,
        append: cursor.is_some(),
    })
}

async fn fork(client: &MedousaClient, source: &str) -> Result<Update, medousa_sdk::SdkError> {
    // Freeze the exact committed range before mutation. No active/live text is
    // copied, and neither derivation nor an unknown admission is auto-replayed.
    let snapshot = client.sessions().history(source).await?;
    let Some(through_entry_seq) = snapshot.turns.iter().map(|entry| entry.entry_seq).max() else {
        return Ok(Update::Notice("Cannot fork an empty conversation.".into()));
    };
    static NEXT_KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let key = format!(
        "native-fork-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_KEY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let result = client
        .sessions()
        .derive(
            &medousa_types::DeriveSessionRequest {
                sources: vec![medousa_types::ConversationRangeSelection {
                    session: medousa_types::SessionRef {
                        authority_id: snapshot.authority_id,
                        session_id: medousa_types::SessionId::parse(snapshot.session_id).map_err(
                            |_| {
                                medousa_sdk::SdkError::Serde(
                                    "Invalid history session identifier".into(),
                                )
                            },
                        )?,
                    },
                    after_entry_seq: None,
                    through_entry_seq,
                }],
                intent: "fork".into(),
                target: medousa_types::DeriveSessionTarget {
                    catalog: Some("single".into()),
                    display_name: Some("Forked conversation".into()),
                },
            },
            &key,
        )
        .await?;
    Ok(Update::Forked {
        source: source.into(),
        session: result.session_id,
    })
}

// Deliberately log only operation names and elapsed time, never IDs, prompts,
// credentials or server diagnostics.
fn timing(operation: &str, started: std::time::Instant) {
    eprintln!(
        "native backend {operation} elapsed_ms={}",
        started.elapsed().as_millis()
    );
}

async fn history(client: &MedousaClient, id: &str) -> Result<Update, medousa_sdk::SdkError> {
    let started = std::time::Instant::now();
    let result = read_history(client, id).await;
    timing("history", started);
    result
}

async fn read_history(client: &MedousaClient, id: &str) -> Result<Update, medousa_sdk::SdkError> {
    // These routes are not an atomic snapshot. Bracket the read and retry only
    // a bounded number of times if the observed execution changes/settles.
    for _ in 0..3 {
        let before = client.sessions().active_turn(id).await?;
        let history = client.sessions().history(id).await?;
        let active = client.sessions().active_turn(id).await?;
        if before.turn.as_ref().map(|t| &t.turn_id) != active.turn.as_ref().map(|t| &t.turn_id) {
            continue;
        }
        // Forward the full ledger wire contract. ChatCore owns projection,
        // history/live overlap and durable identity, not the transport.
        let turns = history
            .turns
            .into_iter()
            .map(|entry| serde_json::to_value(entry).expect("ledger entry serializes"))
            .collect();
        return Ok(Update::History(
            id.into(),
            history.authority_id.to_string(),
            turns,
            active.turn,
        ));
    }
    Ok(Update::Failure("History changed during three reconciliation attempts. Reload again; sending remains blocked. No atomic snapshot is available.".into()))
}

// Mode metadata is independent of transcript reconciliation. A failed read must
// not make a successfully loaded chat uncertain or replace its live prose.
async fn mode(client: &MedousaClient, session: &str) -> Update {
    match client.sessions().agent_mode(session).await {
        Ok(state) => Update::Mode(state.effective_mode.as_str().into()),
        Err(_) => Update::Notice("Session mode unavailable; reload to retry.".into()),
    }
}

async fn set_mode(client: &MedousaClient, session: &str, mode: AgentModeId) -> Update {
    match client
        .sessions()
        .set_agent_mode(
            session,
            &SetSessionAgentModeRequest {
                mode,
                scope: AgentModeScope::Session,
                task_id: None,
                expires_at_utc: None,
            },
        )
        .await
    {
        // A task lease may override the selected session mode. Always report
        // the server's effective value rather than optimistically echoing input.
        Ok(state) => Update::Mode(state.effective_mode.as_str().into()),
        Err(_) => Update::Notice(
            "Mode change failed; outcome may be unknown. Reload to reconcile before retrying."
                .into(),
        ),
    }
}

async fn settled_history(client: &MedousaClient, session: &str) -> Update {
    // Settlement may precede active-ticket cleanup (handoffs can remain active).
    // Retry reads only, never admission, and do not claim the ticket has cleared.
    for attempt in 0..3 {
        match history(client, session).await {
            Ok(Update::History(id, authority, messages, None)) => return Update::SettledHistory(id, authority, messages),
            Ok(Update::History(_, _, _, Some(_))) if attempt < 2 => {
                tokio::time::sleep(std::time::Duration::from_millis(25 << attempt)).await;
            }
            Ok(Update::History(_, _, _, Some(_))) => return Update::Failure("Stream settled but daemon still reports an active turn after bounded reads (possibly a handoff). Reload to reconcile/reattach; no automatic submission.".into()),
            Ok(other) => return other,
            Err(_) => return Update::Failure("Turn settled, but history reconciliation failed. Live in-memory text retained; reload before sending.".into()),
        }
    }
    unreachable!("bounded settlement reads return an update")
}

async fn follow(
    client: Arc<MedousaClient>,
    path: String,
    tx: feed::Sender<Reply>,
    generation: u64,
    session: String,
) {
    let path = match stream_path(&path, client.base_url()) {
        Ok(path) => path.to_string(),
        Err(()) => {
            emit(
                &tx,
                generation,
                Update::Failure("Refused non-local stream path".into()),
            );
            return;
        }
    };
    let api = client.interactive();
    let mut stream = api.stream_reconnecting_v3(path);
    while let Some(event) = stream.next().await {
        match event {
            Ok(e) => {
                let terminal = e.event.is_terminal();
                let fatal = matches!(
                    &e.event,
                    medousa_types::TurnStreamEventV3::TurnCompleted {
                        outcome: medousa_types::TurnCompletionOutcomeV3::Fatal,
                        ..
                    }
                );
                if !emit(&tx, generation, Update::Stream(e)) {
                    return;
                }
                if terminal {
                    // User persistence can occur after the initial history read.
                    // Settlement is published after durable assistant persistence;
                    // replace replay with canonical history, including that user.
                    let update = if fatal {
                        // Persistence failure is itself fatal: replacing live text
                        // with incomplete history here would silently erase it.
                        Update::Failure("Fatal turn: durable history may be incomplete. Live in-memory text retained; inspect/copy it before explicitly reloading.".into())
                    } else {
                        settled_history(&client, &session).await
                    };
                    let reconciled = matches!(update, Update::SettledHistory(_, _, _));
                    if !emit(&tx, generation, update) {
                        return;
                    }
                    if reconciled {
                        emit(&tx, generation, mode(&client, &session).await);
                    }
                    return;
                }
            }
            Err(_) => {
                emit(
                    &tx,
                    generation,
                    Update::Failure("Stream disconnected after bounded SDK reconnect".into()),
                );
                return;
            }
        }
    }
    emit(
        &tx,
        generation,
        Update::Failure("Stream ended without settlement".into()),
    );
}

pub fn run(
    origin: String,
    data_dir: PathBuf,
    commands: mpsc::UnboundedReceiver<Request>,
    tx: feed::Sender<Reply>,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("network runtime");
    runtime.block_on(async move {
        let client = match client(&origin, &data_dir) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                emit(&tx, 0, Update::Unavailable(e));
                return;
            }
        };
        serve(client, commands, tx).await;
    });
}
async fn serve(
    client: Arc<MedousaClient>,
    mut commands: mpsc::UnboundedReceiver<Request>,
    tx: feed::Sender<Reply>,
) {
    // Independent, read-only catalogs must not serialize startup or gate history.
    // Own these tasks until shutdown rather than detaching network work.
    let mut metadata_tasks = tokio::task::JoinSet::new();
    {
        let client = client.clone();
        let tx = tx.clone();
        metadata_tasks.spawn(async move {
            let started = std::time::Instant::now();
            let initial = sessions(&client, "", None).await.unwrap_or_else(|_| {
                Update::Notice(
                    "Chat list unavailable. Check the local daemon connection and retry.".into(),
                )
            });
            emit(&tx, 0, initial);
            timing("sessions", started);
        });
    }
    {
        let client = client.clone();
        let tx = tx.clone();
        metadata_tasks.spawn(async move {
            let started = std::time::Instant::now();
            // Read-only catalog, never refreshes a provider or sends inference.
            match client
                .http()
                .get::<serde_json::Value>("/v1/models/catalog")
                .await
            {
                Ok(v) => {
                    let models = v["models"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|m| {
                            Some((
                                m["provider"].as_str()?.into(),
                                m["model_id"].as_str()?.into(),
                            ))
                        })
                        .collect();
                    emit(&tx, 0, Update::Models(models));
                }
                Err(_) => {
                    emit(&tx, 0, Update::Notice(concat!("Model catalog unavailable; no inference until a model is selected. ", "Check the local daemon model configuration.").into()));
                }
            }
            timing("models", started);
        });
    }
    {
        let client = client.clone();
        let tx = tx.clone();
        metadata_tasks.spawn(async move {
            let started = std::time::Instant::now();
            match client.runtime().agent_modes().await {
                Ok(registry) => {
                    let modes = registry
                        .modes
                        .into_iter()
                        .map(|m| (m.mode.as_str().into(), m.label, m.available))
                        .collect();
                    emit(&tx, 0, Update::Modes(modes));
                }
                Err(_) => {
                    emit(
                        &tx,
                        0,
                        Update::Notice(
                            "Mode registry unavailable; mode availability could not be confirmed."
                                .into(),
                        ),
                    );
                }
            }
            timing("modes", started);
        });
    }
    let mut stream_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut mode_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut view_generation = 0;
    while let Some(Request {
        generation,
        command,
    }) = commands.recv().await
    {
        while metadata_tasks.try_join_next().is_some() {}
        if !matches!(command, Command::List(_) | Command::More { .. }) {
            if generation < view_generation {
                // Never admit queued stale mutations after navigation.
                continue;
            }
            view_generation = generation;
        }
        // A read launched by Open must not overwrite a newer mode mutation,
        // even when both commands belong to the same view generation.
        if matches!(
            command,
            Command::New | Command::Open(_) | Command::SetMode(_, _)
        ) && let Some(task) = mode_task.take()
        {
            task.abort();
            let _ = task.await;
        }
        if matches!(command, Command::Send(_))
            && stream_task.as_ref().is_some_and(|task| !task.is_finished())
        {
            emit(
                &tx,
                generation,
                Update::Notice("A turn is already active. Cancel or wait before sending.".into()),
            );
            continue;
        }
        if matches!(command, Command::New | Command::Open(_))
            && let Some(task) = stream_task.take()
        {
            task.abort();
            let _ = task.await;
        }
        let metadata = matches!(command, Command::List(_) | Command::More { .. });
        let mutation = matches!(
            command,
            Command::Rename { .. } | Command::Delete(_) | Command::Fork(_)
        );
        let result = match command {
            Command::New => match client
                .sessions()
                .create(&CreateSessionRequest {
                    session_id: None,
                    catalog: Some("single".into()),
                    member_profile_ids: None,
                    agent_profile_id: None,
                    display_name: Some("Today".into()),
                })
                .await
            {
                Ok(s) => history(&client, &s.session_id).await,
                Err(e) => Err(e),
            },
            Command::List(query) => sessions(&client, &query, None).await,
            Command::More { query, cursor } => sessions(&client, &query, Some(&cursor)).await,
            Command::Rename { session, title } => client
                .sessions()
                .set_display_name(&session, &title)
                .await
                .map(|r| Update::Renamed {
                    session: r.session_id,
                    title: r.display_name,
                }),
            Command::Delete(session) => client
                .sessions()
                .delete(
                    &session,
                    &medousa_types::SessionDeleteQuery {
                        purge_memory: false,
                    },
                )
                .await
                .map(|r| {
                    if r.deleted && r.status == medousa_types::SessionDeletionStatus::Complete {
                        Update::Deleted(r.session_id)
                    } else {
                        Update::Notice(
                            "Deletion is not complete. Reload to reconcile; no automatic retry."
                                .into(),
                        )
                    }
                }),
            Command::Fork(session) => fork(&client, &session).await,
            // Tauri pins are local-storage UI preferences, not daemon session
            // facts. Never invent a pin route or falsely acknowledge a write.
            Command::Pin { session, pinned } => Ok(Update::Notice(format!(
                "{} {session}: pinning is a local UI preference; no daemon pin endpoint is available.",
                if pinned { "Pin" } else { "Unpin" }
            ))),
            Command::Open(id) => history(&client, &id).await,
            Command::SetMode(id, selected) => Ok(set_mode(&client, &id, selected).await),
            Command::Cancel(id) => client
                .sessions()
                .cancel_active_turn(&id)
                .await
                .map(|r| Update::Notice(r.message)),
            Command::Send(request) => {
                let c = client.clone();
                let out = tx.clone();
                stream_task = Some(tokio::spawn(async move {
                    let response = match c.interactive().start_turn(&request).await {
                        Ok(r) => r,
                        Err(_) => {
                            emit(&out, generation, Update::Failure("Turn admission failed (check daemon/auth); outcome may be unknown".into()));
                            return;
                        }
                    };
                    emit(
                        &out,
                        generation,
                        Update::Admitted(response.turn_id, request.prompt.clone()),
                    );
                    if let Some(notice) = response.daemon_notice {
                        emit(&out, generation, Update::Notice(notice));
                    }
                    follow(
                        c,
                        response.stream_url,
                        out,
                        generation,
                        request.session_id.clone(),
                    )
                    .await;
                }));
                continue;
            }
        };
        // Avoid raw server response bodies: they can carry sensitive diagnostics.
        let update = result.unwrap_or_else(|_| {
            if metadata {
                Update::Notice("Chat list/search unavailable. Check the local daemon connection and retry.".into())
            } else if mutation {
                Update::Notice("Session change failed; outcome may be unknown. Reload to reconcile before retrying; no automatic replay.".into())
            } else {
                Update::Failure("Daemon request failed. Check local URL, data directory, authentication and runtime in Medousa.".into())
            }
        });
        let mode_session = match &update {
            Update::History(id, _, _, _) => Some(id.clone()),
            _ => None,
        };
        let resume = match &update {
            Update::History(id, _, _, Some(turn)) => Some((id.clone(), turn.clone())),
            _ => None,
        };
        if !emit(&tx, generation, update) {
            break;
        }
        if let Some(session) = mode_session {
            let client = client.clone();
            let tx = tx.clone();
            mode_task = Some(tokio::spawn(async move {
                emit(&tx, generation, mode(&client, &session).await);
            }));
        }
        if let Some((session, turn)) = resume {
            stream_task = Some(tokio::spawn(follow(
                client.clone(),
                turn.stream_url,
                tx.clone(),
                generation,
                session,
            )));
        }
    }
    if let Some(task) = stream_task {
        task.abort();
    }
    if let Some(task) = mode_task {
        let _ = task.await;
    }
    while metadata_tasks.join_next().await.is_some() {}
}
