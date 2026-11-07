#[cfg(test)]
#[path = "backend_tests.rs"]
mod tests;
use futures_util::StreamExt;
use medousa_sdk::{HttpTransport, MedousaClient};
use medousa_types::{
    ActiveSessionTurn, CreateSessionRequest, InteractiveTurnRequest, TurnStreamEnvelopeV3,
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
}
pub struct Request {
    pub generation: u64,
    pub command: Command,
}
#[derive(Clone)]
pub enum Update {
    History(String, String, Option<ActiveSessionTurn>),
    Admitted(String),
    Stream(TurnStreamEnvelopeV3),
    Models(Vec<(String, String)>),
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

async fn history(client: &MedousaClient, id: &str) -> Result<Update, medousa_sdk::SdkError> {
    let history = client.sessions().history(id).await?;
    let active = client.sessions().active_turn(id).await?;
    let transcript = history
        .turns
        .iter()
        .map(|entry| {
            format!(
                "### {}\n\n{}",
                if entry.turn.role == "user" {
                    "You"
                } else {
                    "Medousa"
                },
                entry.turn.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(Update::History(id.into(), transcript, active.turn))
}

async fn follow(
    client: Arc<MedousaClient>,
    path: String,
    tx: feed::Sender<Reply>,
    generation: u64,
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
                if !emit(&tx, generation, Update::Stream(e)) || terminal {
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
                Ok(s) => history(&client, &s.session_id).await,
                Err(e) => Err(e),
            },
            Command::Recent => match client.sessions().list(1).await {
                Ok(list) => {
                    if let Some(s) = list.sessions.first() {
                        history(&client, &s.session_id).await
                    } else {
                        Ok(Update::Notice(
                            "No recent chats. Use + to create one.".into(),
                        ))
                    }
                }
                Err(e) => Err(e),
            },
            Command::Open(id) => history(&client, &id).await,
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
                    emit(&out, generation, Update::Admitted(response.turn_id));
                    if let Some(notice) = response.daemon_notice {
                        emit(&out, generation, Update::Notice(notice));
                    }
                    follow(c, response.stream_url, out, generation).await;
                }));
                continue;
            }
        };
        // Avoid raw server response bodies: they can carry sensitive diagnostics.
        let update = result.unwrap_or_else(|_| Update::Failure("Daemon request failed. Check local URL, data directory, authentication and runtime in existing Medousa app.".into()));
        let resume = match &update {
            Update::History(_, _, Some(turn)) => Some(turn.clone()),
            _ => None,
        };
        if !emit(&tx, generation, update) {
            break;
        }
        if let Some(turn) = resume {
            stream_task = Some(tokio::spawn(follow(
                client.clone(),
                turn.stream_url,
                tx.clone(),
                generation,
            )));
        }
    }
    if let Some(task) = stream_task {
        task.abort();
    }
}
