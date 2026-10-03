//! Revocable API credentials for agents that can only make HTTP requests.

use super::*;
use base64::Engine;
use medousa_types::{
    CreateExternalAgentTokenRequest, CreateExternalAgentTokenResponse, ExternalAgentAccessStatus,
    ExternalAgentScope,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

const TOKEN_PREFIX: &str = "medousa_agent_";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StoredGrant {
    id: String,
    token_hash: String,
    pub status: ExternalAgentAccessStatus,
}

pub struct AgentAccess {
    credential_id: String,
    owner_id: String,
    scopes: Vec<ExternalAgentScope>,
}

impl AgentAccess {
    /// Match the router's registered pattern, never a user-supplied URL prefix.
    /// Adding a daemon route cannot silently expand an agent token's authority.
    pub fn permits(&self, method: &Method, route: &str) -> bool {
        if self.scopes.contains(&ExternalAgentScope::Read)
            && method == Method::GET
            && matches!(
                route,
                "/v1/capabilities"
                    | "/v1/capabilities/{capability_id}"
                    | "/v1/vault/notes"
                    | "/v1/vault/notes/{*note_path}"
                    | "/v1/vault/search"
                    | "/v1/vault/tags"
                    | "/v1/calendar/events"
            )
        {
            return true;
        }
        if method == Method::POST && route == "/v1/work/query" {
            return self.scopes.contains(&ExternalAgentScope::Read)
                || self.scopes.contains(&ExternalAgentScope::Work);
        }
        self.scopes.contains(&ExternalAgentScope::Work)
            && ((method == Method::POST && matches!(route, "/v1/jobs/ask" | "/v1/work/mutate"))
                || (method == Method::GET
                    && matches!(
                        route,
                        "/v1/jobs/{job_id}/result" | "/v1/jobs/{job_id}/report"
                    )))
    }

    pub fn principal(
        &self,
        transport: crate::request_principal::TransportClass,
    ) -> RequestPrincipal {
        RequestPrincipal::external_agent(
            Arc::from(self.credential_id.as_str()),
            self.owner_id.clone(),
            self.scopes.contains(&ExternalAgentScope::Work),
            transport,
        )
    }
}

impl ExternalConversationStore {
    pub async fn resolve_agent_token(&self, token: &str) -> Option<AgentAccess> {
        if token.len() > 200 {
            return None;
        }
        let (id, _) = token.strip_prefix(TOKEN_PREFIX)?.split_once('.')?;
        let document = self.document.lock().await;
        let record = document.conversations.get(id)?;
        let grant = record.api_access.as_ref()?;
        if grant.status.expires_at <= Utc::now()
            || !constant_time_equal(grant.token_hash.as_bytes(), token_hash(token).as_bytes())
        {
            return None;
        }
        Some(AgentAccess {
            credential_id: format!("external-agent:{}", grant.id),
            owner_id: record.owner_id.clone(),
            scopes: grant.status.scopes.clone(),
        })
    }

    async fn issue_agent_token(
        &self,
        owner_id: &str,
        id: &str,
        input: CreateExternalAgentTokenRequest,
    ) -> Result<CreateExternalAgentTokenResponse, HttpError> {
        if input.scopes.is_empty()
            || input.scopes.len() > 2
            || !(1..=90).contains(&input.expires_in_days)
        {
            return Err(bad_request(
                "select API scopes and an expiry between 1 and 90 days",
            ));
        }
        let mut scopes = input.scopes;
        scopes.dedup();
        let mut current = self.document.lock().await;
        let record = current
            .conversations
            .get(id)
            .filter(|record| record.owner_id == owner_id)
            .ok_or((StatusCode::NOT_FOUND, "agent conversation not found".into()))?;
        let mut random = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut random);
        let token = format!(
            "{TOKEN_PREFIX}{}.{}",
            record.id,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
        );
        let access = ExternalAgentAccessStatus {
            scopes,
            expires_at: Utc::now() + TimeDelta::days(i64::from(input.expires_in_days)),
        };
        let mut next = current.clone();
        next.conversations
            .get_mut(id)
            .expect("record checked")
            .api_access = Some(StoredGrant {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash(&token),
            status: access.clone(),
        });
        self.persist(&next).await.map_err(internal)?;
        *current = next;
        Ok(CreateExternalAgentTokenResponse { token, access })
    }

    async fn revoke_agent_token(
        &self,
        owner_id: &str,
        id: &str,
    ) -> Result<ConversationView, HttpError> {
        let mut current = self.document.lock().await;
        let mut next = current.clone();
        let record = next
            .conversations
            .get_mut(id)
            .filter(|record| record.owner_id == owner_id)
            .ok_or((StatusCode::NOT_FOUND, "agent conversation not found".into()))?;
        record.api_access = None;
        let view = ConversationView::from(&*record);
        self.persist(&next).await.map_err(internal)?;
        *current = next;
        Ok(view)
    }
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub async fn issue(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<CreateExternalAgentTokenRequest>,
) -> Result<
    (
        [(axum::http::header::HeaderName, &'static str); 1],
        Json<CreateExternalAgentTokenResponse>,
    ),
    HttpError,
> {
    let result = state
        .external_conversations
        .issue_agent_token(&owner(&principal, &state), &id, input)
        .await?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(result),
    ))
}

pub async fn revoke(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<ConversationView>, HttpError> {
    state
        .external_conversations
        .revoke_agent_token(&owner(&principal, &state), &id)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request};
    use tower::ServiceExt;

    async fn fixture() -> (tempfile::TempDir, Arc<ExternalConversationStore>, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = ExternalConversationStore::open(dir.path().join("conversations.json"))
            .await
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        store
            .create(
                id.clone(),
                "owner".into(),
                &CreateConversationRequest {
                    provider: Provider::Instinct,
                    label: "Instinct".into(),
                    target: normalize_instinct_phone("+1 (555) 123-4567").unwrap(),
                    webhook_url: None,
                    webhook_key: None,
                    slack_user_token: None,
                    dot_user_id: None,
                },
            )
            .await
            .unwrap();
        (dir, store, id)
    }

    fn request(scopes: Vec<ExternalAgentScope>) -> CreateExternalAgentTokenRequest {
        CreateExternalAgentTokenRequest {
            scopes,
            expires_in_days: 30,
        }
    }

    #[tokio::test]
    async fn tokens_are_hashed_durable_rotatable_revocable_and_owner_scoped() {
        let (dir, store, id) = fixture().await;
        assert!(
            store
                .issue_agent_token("other", &id, request(vec![ExternalAgentScope::Read]))
                .await
                .is_err()
        );
        let first = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Read]))
            .await
            .unwrap();
        let disk = tokio::fs::read_to_string(&store.path).await.unwrap();
        assert!(!disk.contains(&first.token));
        let view = serde_json::to_string(&store.get("owner", &id).await.unwrap()).unwrap();
        assert!(!view.contains("token_hash"));
        assert!(!view.contains(&first.token));
        let reloaded = ExternalConversationStore::open(dir.path().join("conversations.json"))
            .await
            .unwrap();
        assert!(reloaded.resolve_agent_token(&first.token).await.is_some());
        let next = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Work]))
            .await
            .unwrap();
        assert!(store.resolve_agent_token(&first.token).await.is_none());
        assert!(store.resolve_agent_token(&next.token).await.is_some());
        assert!(store.revoke_agent_token("other", &id).await.is_err());
        store.revoke_agent_token("owner", &id).await.unwrap();
        assert!(store.resolve_agent_token(&next.token).await.is_none());
        let last = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Read]))
            .await
            .unwrap();
        store.remove("owner", &id).await.unwrap();
        assert!(store.resolve_agent_token(&last.token).await.is_none());
    }

    #[tokio::test]
    async fn every_provider_has_revocable_work_participation_without_admin_authority() {
        let (_dir, store, _) = fixture().await;
        for (index, provider) in [
            Provider::Muse,
            Provider::Instinct,
            Provider::Dots,
            Provider::GrokBot,
        ]
        .into_iter()
        .enumerate()
        {
            let id = uuid::Uuid::new_v4().to_string();
            store
                .create(
                    id.clone(),
                    "owner".into(),
                    &CreateConversationRequest {
                        provider,
                        label: "Participant".into(),
                        target: format!("target-{index}"),
                        webhook_url: None,
                        webhook_key: None,
                        slack_user_token: None,
                        dot_user_id: None,
                    },
                )
                .await
                .unwrap();
            let grant = store
                .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Work]))
                .await
                .unwrap();
            let access = store.resolve_agent_token(&grant.token).await.unwrap();
            assert!(access.permits(&Method::POST, "/v1/work/query"));
            assert!(access.permits(&Method::POST, "/v1/work/mutate"));
            for route in [
                "/v1/work/mutate/extra",
                "/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/approve",
                "/v1/external-conversations/{id}/messages",
            ] {
                assert!(!access.permits(&Method::POST, route));
            }
            let principal = access.principal(crate::request_principal::TransportClass::Iroh);
            assert_eq!(principal.profile_id(), Some("owner"));
            assert!(!principal.capabilities().contains(Capability::AdminExecute));
            let reopened = ExternalConversationStore::open(store.path.clone())
                .await
                .unwrap();
            assert!(reopened.resolve_agent_token(&grant.token).await.is_some());
            store.revoke_agent_token("owner", &id).await.unwrap();
            assert!(store.resolve_agent_token(&grant.token).await.is_none());
        }
    }

    #[tokio::test]
    async fn expired_or_invalid_grants_fail_closed() {
        let (_dir, store, id) = fixture().await;
        assert!(
            store
                .issue_agent_token("owner", &id, request(vec![]))
                .await
                .is_err()
        );
        assert!(
            store
                .issue_agent_token(
                    "owner",
                    &id,
                    CreateExternalAgentTokenRequest {
                        scopes: vec![ExternalAgentScope::Read],
                        expires_in_days: 91,
                    }
                )
                .await
                .is_err()
        );
        let grant = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Read]))
            .await
            .unwrap();
        assert!(
            store
                .resolve_agent_token(&format!("{}bad", grant.token))
                .await
                .is_none()
        );
        store
            .document
            .lock()
            .await
            .conversations
            .get_mut(&id)
            .unwrap()
            .api_access
            .as_mut()
            .unwrap()
            .status
            .expires_at = Utc::now() - TimeDelta::seconds(1);
        assert!(store.resolve_agent_token(&grant.token).await.is_none());
    }

    #[tokio::test]
    async fn curl_bearer_is_limited_by_scope_and_matched_route_on_every_transport() {
        let (_dir, store, id) = fixture().await;
        let grant = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Read]))
            .await
            .unwrap();
        let mut declared = DeclaredRouter::default();
        async fn admitted(Extension(principal): Extension<RequestPrincipal>) -> &'static str {
            assert_eq!(
                principal.kind(),
                crate::request_principal::PrincipalKind::ExternalAgent
            );
            assert_eq!(principal.profile_id(), Some("owner"));
            "ok"
        }
        for (method, path, capability) in [
            (Method::GET, "/v1/vault/notes", Capability::ContentRead),
            (
                Method::GET,
                "/v1/vault/notes/{*note_path}",
                Capability::ContentRead,
            ),
            (Method::POST, "/v1/jobs/ask", Capability::WorkshopInteract),
            (Method::POST, "/v1/work/query", Capability::ContentRead),
            (Method::POST, "/v1/work/mutate", Capability::ContentWrite),
            (
                Method::GET,
                "/v1/external-conversations",
                Capability::WorkshopRead,
            ),
            (
                Method::GET,
                "/v1/admin/local-credentials",
                Capability::AdminIdentity,
            ),
        ] {
            let handler = if method == Method::GET {
                get(admitted)
            } else {
                post(admitted)
            };
            declared = declared.route(policy(method, path, capability, 1024), handler);
        }
        let app = crate::peer_scope::assemble_daemon_access_boundary_with_declared(
            Router::new(),
            declared,
            DeclaredRouter::default(),
            Router::new(),
            crate::peer_scope::DaemonAccessState::new(None).with_external_agents(store.clone()),
        );
        for (addr, iroh) in [
            ("127.0.0.1:12345", false),
            ("192.0.2.1:12345", false),
            ("127.0.0.1:12345", true),
        ] {
            let app = app
                .clone()
                .layer(Extension(ConnectInfo(addr.parse::<SocketAddr>().unwrap())));
            for (method, path, expected) in [
                (Method::GET, "/v1/vault/notes", StatusCode::OK),
                (Method::GET, "/v1/vault/notes/note.md", StatusCode::OK),
                (Method::POST, "/v1/jobs/ask", StatusCode::FORBIDDEN),
                (Method::POST, "/v1/work/query", StatusCode::OK),
                (Method::POST, "/v1/work/mutate", StatusCode::FORBIDDEN),
                (
                    Method::GET,
                    "/v1/external-conversations",
                    StatusCode::FORBIDDEN,
                ),
                (
                    Method::GET,
                    "/v1/admin/local-credentials",
                    StatusCode::FORBIDDEN,
                ),
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("authorization", format!("Bearer {}", grant.token))
                            .header("x-medousa-transport", if iroh { "iroh" } else { "direct" })
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected, "{addr} {iroh} {path}");
            }
        }
        let app = app.layer(Extension(ConnectInfo(
            "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        )));
        let work = store
            .issue_agent_token("owner", &id, request(vec![ExternalAgentScope::Work]))
            .await
            .unwrap();
        for (method, path, expected) in [
            (Method::POST, "/v1/jobs/ask", StatusCode::OK),
            (Method::POST, "/v1/work/query", StatusCode::OK),
            (Method::POST, "/v1/work/mutate", StatusCode::OK),
            (Method::GET, "/v1/vault/notes", StatusCode::FORBIDDEN),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("authorization", format!("Bearer {}", work.token))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{path}");
        }
        store.revoke_agent_token("owner", &id).await.unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/vault/notes")
                    .header("authorization", format!("Bearer {}", work.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn phone_binding_is_normalized_and_cannot_be_bound_twice() {
        let (_dir, store, id) = fixture().await;
        assert_eq!(
            normalize_instinct_phone("+1 (555) 123-4567").unwrap(),
            "15551234567@s.whatsapp.net"
        );
        for invalid in [
            "5551234567",
            "+0123456789",
            "+123",
            "+123456789@g.us",
            "+1234567890123456",
        ] {
            assert!(normalize_instinct_phone(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            store
                .whatsapp_binding("15551234567@s.whatsapp.net")
                .await
                .unwrap()
                .id,
            id
        );
        assert!(store.whatsapp_binding("15551234567@g.us").await.is_none());
        assert!(
            store
                .create(
                    uuid::Uuid::new_v4().to_string(),
                    "other".into(),
                    &CreateConversationRequest {
                        provider: Provider::Instinct,
                        label: "duplicate".into(),
                        target: "15551234567@s.whatsapp.net".into(),
                        webhook_url: None,
                        webhook_key: None,
                        slack_user_token: None,
                        dot_user_id: None,
                    }
                )
                .await
                .is_err()
        );
    }
}
