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
    Recent,
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
    History(String, String, Option<ActiveSessionTurn>),
    SettledHistory(String, String),
    Admitted(String),
    Stream(TurnStreamEnvelopeV3),
    Models(Vec<(String, String)>),
    Messages(Vec<(String, String)>),
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
    let origin = medousa_rndr_home::local_url(origin)?;
    // Same first-party local boundary as Tauri. Load only: never provision,
    // rotate, copy, or persist credentials. No pairing or remote fallback.
    let secret = medousa_local_credential::load_home_local_secret(data_dir)
        .map_err(|_| "Local credential unavailable. Start the daemon for this data directory using the existing Medousa app.".to_string())?;
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

#[cfg(test)]
async fn history(client: &MedousaClient, id: &str) -> Result<Update, medousa_sdk::SdkError> {
    history_with_messages(client, id)
        .await
        .map(|(update, _)| update)
}
async fn history_with_messages(
    client: &MedousaClient,
    id: &str,
) -> Result<(Update, Vec<(String, String)>), medousa_sdk::SdkError> {
    // These routes are not an atomic snapshot. Bracket the read and retry only
    // a bounded number of times if the observed execution changes/settles.
    for _ in 0..3 {
        let before = client.sessions().active_turn(id).await?;
        let history = client.sessions().history(id).await?;
        let active = client.sessions().active_turn(id).await?;
        if before.turn.as_ref().map(|t| &t.turn_id) != active.turn.as_ref().map(|t| &t.turn_id) {
            continue;
        }
        // The daemon persists user and finalized assistant entries with caused_by
        // = execution_ref(session, turn_id). Replay owns ALL assistant prose for
        // this active execution; committed users remain in canonical history.
        // Never deduplicate by text, timestamp, or segment-id guesses.
        let messages = history
            .turns
            .iter()
            .filter(|entry| {
                !(entry.turn.role == "assistant"
                    && active.turn.as_ref().is_some_and(|turn| {
                        entry.caused_by.as_ref().is_some_and(|cause| {
                            cause.authority_id == history.authority_id
                                && cause.session_id.as_str() == id
                                && cause.execution_id.as_str() == turn.turn_id
                        })
                    }))
            })
            .map(|entry| (entry.turn.role.clone(), entry.turn.content.clone()))
            .collect::<Vec<_>>();
        let transcript = messages
            .iter()
            .map(|(role, content)| {
                format!(
                    "### {}\n\n{}",
                    if role == "user" { "You" } else { "Medousa" },
                    content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok((
            Update::History(id.into(), transcript, active.turn),
            messages,
        ));
    }
    Ok((Update::Failure("History changed during three reconciliation attempts. Reload again; sending remains blocked. No atomic snapshot is available.".into()), Vec::new()))
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

#[cfg(test)]
async fn settled_history(client: &MedousaClient, session: &str) -> Update {
    settled_history_with_messages(client, session).await.0
}
async fn settled_history_with_messages(
    client: &MedousaClient,
    session: &str,
) -> (Update, Vec<(String, String)>) {
    // Settlement may precede active-ticket cleanup (handoffs can remain active).
    // Retry reads only, never admission, and do not claim the ticket has cleared.
    for attempt in 0..3 {
        match history_with_messages(client, session).await {
            Ok((Update::History(id, text, None), messages)) => return (Update::SettledHistory(id, text), messages),
            Ok((Update::History(_, _, Some(_)), _)) if attempt < 2 => {
                tokio::time::sleep(std::time::Duration::from_millis(25 << attempt)).await;
            }
            Ok((Update::History(_, _, Some(_)), _)) => return (Update::Failure("Stream settled but daemon still reports an active turn after bounded reads (possibly a handoff). Reload to reconcile/reattach; no automatic submission.".into()), Vec::new()),
            Ok(other) => return other,
            Err(_) => return (Update::Failure("Turn settled, but history reconciliation failed. Live in-memory text retained; reload before sending.".into()), Vec::new()),
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
    if medousa_rndr_home::stream_path(&path).is_err() {
        emit(
            &tx,
            generation,
            Update::Failure("Refused non-local stream path".into()),
        );
        return;
    }
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
                    let (update, messages) = if fatal {
                        // Persistence failure is itself fatal: replacing live text
                        // with incomplete history here would silently erase it.
                        (Update::Failure("Fatal turn: durable history may be incomplete. Live in-memory text retained; inspect/copy it before explicitly reloading.".into()), Vec::new())
                    } else {
                        settled_history_with_messages(&client, &session).await
                    };
                    let reconciled = matches!(update, Update::SettledHistory(_, _));
                    if !emit(&tx, generation, update) {
                        return;
                    }
                    if reconciled {
                        emit(&tx, generation, Update::Messages(messages));
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
            emit(&tx, 0, Update::Notice("Model catalog unavailable; no inference until a model is selected. Check Connection in existing app.".into()));
        }
    }
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
                    "Mode registry unavailable; mode availability could not be confirmed.".into(),
                ),
            );
        }
    }
    let mut stream_task: Option<tokio::task::JoinHandle<()>> = None;
    while let Some(Request {
        generation,
        command,
    }) = commands.recv().await
    {
        if matches!(command, Command::New | Command::Recent | Command::Open(_))
            && let Some(task) = stream_task.take()
        {
            task.abort();
            let _ = task.await;
        }
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
                Ok(s) => history_with_messages(&client, &s.session_id).await,
                Err(e) => Err(e),
            },
            Command::Recent => match client.sessions().list(1).await {
                Ok(list) => {
                    if let Some(s) = list.sessions.first() {
                        history_with_messages(&client, &s.session_id).await
                    } else {
                        Ok((
                            Update::Notice("No recent chats. Use + to create one.".into()),
                            Vec::new(),
                        ))
                    }
                }
                Err(e) => Err(e),
            },
            Command::Open(id) => history_with_messages(&client, &id).await,
            Command::SetMode(id, selected) => {
                Ok((set_mode(&client, &id, selected).await, Vec::new()))
            }
            Command::Cancel(id) => client
                .sessions()
                .cancel_active_turn(&id)
                .await
                .map(|r| (Update::Notice(r.message), Vec::new())),
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
                    emit(&out, generation, Update::Admitted(response.turn_id));
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
        let (update, messages) = result.unwrap_or_else(|_| (Update::Failure("Daemon request failed. Check local URL, data directory, authentication and runtime in existing Medousa app.".into()), Vec::new()));
        let mode_session = match &update {
            Update::History(id, _, _) => Some(id.clone()),
            _ => None,
        };
        let resume = match &update {
            Update::History(id, _, Some(turn)) => Some((id.clone(), turn.clone())),
            _ => None,
        };
        if !emit(&tx, generation, update) {
            break;
        }
        if let Some(session) = mode_session {
            if !emit(&tx, generation, Update::Messages(messages)) {
                break;
            }
            if !emit(&tx, generation, mode(&client, &session).await) {
                break;
            }
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
}
