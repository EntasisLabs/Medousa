//! Offline fixtures. The committed RSA key is test-only, never a production credential.
use super::*;
use axum::{
    Json,
    extract::{Form, OriginalUri, State},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Default)]
struct MemoryStore(StdMutex<Option<String>>);
impl ChatGptCredentialStore for MemoryStore {
    fn load_bundle(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save_bundle(&self, value: Option<&str>) -> Result<(), String> {
        *self.0.lock().unwrap() = value.map(str::to_owned);
        Ok(())
    }
}
#[derive(Clone)]
struct Fixture {
    issuer: String,
    nonce: Arc<StdMutex<String>>,
    scope: Arc<StdMutex<String>>,
    refresh_error: Arc<StdMutex<Option<String>>>,
    calls: Arc<AtomicUsize>,
}
fn signed(claims: Value) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("medousa-test-only".into());
    jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_rsa_pem(include_bytes!(
            "../../tests/fixtures/siwc-test-key.txt"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn claims(issuer: &str, audience: &str, nonce: &str) -> Value {
    json!({"iss":issuer, "aud":audience, "sub":"subject", "nonce":nonce, "email":"user@example.com", "exp":Utc::now().timestamp()+3600, "iat":Utc::now().timestamp()})
}
async fn setup() -> (Arc<ChatGptOAuthBroker>, Arc<MemoryStore>, Fixture) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let f = Fixture {
        issuer: format!("http://{}", listener.local_addr().unwrap()),
        nonce: Default::default(),
        scope: Arc::new(StdMutex::new(SCOPES.into())),
        refresh_error: Default::default(),
        calls: Default::default(),
    };
    async fn token(
        State(f): State<Fixture>,
        OriginalUri(uri): OriginalUri,
        Form(form): Form<HashMap<String, String>>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        f.calls.fetch_add(1, Ordering::SeqCst);
        if form["grant_type"] == "refresh_token"
            && let Some(error) = f.refresh_error.lock().unwrap().clone()
        {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({"error":error})),
            );
        }
        let legacy = form["client_id"] == LEGACY_CLIENT_ID;
        if legacy {
            assert_eq!(uri.path(), "/oauth/token");
            assert!(!form.contains_key("resource"));
        } else {
            assert_eq!(uri.path(), "/api/accounts/oauth/token");
            assert_eq!(form["resource"], RESOURCE);
            assert!(form["client_id"].starts_with("oaiapp_"));
        }
        if form["grant_type"] == "authorization_code" {
            assert_eq!(
                form["redirect_uri"],
                if legacy {
                    "http://127.0.0.1:1455/auth/callback"
                } else {
                    "http://127.0.0.1:1455/oauth/callback"
                }
            );
            assert!(form["code_verifier"].len() >= 43);
        }
        let nonce = f.nonce.lock().unwrap().clone();
        let scope = f.scope.lock().unwrap().clone();
        let mut identity = claims(&f.issuer, &form["client_id"], &nonce);
        if legacy {
            identity["https://api.openai.com/auth"] = json!({"chatgpt_account_id": "acct_legacy"});
        }
        (
            axum::http::StatusCode::OK,
            Json(
                json!({"access_token":format!("access-{}-{}",form["client_id"], f.calls.load(Ordering::SeqCst)), "refresh_token":"rotated-refresh", "id_token":signed(identity), "token_type":"Bearer", "expires_in":3600,"scope":scope}),
            ),
        )
    }
    async fn discovery(State(f): State<Fixture>) -> Json<Value> {
        Json(
            json!({"issuer":f.issuer,"jwks_uri":format!("{}/jwks",f.issuer),"revocation_endpoint":format!("{}/revoke", f.issuer)}),
        )
    }
    async fn jwks() -> Json<Value> {
        Json(
            serde_json::from_str(include_str!("../../tests/fixtures/siwc-test-jwks.json")).unwrap(),
        )
    }
    async fn revoke(
        OriginalUri(uri): OriginalUri,
        Form(form): Form<HashMap<String, String>>,
    ) -> axum::http::StatusCode {
        assert_eq!(form["token_type_hint"], "refresh_token");
        assert_eq!(
            form["client_id"],
            if uri.path() == "/oauth/revoke" {
                LEGACY_CLIENT_ID
            } else {
                "oaiapp_first"
            }
        );
        axum::http::StatusCode::OK
    }
    async fn models(OriginalUri(uri): OriginalUri, headers: axum::http::HeaderMap) -> Json<Value> {
        assert!(headers.contains_key("authorization"));
        if uri.path() == "/legacy-models" {
            assert_eq!(headers["chatgpt-account-id"], "acct_legacy");
            assert_eq!(
                headers["originator"],
                crate::openai_codex_chat_client::CODEX_COMPAT_ORIGINATOR
            );
            assert_eq!(
                headers["version"],
                crate::openai_codex_chat_client::CODEX_COMPAT_VERSION
            );
            assert_eq!(
                headers["user-agent"],
                crate::openai_codex_chat_client::codex_compat_user_agent()
            );
            assert_eq!(
                uri.query().unwrap(),
                format!(
                    "client_version={}",
                    crate::openai_codex_chat_client::CODEX_COMPAT_VERSION
                )
            );
        } else {
            for name in ["originator", "version", "chatgpt-account-id"] {
                assert!(!headers.contains_key(name));
            }
        }
        Json(
            json!({"models":[{"slug":"gpt-6.1-sol","display_name":"GPT-6.1 Sol","visibility":"list","priority":1}, {"slug":"gpt-6-luna","display_name":"GPT-6 Luna","visibility":"list","priority":100}, {"slug":"hidden","visibility":"hide"}, {"slug":"unspecified"}]}),
        )
    }
    let router = axum::Router::new()
        .route("/api/accounts/oauth/token", post(token))
        .route("/oauth/token", post(token))
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/revoke", post(revoke))
        .route("/oauth/revoke", post(revoke))
        .route("/models", get(models))
        .route("/legacy-models", get(models))
        .with_state(f.clone());
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let store = Arc::new(MemoryStore::default());
    let broker = Arc::new(ChatGptOAuthBroker::with_config(
        OAuthConfig {
            issuer: f.issuer.clone(),
        },
        store.clone(),
    ));
    (broker, store, f)
}
fn begin_request(client_id: Option<&str>) -> BeginChatGptOAuthRequest {
    BeginChatGptOAuthRequest {
        redirect_uri: "http://127.0.0.1:1455/oauth/callback".into(),
        client_id: client_id.map(str::to_owned),
        enable_plan_usage: false,
        auth_method: Some(ChatGptAuthMethod::Registered),
    }
}
fn callback(begin: &BeginChatGptOAuthResponse, id: &str) -> CompleteChatGptOAuthRequest {
    let params: HashMap<_, _> = reqwest::Url::parse(&begin.authorization_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    let mut url = reqwest::Url::parse(&params["redirect_uri"]).unwrap();
    url.query_pairs_mut().extend_pairs([
        ("state", params["state"].as_str()),
        ("code", "test-code"),
        ("client_id", id),
    ]);
    CompleteChatGptOAuthRequest {
        login_id: begin.login_id.clone(),
        callback_url: url.into(),
    }
}
async fn connect(b: &ChatGptOAuthBroker, f: &Fixture, id: &str) -> CompleteChatGptOAuthResponse {
    let begin = b.begin(begin_request(None)).await.unwrap();
    *f.nonce.lock().unwrap() = reqwest::Url::parse(&begin.authorization_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "nonce")
        .unwrap()
        .1
        .into_owned();
    b.complete(callback(&begin, id)).await.unwrap()
}

async fn connect_legacy(b: &ChatGptOAuthBroker, f: &Fixture) -> CompleteChatGptOAuthResponse {
    let begin = b
        .begin(BeginChatGptOAuthRequest {
            redirect_uri: "http://127.0.0.1:1455/auth/callback".into(),
            client_id: None,
            enable_plan_usage: false,
            auth_method: Some(ChatGptAuthMethod::CodexLegacy),
        })
        .await
        .unwrap();
    let params: HashMap<_, _> = reqwest::Url::parse(&begin.authorization_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(params["client_id"], LEGACY_CLIENT_ID);
    assert_eq!(params["scope"], LEGACY_SCOPES);
    assert!(begin.authorization_url.contains("/oauth/authorize?"));
    for field in [
        "resource",
        "ext_agent_host_id",
        "agent_name_hint",
        "id_token_hint",
    ] {
        assert!(!params.contains_key(field), "{field}");
    }
    *f.nonce.lock().unwrap() = params["nonce"].clone();
    // The legacy callback does not issue a dynamic client ID.
    let mut request = callback(&begin, LEGACY_CLIENT_ID);
    request.callback_url = request
        .callback_url
        .split("&client_id=")
        .next()
        .unwrap()
        .into();
    b.complete(request).await.unwrap()
}

#[tokio::test]
async fn legacy_and_registered_connections_remain_independent() {
    let (b, store, f) = setup().await;
    let registered = connect(&b, &f, "oaiapp_first").await.connection;
    *f.scope.lock().unwrap() = LEGACY_SCOPES.into();
    let legacy = connect_legacy(&b, &f).await;
    assert!(legacy.first_connection);
    assert_eq!(
        legacy.connection.auth_method,
        Some(ChatGptAuthMethod::CodexLegacy)
    );
    assert_eq!(legacy.connection.account_id.as_deref(), Some("acct_legacy"));
    assert!(legacy.connection.plan_usage_enabled);
    let legacy_id = legacy.connection.client_id.unwrap();
    let (token, account, method) = b.credentials_for_request().await.unwrap();
    assert!(token.contains(LEGACY_CLIENT_ID));
    assert_eq!(account, "acct_legacy");
    assert_eq!(method, ChatGptAuthMethod::CodexLegacy);
    let models = b
        .list_models_from_url(&format!("{}/legacy-models", f.issuer))
        .await
        .unwrap();
    assert_eq!(models.models, ["gpt-6-luna", "gpt-6.1-sol", "unspecified"]);
    let refreshed = b.refresh_after_unauthorized(&token).await.unwrap();
    assert_eq!(refreshed.1, "acct_legacy");
    assert_eq!(refreshed.2, ChatGptAuthMethod::CodexLegacy);
    assert_ne!(token, refreshed.0);
    b.select(registered.client_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(b.status().auth_method, Some(ChatGptAuthMethod::Registered));
    assert_eq!(b.credentials_for_request().await.unwrap().1, "oaiapp_first");
    assert!(matches!(
        b.refresh_after_unauthorized(&refreshed.0).await,
        Err(OAuthError::ReauthenticationRequired)
    ));
    b.list_models_from_url(&format!("{}/models", f.issuer))
        .await
        .unwrap();
    b.select(&legacy_id).await.unwrap();
    assert!(b.disconnect().await.unwrap().revoked);
    assert!(!b.status().connected);
    assert!(b.select("oaiapp_first").await.unwrap().connected);
    let restored = ChatGptOAuthBroker::with_config(OAuthConfig { issuer: f.issuer }, store);
    assert_eq!(restored.status(), b.status());
    assert_eq!(restored.status().profiles.len(), 2);
}

#[tokio::test]
async fn saved_connections_cannot_change_methods_or_publish_another_accounts_catalog() {
    let (b, _, f) = setup().await;
    connect(&b, &f, "oaiapp_first").await;
    let public_credentials = b.credentials_for_request().await.unwrap();
    assert!(
        b.begin(BeginChatGptOAuthRequest {
            auth_method: Some(ChatGptAuthMethod::CodexLegacy),
            ..begin_request(Some("oaiapp_first"))
        })
        .await
        .is_err()
    );
    connect_legacy(&b, &f).await;
    assert!(matches!(
        b.list_models_with_credentials(&format!("{}/models", f.issuer), public_credentials)
            .await,
        Err(OAuthError::AccountIdentityMissing)
    ));
    for redirect in [
        "http://127.0.0.1:5000/auth/callback",
        "http://127.0.0.1:1455/oauth/callback",
        "http://localhost:1455/auth/callback",
    ] {
        assert!(
            b.begin(BeginChatGptOAuthRequest {
                redirect_uri: redirect.into(),
                client_id: None,
                auth_method: Some(ChatGptAuthMethod::CodexLegacy),
                enable_plan_usage: false,
            })
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn existing_registered_bundles_keep_their_original_method() {
    let (b, store, f) = setup().await;
    connect(&b, &f, "oaiapp_first").await;
    let mut old: Value = serde_json::from_str(&store.load_bundle().unwrap().unwrap()).unwrap();
    old["profiles"][0]
        .as_object_mut()
        .unwrap()
        .remove("auth_method");
    old["profiles"][0]
        .as_object_mut()
        .unwrap()
        .remove("account_id");
    store.save_bundle(Some(&old.to_string())).unwrap();
    let restored = ChatGptOAuthBroker::with_config(OAuthConfig { issuer: f.issuer }, store);
    assert_eq!(
        restored.credentials_for_request().await.unwrap().2,
        ChatGptAuthMethod::Registered
    );
    let begin = restored
        .begin(BeginChatGptOAuthRequest {
            auth_method: None,
            ..begin_request(None)
        })
        .await
        .unwrap();
    assert!(begin.authorization_url.contains("/api/accounts/authorize?"));
}
#[tokio::test]
async fn registration_identity_catalog_and_returning_sign_in() {
    let (b, store, f) = setup().await;
    let result = connect(&b, &f, "oaiapp_first").await;
    assert!(result.first_connection);
    assert_eq!(result.connection.account_id.as_deref(), Some("subject"));
    assert!(result.connection.plan_usage_enabled);
    let data: Connections = serde_json::from_str(&store.load_bundle().unwrap().unwrap()).unwrap();
    let host = data.ext_agent_host_id;
    assert!(host.starts_with("urn:uuid:"));
    let begin = b.begin(begin_request(Some("oaiapp_first"))).await.unwrap();
    let params: HashMap<_, _> = reqwest::Url::parse(&begin.authorization_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(params["client_id"], "oaiapp_first");
    assert_eq!(params["ext_agent_host_id"], host);
    assert!(!params.contains_key("agent_name_hint"));
    assert!(params.contains_key("id_token_hint"));
    let models = b
        .list_models_from_url(&format!("{}/models", f.issuer))
        .await
        .unwrap();
    assert_eq!(models.models, ["gpt-6.1-sol", "gpt-6-luna"]);
    assert_eq!(models.display_names["gpt-6.1-sol"], "GPT-6.1 Sol");
    assert_eq!(
        ChatGptOAuthBroker::with_config(OAuthConfig { issuer: f.issuer }, store).status(),
        b.status()
    );
}
#[tokio::test]
async fn invalid_or_replayed_callback_never_exchanges_a_code() {
    let (b, _, f) = setup().await;
    let begin = b.begin(begin_request(None)).await.unwrap();
    let mut r = callback(&begin, "oaiapp_first");
    r.callback_url += "&state=duplicate";
    assert!(b.complete(r.clone()).await.is_err());
    assert!(matches!(
        b.complete(r).await,
        Err(OAuthError::LoginNotFound)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    for uri in [
        "http://localhost:1455/oauth/callback",
        "https://127.0.0.1:1455/oauth/callback",
        "http://127.0.0.1:1455/wrong",
        "http://127.0.0.1:1455/oauth/callback?x=1",
    ] {
        assert!(
            b.begin(BeginChatGptOAuthRequest {
                redirect_uri: uri.into(),
                ..begin_request(None)
            })
            .await
            .is_err()
        );
    }
}
#[test]
fn rejects_invalid_signature_issuer_audience_expiry_and_nonce() {
    let jwks =
        serde_json::from_str(include_str!("../../tests/fixtures/siwc-test-jwks.json")).unwrap();
    let valid = claims(DEFAULT_ISSUER, "oaiapp_first", "nonce");
    assert!(
        validate_identity(
            &signed(valid.clone()),
            "oaiapp_first",
            DEFAULT_ISSUER,
            Some("nonce"),
            &jwks
        )
        .is_ok()
    );
    for (key, value) in [
        ("iss", json!("https://evil.example")),
        ("aud", json!("oaiapp_other")),
        ("exp", json!(1)),
        ("nonce", json!("wrong")),
        ("sub", json!("")),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(
            validate_identity(
                &signed(invalid),
                "oaiapp_first",
                DEFAULT_ISSUER,
                Some("nonce"),
                &jwks
            )
            .is_err()
        );
    }
    let symmetric = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &valid,
        &jsonwebtoken::EncodingKey::from_secret(b"not-a-public-key"),
    )
    .unwrap();
    assert!(
        validate_identity(
            &symmetric,
            "oaiapp_first",
            DEFAULT_ISSUER,
            Some("nonce"),
            &jwks
        )
        .is_err()
    );
    let token = signed(valid);
    let mut fields: Vec<_> = token.split('.').map(str::to_owned).collect();
    fields[2].replace_range(0..1, "!");
    assert!(
        validate_identity(
            &fields.join("."),
            "oaiapp_first",
            DEFAULT_ISSUER,
            Some("nonce"),
            &jwks
        )
        .is_err()
    );
}
#[tokio::test]
async fn identity_only_grant_blocks_inference_and_explicitly_enables_consent() {
    let (b, _, f) = setup().await;
    *f.scope.lock().unwrap() = "openid email profile".into();
    let result = connect(&b, &f, "oaiapp_first").await;
    assert_eq!(result.status, "plan_usage_disabled");
    assert!(!result.first_connection);
    assert!(matches!(
        b.credentials_for_request().await,
        Err(OAuthError::PlanUsageDisabled)
    ));
    let begin = b
        .begin(BeginChatGptOAuthRequest {
            enable_plan_usage: true,
            ..begin_request(Some("oaiapp_first"))
        })
        .await
        .unwrap();
    let p: HashMap<_, _> = reqwest::Url::parse(&begin.authorization_url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(p["prompt"], "consent");
    assert!(p["scope"].contains(DIRECT_SCOPE));
}
#[tokio::test]
async fn profiles_rotation_and_logout_keep_registration_and_host() {
    let (b, store, f) = setup().await;
    connect(&b, &f, "oaiapp_first").await;
    connect(&b, &f, "oaiapp_second").await;
    assert_eq!(b.status().profiles.len(), 2);
    assert!(
        b.status()
            .profiles
            .iter()
            .all(|p| p.email.as_deref() == Some("user@example.com"))
    );
    b.select("oaiapp_first").await.unwrap();
    b.cached.write().unwrap().profiles[0]
        .credentials
        .as_mut()
        .unwrap()
        .expires_at_utc = Utc::now();
    let before = f.calls.load(Ordering::SeqCst);
    let mut requests = Vec::new();
    for _ in 0..8 {
        let b = b.clone();
        requests.push(tokio::spawn(async move {
            b.credentials_for_request().await.unwrap()
        }));
    }
    let mut tokens = Vec::new();
    for r in requests {
        tokens.push(r.await.unwrap().0);
    }
    assert!(tokens.iter().all(|t| t == &tokens[0]));
    assert_eq!(f.calls.load(Ordering::SeqCst), before + 1);
    let saved: Connections = serde_json::from_str(&store.load_bundle().unwrap().unwrap()).unwrap();
    let host = saved.ext_agent_host_id;
    assert!(b.disconnect().await.unwrap().revoked);
    let saved: Connections = serde_json::from_str(&store.load_bundle().unwrap().unwrap()).unwrap();
    assert_eq!(saved.ext_agent_host_id, host);
    assert_eq!(saved.profiles[0].client_id, "oaiapp_first");
    assert!(saved.profiles[0].credentials.is_none());
    assert!(saved.profiles[1].credentials.is_some());
    assert!(
        !b.begin(begin_request(Some("oaiapp_first")))
            .await
            .unwrap()
            .authorization_url
            .contains("id_token_hint")
    );
}
#[tokio::test]
async fn concurrent_unauthorized_requests_rotate_once_and_never_switch_accounts() {
    let (b, _, f) = setup().await;
    connect(&b, &f, "oaiapp_first").await;
    let (rejected, _, _) = b.credentials_for_request().await.unwrap();
    let before = f.calls.load(Ordering::SeqCst);
    let mut requests = Vec::new();
    for _ in 0..8 {
        let b = b.clone();
        let rejected = rejected.clone();
        requests.push(tokio::spawn(async move {
            b.refresh_after_unauthorized(&rejected).await.unwrap()
        }));
    }
    let mut tokens = Vec::new();
    for request in requests {
        let (token, client_id, method) = request.await.unwrap();
        assert_eq!(client_id, "oaiapp_first");
        assert_eq!(method, ChatGptAuthMethod::Registered);
        tokens.push(token);
    }
    assert!(tokens.iter().all(|token| token == &tokens[0]));
    assert_ne!(tokens[0], rejected);
    assert_eq!(f.calls.load(Ordering::SeqCst), before + 1);
    connect(&b, &f, "oaiapp_second").await;
    let before = f.calls.load(Ordering::SeqCst);
    assert!(matches!(
        b.refresh_after_unauthorized(&tokens[0]).await,
        Err(OAuthError::ReauthenticationRequired)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
    assert_eq!(b.status().client_id.as_deref(), Some("oaiapp_second"));
}
#[tokio::test]
async fn refresh_errors_distinguish_temporary_failure_from_unusable_tokens() {
    let (b, _, f) = setup().await;
    connect(&b, &f, "oaiapp_first").await;
    for code in ["temporarily_unavailable", "invalid_client"] {
        *f.refresh_error.lock().unwrap() = Some(code.into());
        assert!(b.refresh().await.is_err());
        assert!(b.status().connected);
    }
    *f.refresh_error.lock().unwrap() = Some("invalid_grant".into());
    assert!(matches!(
        b.refresh().await,
        Err(OAuthError::ReauthenticationRequired)
    ));
    assert_eq!(b.status().status, "reauth_required");
    assert_eq!(b.status().client_id.as_deref(), Some("oaiapp_first"));
}
#[test]
fn incomplete_legacy_credentials_are_not_used() {
    let store = Arc::new(MemoryStore(StdMutex::new(Some(
        json!({"account_id":"old","access_token":"codex-token"}).to_string(),
    ))));
    let b = ChatGptOAuthBroker::new(store);
    assert!(!b.status().connected);
    assert!(b.status().profiles.is_empty());
}

#[tokio::test]
async fn complete_legacy_credentials_are_imported_without_becoming_public_tokens() {
    let access_token = signed(json!({"exp": Utc::now().timestamp() + 3600}));
    let id_token = signed(json!({"sub": "old-subject", "email": "old@example.com",
        "https://api.openai.com/auth": {"chatgpt_account_id": "old-account"}}));
    let raw = json!({"account_id": "old-account", "access_token": access_token,
        "refresh_token": "old-refresh", "id_token": id_token,
        "expires_at_utc": Utc::now() + ChronoDuration::hours(1)})
    .to_string();
    let store = Arc::new(MemoryStore(StdMutex::new(Some(raw))));
    let b = ChatGptOAuthBroker::new(store);
    assert_eq!(b.status().auth_method, Some(ChatGptAuthMethod::CodexLegacy));
    let credentials = b.credentials_for_request().await.unwrap();
    assert_eq!(credentials.0, access_token);
    assert_eq!(credentials.1, "old-account");
    assert_eq!(credentials.2, ChatGptAuthMethod::CodexLegacy);
    let refreshed = credentials_from_tokens(
        TokenResponse {
            access_token: credentials.0,
            refresh_token: None,
            id_token: None,
            expires_in: None,
            scope: None,
            token_type: None,
            earliest_refresh_at: None,
        },
        b.cached
            .read()
            .unwrap()
            .active()
            .unwrap()
            .credentials
            .as_ref(),
        ChatGptAuthMethod::CodexLegacy,
    )
    .unwrap();
    assert_eq!(refreshed.refresh_token, "old-refresh");
    assert!(refreshed.expires_at_utc > Utc::now());
}

#[test]
fn registered_oauth_still_requires_token_type_and_expiry_metadata() {
    for missing in ["token_type", "expires_in"] {
        let mut response = json!({"access_token":"access", "refresh_token":"refresh",
            "id_token":"identity", "expires_in":3600, "token_type":"Bearer", "scope":SCOPES});
        response.as_object_mut().unwrap().remove(missing);
        assert!(matches!(
            credentials_from_tokens(
                serde_json::from_value(response).unwrap(),
                None,
                ChatGptAuthMethod::Registered
            ),
            Err(OAuthError::InvalidAuthorizationResponse)
        ));
    }
}
