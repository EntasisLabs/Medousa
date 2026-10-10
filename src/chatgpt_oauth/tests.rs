//! Offline fixtures. The committed RSA key is test-only, never a production credential.
use super::*;
use axum::{
    Json,
    extract::{Form, State},
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
        assert_eq!(form["resource"], RESOURCE);
        assert!(form["client_id"].starts_with("oaiapp_"));
        if form["grant_type"] == "authorization_code" {
            assert_eq!(form["redirect_uri"], "http://127.0.0.1:1455/oauth/callback");
            assert!(form["code_verifier"].len() >= 43);
        }
        let nonce = f.nonce.lock().unwrap().clone();
        let scope = f.scope.lock().unwrap().clone();
        (
            axum::http::StatusCode::OK,
            Json(
                json!({"access_token":format!("access-{}-{}",form["client_id"], f.calls.load(Ordering::SeqCst)), "refresh_token":"rotated-refresh", "id_token":signed(claims(&f.issuer,&form["client_id"],&nonce)), "token_type":"Bearer", "expires_in":3600,"scope":scope}),
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
    async fn revoke(Form(form): Form<HashMap<String, String>>) -> axum::http::StatusCode {
        assert_eq!(form["token_type_hint"], "refresh_token");
        assert_eq!(form["client_id"], "oaiapp_first");
        axum::http::StatusCode::OK
    }
    async fn models(headers: axum::http::HeaderMap) -> Json<Value> {
        assert!(headers.contains_key("authorization"));
        for name in ["originator", "version", "chatgpt-account-id"] {
            assert!(!headers.contains_key(name));
        }
        Json(
            json!({"models":[{"slug":"gpt-6.1-sol","display_name":"GPT-6.1 Sol","visibility":"list","priority":1}, {"slug":"gpt-6-luna","display_name":"GPT-6 Luna","visibility":"list","priority":100}, {"slug":"hidden","visibility":"hide"}, {"slug":"unspecified"}]}),
        )
    }
    let router = axum::Router::new()
        .route("/api/accounts/oauth/token", post(token))
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/revoke", post(revoke))
        .route("/models", get(models))
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
    let (rejected, _) = b.credentials_for_request().await.unwrap();
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
        let (token, client_id) = request.await.unwrap();
        assert_eq!(client_id, "oaiapp_first");
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
fn legacy_codex_credentials_require_new_registration() {
    let store = Arc::new(MemoryStore(StdMutex::new(Some(
        json!({"account_id":"old","access_token":"codex-token"}).to_string(),
    ))));
    let b = ChatGptOAuthBroker::new(store);
    assert!(!b.status().connected);
    assert!(b.status().profiles.is_empty());
}
