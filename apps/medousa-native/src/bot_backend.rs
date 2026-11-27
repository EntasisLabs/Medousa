//! Off-frame, authority-fenced Bot operations using the daemon's durable profiles.
//! Call through the owning workshop's Auxiliary worker, never the focused chat's
//! client. The caller must reject updates whose binding/epoch is no longer current
//! and serialize profile saves. Mutations are never automatically retried.
use crate::routing::{Binding, SessionKey};
use medousa_sdk::MedousaClient;
use medousa_types::{
    BotId, BotOpenResponse, BotProfile, CreateBotRequest, CreateManuscriptRequest,
    UpdateBotRequest, UpdateManuscriptRequest,
};

#[derive(Debug, Clone)]
pub enum Command {
    List,
    Create(CreateBotRequest),
    CreatePersonalized {
        display_name: String,
        role_description: Option<String>,
        avatar_ref: Option<String>,
        instructions: String,
    },
    GetInstructions {
        manuscript_id: String,
    },
    SaveInstructions {
        manuscript_id: String,
        instructions: String,
    },
    Update {
        bot_id: String,
        request: UpdateBotRequest,
    },
    /// The daemon resumes the primary conversation, creating a replacement only
    /// when necessary. Never implement this by creating an ordinary chat locally.
    Open {
        bot_id: String,
    },
}
#[derive(Debug, Clone)]
pub struct Request {
    pub binding: Binding,
    pub command: Command,
}
#[derive(Debug, Clone)]
pub enum Outcome {
    Listed(Vec<BotProfile>),
    Created(BotOpenResponse),
    Updated(BotProfile),
    Instructions {
        manuscript_id: String,
        instructions: String,
    },
    Opened(BotOpenResponse),
}
#[derive(Debug, Clone)]
pub struct Update {
    pub binding: Binding,
    pub result: Result<Outcome, String>,
}
impl Update {
    pub fn accepts(&self, current: &Binding) -> bool {
        current.accepts(&self.binding)
    }
    /// Composite native handle, not the raw daemon session id. Only navigate
    /// after accepting this update against the current view's binding.
    pub fn session_handle(&self) -> Option<String> {
        match self.result.as_ref().ok()? {
            Outcome::Created(open) | Outcome::Opened(open) => {
                Some(SessionKey::new(&self.binding.authority, &open.binding.session_id).handle())
            }
            _ => None,
        }
    }
}

/// Preserve hidden capabilities and the optimistic revision when editing the
/// simple identity fields. In particular, a rename must not erase manuscripts,
/// durable world continuity, or an installed external executor.
pub fn edit_request(profile: &BotProfile) -> UpdateBotRequest {
    UpdateBotRequest {
        expected_revision: profile.revision,
        display_name: profile.display_name.clone(),
        role_description: profile.role_description.clone(),
        avatar_ref: profile.avatar_ref.clone(),
        primary_manuscript_id: profile.primary_manuscript_id.clone(),
        additional_manuscript_ids: profile.additional_manuscript_ids.clone(),
        default_mode: profile.default_mode,
        world_binding: profile.world_binding.clone(),
        clear_world_binding: false,
        external_agent: profile.external_agent.clone(),
        clear_external_agent: false,
    }
}
fn check_authority(expected: &str, actual: &str) -> Result<(), String> {
    if expected.is_empty() || expected != actual {
        Err("Bot operation belongs to a different workshop; no fallback was attempted.".into())
    } else {
        Ok(())
    }
}
fn validate(command: &Command) -> Result<(), String> {
    match command {
        Command::Update { bot_id, .. } | Command::Open { bot_id } => {
            BotId::parse(bot_id).map_err(|e| e.to_string())?;
        }
        Command::CreatePersonalized { display_name, .. } if display_name.trim().is_empty() => {
            return Err("Give your Bot a name before creating it.".into());
        }
        Command::GetInstructions { manuscript_id }
        | Command::SaveInstructions { manuscript_id, .. }
            if manuscript_id.trim().is_empty() =>
        {
            return Err("Bot has no primary manuscript.".into());
        }
        _ => {}
    }
    Ok(())
}

pub async fn execute(client: &MedousaClient, request: Request) -> Update {
    let result = execute_inner(client, &request).await;
    Update {
        binding: request.binding,
        result,
    }
}
async fn execute_inner(client: &MedousaClient, request: &Request) -> Result<Outcome, String> {
    validate(&request.command)?;
    if request.binding.authority.is_empty() {
        return Err("Choose an owning workshop before accessing Bots.".into());
    }
    // Identity comes from authenticated /v1/health through the same pinned SDK
    // transport, never the anonymous liveness endpoint or a URL-derived label.
    let health = client.health().get().await.map_err(|e| e.to_string())?;
    check_authority(
        &request.binding.authority,
        health.runtime.authority_id.as_ref(),
    )?;
    // These additional flows still use the same authority-checked client.
    match &request.command {
        Command::CreatePersonalized {
            display_name,
            role_description,
            avatar_ref,
            instructions,
        } => {
            let manuscript = client.manuscripts().create(&CreateManuscriptRequest {
                name: format!("{} identity", display_name.trim()),
                description: role_description.clone(),
                scope: Some("user".into()),
                template: None,
            }).await.map_err(|e| format!("Identity manuscript creation failed; outcome may be unknown. Refresh manuscripts before retrying: {e}"))?;
            if !instructions.trim().is_empty() {
                save_instructions(client, &manuscript.id, instructions).await.map_err(|e| format!("Manuscript '{}' was created but behavior save failed; no Bot was created. Keep this manuscript for recovery: {e}", manuscript.id))?;
            }
            let body = CreateBotRequest {
                display_name: display_name.trim().into(),
                role_description: role_description.clone(),
                avatar_ref: avatar_ref.clone(),
                primary_manuscript_id: manuscript.id.clone(),
                additional_manuscript_ids: vec![],
                default_mode: None,
                world_binding: None,
                external_agent: None,
            };
            return client.bots().create(&body).await.map(Outcome::Created).map_err(|e| format!("Identity manuscript '{}' was saved but Bot creation failed or its outcome is unknown. Refresh Bots before retrying; no rollback was attempted: {e}", manuscript.id));
        }
        Command::GetInstructions { manuscript_id } => {
            let manuscript = client
                .manuscripts()
                .get(manuscript_id)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Outcome::Instructions {
                manuscript_id: manuscript.id,
                instructions: manuscript.voice_appendix.unwrap_or_default(),
            });
        }
        Command::SaveInstructions {
            manuscript_id,
            instructions,
        } => {
            return save_instructions(client, manuscript_id, instructions).await;
        }
        _ => {}
    }
    let api = client.bots();
    let result = match &request.command {
        Command::List => api.list().await.map(|r| Outcome::Listed(r.bots)),
        Command::Create(body) => api.create(body).await.map(Outcome::Created),
        Command::Update { bot_id, request } => {
            api.update(bot_id, request).await.map(Outcome::Updated)
        }
        Command::Open { bot_id } => api.open(bot_id).await.map(Outcome::Opened),
        Command::CreatePersonalized { .. }
        | Command::GetInstructions { .. }
        | Command::SaveInstructions { .. } => unreachable!("handled above"),
    };
    result.map_err(|e| e.to_string())
}

async fn save_instructions(
    client: &MedousaClient,
    manuscript_id: &str,
    instructions: &str,
) -> Result<Outcome, String> {
    let manuscript = client
        .manuscripts()
        .update(
            manuscript_id,
            &UpdateManuscriptRequest {
                voice_appendix: Some(instructions.into()),
                clear_voice_appendix: Some(instructions.trim().is_empty()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(Outcome::Instructions {
        manuscript_id: manuscript.id,
        instructions: manuscript.voice_appendix.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const BOT: &str = "bot_0123456789abcdef0123456789abcdef";
    fn profile() -> BotProfile {
        serde_json::from_value(json!({
            "schema_version":3,"bot_id":BOT,"owner_profile_id":"user:default",
            "display_name":"Ada","role_description":"Build carefully","avatar_ref":"avatar:ada",
            "primary_manuscript_id":"main","additional_manuscript_ids":["extra"],
            "memory_scope_id":"memory:ada","primary_session_id":"primary-chat","revision":7,
            "world_binding":{"kind":"persistent_browser","world_id":"browser","execution_runtime_id":"runtime"},
            "external_agent":{"runtime":"codex","home_workshop_id":"workshop","forge_work_id":"work","forge_repo_id":"repo","session_contract":"fresh_per_job"},
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"
        })).unwrap()
    }
    #[test]
    fn simple_edit_preserves_revision_and_hidden_configuration() {
        let profile = profile();
        let request = edit_request(&profile);
        assert_eq!(request.expected_revision, 7);
        assert_eq!(request.primary_manuscript_id, profile.primary_manuscript_id);
        assert_eq!(
            request.additional_manuscript_ids,
            profile.additional_manuscript_ids
        );
        assert_eq!(request.world_binding, profile.world_binding);
        assert_eq!(request.external_agent, profile.external_agent);
        assert!(!request.clear_world_binding && !request.clear_external_agent);
    }
    #[test]
    fn cross_authority_and_stale_epoch_are_rejected() {
        assert!(check_authority("a", "b").is_err());
        assert!(check_authority("", "").is_err());
        assert!(check_authority("a", "a").is_ok());
        let binding = Binding::new("a".into());
        let update = Update {
            binding: binding.clone(),
            result: Ok(Outcome::Listed(vec![])),
        };
        assert!(update.accepts(&binding));
        let mut current = binding;
        current.select("b".into());
        current.select("a".into());
        assert!(!update.accepts(&current));
    }
    #[test]
    fn open_handle_uses_owner_and_daemon_binding_not_profile_hint() {
        let open = BotOpenResponse {
            bot: profile(),
            binding: serde_json::from_value(json!({"bot_id":BOT,"session_id":"canonical-chat","kind":"primary","bot_revision_at_bind":7,"created_at":"2026-01-01T00:00:00Z"})).unwrap(),
        };
        let update = Update {
            binding: Binding::new("a".into()),
            result: Ok(Outcome::Opened(open)),
        };
        let key = SessionKey::parse(&update.session_handle().unwrap()).unwrap();
        assert_eq!(key.authority, "a");
        assert_eq!(key.session, "canonical-chat");
    }
    #[tokio::test]
    async fn invalid_or_unowned_requests_fail_before_network() {
        let client = MedousaClient::new("http://127.0.0.1:1");
        for (owner, command) in [
            (
                "a",
                Command::Open {
                    bot_id: "../bad".into(),
                },
            ),
            ("", Command::List),
        ] {
            let update = execute(
                &client,
                Request {
                    binding: Binding::new(owner.into()),
                    command,
                },
            )
            .await;
            assert!(update.result.is_err());
            assert!(!update.result.unwrap_err().contains("error sending request"));
        }
    }
    fn health() -> serde_json::Value {
        json!({"runtime": {
            "authority_id":"auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "product_version":"test", "build_revision":"test",
            "contract_revision":medousa_types::DAEMON_API_CONTRACT_REVISION,
            "base_schema_revision":1,"deployment_profile":"test","deployment_target":"test",
            "advertised_capabilities":[]
        },"status":"ok","backend":"test","worker_id":"test","now_utc":"2026-01-01T00:00:00Z"})
    }
    fn manuscript() -> serde_json::Value {
        json!({"id":"ada-identity","name":"Ada identity","scope":"user","path":"not-a-local-path",
            "voice_appendix":"Be kind", "tools_allow":[], "openshell":{
                "enabled":false,"allow_scheduled":false,"default_path":""},
            "has_scripts":false,"scheduled_tools":[],"schedule_ready":false,"palette_tools":[]})
    }
    // Real SDK HTTP calls, without starting a daemon or mutating a user's store.
    fn server(
        steps: Vec<(&'static str, u16, serde_json::Value)>,
    ) -> (MedousaClient, std::thread::JoinHandle<Vec<String>>) {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = std::thread::spawn(move || {
            let mut requests = vec![];
            for (expected, status, response) in steps {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "missing request: {expected}");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                loop {
                    let mut buffer = [0u8; 1024];
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let size = String::from_utf8_lossy(&bytes[..end])
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|n| n.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + size {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.starts_with(expected), "{request}");
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer bot-test")
                );
                requests.push(request);
                let body = response.to_string();
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            "Bearer bot-test".parse().unwrap(),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        let client = MedousaClient::with_transport(
            std::sync::Arc::new(medousa_sdk::HttpTransport::with_client(http)),
            url,
        );
        (client, task)
    }
    fn request(command: Command) -> Request {
        Request {
            binding: Binding::new(
                "auth_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ),
            command,
        }
    }
    fn opened() -> serde_json::Value {
        json!({"bot":profile(),"binding":{"bot_id":BOT,"session_id":"canonical-chat","kind":"primary","bot_revision_at_bind":7,"created_at":"2026-01-01T00:00:00Z"}})
    }
    #[tokio::test]
    async fn sdk_list_and_open_use_authenticated_owner_and_canonical_binding() {
        let (client, server) = server(vec![
            ("GET /v1/health ", 200, health()),
            ("GET /v1/bots ", 200, json!({"bots":[profile()]})),
            ("GET /v1/health ", 200, health()),
            (
                "POST /v1/bots/bot_0123456789abcdef0123456789abcdef/open ",
                200,
                opened(),
            ),
        ]);
        assert!(
            matches!(execute(&client,request(Command::List)).await.result,Ok(Outcome::Listed(bots)) if bots.len()==1)
        );
        let update = execute(&client, request(Command::Open { bot_id: BOT.into() })).await;
        assert_eq!(
            SessionKey::parse(&update.session_handle().unwrap())
                .unwrap()
                .session,
            "canonical-chat"
        );
        server.join().unwrap();
    }
    #[tokio::test]
    async fn authority_mismatch_makes_no_bot_mutation() {
        let (client, server) = server(vec![("GET /v1/health ", 200, health())]);
        let update = execute(
            &client,
            Request {
                binding: Binding::new("other-owner".into()),
                command: Command::Open { bot_id: BOT.into() },
            },
        )
        .await;
        assert!(update.result.unwrap_err().contains("different workshop"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn personalized_create_uses_manuscript_persona_then_real_bot_create() {
        let (client, server) = server(vec![
            ("GET /v1/health ", 200, health()),
            ("POST /v1/manuscripts/create ", 200, manuscript()),
            ("PATCH /v1/manuscripts/ada-identity ", 200, manuscript()),
            ("POST /v1/bots ", 200, opened()),
        ]);
        let update = execute(
            &client,
            request(Command::CreatePersonalized {
                display_name: "Ada".into(),
                role_description: Some("Helper".into()),
                avatar_ref: Some("avatar:ada".into()),
                instructions: "Be kind".into(),
            }),
        )
        .await;
        assert!(
            matches!(update.result, Ok(Outcome::Created(_))),
            "{:?}",
            update.result
        );
        let requests = server.join().unwrap();
        assert!(requests[2].contains("\"voice_appendix\":\"Be kind\""));
        assert!(requests[3].contains("\"primary_manuscript_id\":\"ada-identity\""));
        assert!(!requests[3].contains("world_binding"));
        assert!(!requests[3].contains("external_agent"));
    }
    #[tokio::test]
    async fn partial_creation_failure_reports_retained_manuscript_without_retry() {
        let (client, server) = server(vec![
            ("GET /v1/health ", 200, health()),
            ("POST /v1/manuscripts/create ", 200, manuscript()),
            (
                "PATCH /v1/manuscripts/ada-identity ",
                500,
                json!({"error":"failed"}),
            ),
        ]);
        let update = execute(
            &client,
            request(Command::CreatePersonalized {
                display_name: "Ada".into(),
                role_description: None,
                avatar_ref: None,
                instructions: "Be kind".into(),
            }),
        )
        .await;
        let error = update.result.unwrap_err();
        assert!(error.contains("ada-identity") && error.contains("no Bot was created"));
        assert_eq!(server.join().unwrap().len(), 3);
    }
}
