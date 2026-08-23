//! Run-scoped secret capabilities for native Grapheme execution.
//!
//! Credential values stay in zeroizing daemon memory. Before durable enqueue,
//! model-visible grants are replaced with daemon-only aliases; Stasis state
//! carries only a short-lived run token. Host calls expose only opaque handles,
//! signatures, or endpoint-bound HTTP results.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine as _;
use chrono::Utc;
use grapheme_runtime::host::{CapabilityCall, HostCallError};
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use serde_json::{Map, Value, json};
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::agent_secret_request::GraphemeSecretMaterial;

pub const RUN_TOKEN_STATE_FIELD: &str = "__medousa_grapheme_secret_run_v1";
const RUN_TTL_SECS: i64 = 10 * 60;
const MAX_PENDING_RUNS: usize = 128;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REQUEST_HEADERS: usize = 32;
const MAX_AUTHORIZED_HTTP_CALLS: usize = 8;
const GRANT_PREFIX: &[u8] = b"sgrant-";
const GRANT_HEX_LEN: usize = 32;

static RUNS: Lazy<Mutex<HashMap<String, PendingRun>>> = Lazy::new(|| Mutex::new(HashMap::new()));

struct PendingRun {
    #[allow(dead_code)]
    session_id: String,
    expires_at_utc: chrono::DateTime<Utc>,
    secrets: Vec<GraphemeSecretMaterial>,
}

struct ActiveSecret {
    grant_id: String,
    handle: String,
    secret_name: String,
    allowed_hosts: Vec<String>,
    value: Zeroizing<String>,
}

struct ActiveScope {
    secrets: Vec<ActiveSecret>,
    authorized_http_calls: usize,
}

thread_local! {
    static ACTIVE_SCOPE: RefCell<Option<ActiveScope>> = const { RefCell::new(None) };
}

struct ActiveScopeGuard;

impl Drop for ActiveScopeGuard {
    fn drop(&mut self) {
        ACTIVE_SCOPE.with(|slot| {
            slot.replace(None);
        });
    }
}

/// Exchange already session-bound grants for a daemon-only run token. The raw
/// values are moved into this store and are never serialized.
pub fn register_run(
    session_id: String,
    secrets: Vec<GraphemeSecretMaterial>,
) -> Result<String, String> {
    if secrets.is_empty() {
        return Err("a Grapheme secret run requires at least one grant".to_string());
    }
    let now = Utc::now();
    let mut runs = RUNS.lock().expect("Grapheme secret run store");
    runs.retain(|_, run| run.expires_at_utc > now);
    if runs.len() >= MAX_PENDING_RUNS {
        return Err("too many pending Grapheme secret runs".to_string());
    }
    let token = format!("gsrun-{}", Uuid::new_v4().simple());
    runs.insert(
        token.clone(),
        PendingRun {
            session_id,
            expires_at_utc: now + chrono::Duration::seconds(RUN_TTL_SECS),
            secrets,
        },
    );
    Ok(token)
}

pub struct GraphemeRunTokenGuard {
    token: String,
}

impl GraphemeRunTokenGuard {
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for GraphemeRunTokenGuard {
    fn drop(&mut self) {
        cancel_run(&self.token);
    }
}

pub fn register_run_guard(
    session_id: String,
    secrets: Vec<GraphemeSecretMaterial>,
) -> Result<GraphemeRunTokenGuard, String> {
    register_run(session_id, secrets).map(|token| GraphemeRunTokenGuard { token })
}

/// Replace model-visible grants with daemon-only run aliases before the source
/// enters a Stasis job. The active scope resolves those aliases, so neither the
/// original grants nor credential values are needed in durable payloads.
pub fn prepare_run_guard(
    session_id: String,
    source: &str,
    mut secrets: Vec<GraphemeSecretMaterial>,
) -> Result<(GraphemeRunTokenGuard, String), String> {
    let mut prepared_source = source.to_string();
    for secret in &mut secrets {
        if !prepared_source.contains(&secret.grant_id) {
            return Err("Grapheme source does not reference every attached grant".to_string());
        }
        let alias = format!("sref-{}", Uuid::new_v4().simple());
        prepared_source = prepared_source.replace(&secret.grant_id, &alias);
        secret.grant_id = alias;
    }
    if source_contains_secret_grant(&prepared_source) {
        return Err("Grapheme source contains an unattached secret grant".to_string());
    }
    let guard = register_run_guard(session_id, secrets)?;
    Ok((guard, prepared_source))
}

/// Drop an unconsumed run token after enqueue/process failure. Dropping the run
/// also zeroizes every credential it still owns.
pub fn cancel_run(token: &str) {
    RUNS.lock()
        .expect("Grapheme secret run store")
        .remove(token);
}

/// Find concrete opaque grant ids embedded in source. Placeholders such as
/// `sgrant-…` are intentionally ignored; real ids are the prefix plus a UUID's
/// 32 lowercase hexadecimal digits.
pub fn secret_grant_references(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let total_len = GRANT_PREFIX.len() + GRANT_HEX_LEN;
    let mut references = Vec::new();
    let mut index = 0;
    while index + total_len <= bytes.len() {
        if &bytes[index..index + GRANT_PREFIX.len()] != GRANT_PREFIX {
            index += 1;
            continue;
        }
        let end = index + total_len;
        let hex = &bytes[index + GRANT_PREFIX.len()..end];
        let boundary_ok = end == bytes.len() || !bytes[end].is_ascii_hexdigit();
        if hex.iter().all(u8::is_ascii_hexdigit) && boundary_ok {
            let grant = source[index..end].to_string();
            if !references.contains(&grant) {
                references.push(grant);
            }
            index = end;
        } else {
            index += GRANT_PREFIX.len();
        }
    }
    references
}

pub fn source_contains_secret_grant(source: &str) -> bool {
    !secret_grant_references(source).is_empty()
}

pub fn redact_secret_grant_references(text: &str) -> String {
    secret_grant_references(text)
        .into_iter()
        .fold(text.to_string(), |redacted, grant| {
            redacted.replace(&grant, "[REDACTED_GRANT]")
        })
}

/// Remove Medousa's internal run token from user-visible Grapheme state.
pub fn split_execution_state(
    state_current: Option<&Value>,
) -> Result<(Option<String>, Option<Value>), String> {
    let Some(value) = state_current else {
        return Ok((None, None));
    };
    let mut sanitized = value.clone();
    let token = match sanitized.as_object_mut() {
        Some(object) => match object.remove(RUN_TOKEN_STATE_FIELD) {
            Some(Value::String(token)) if token.starts_with("gsrun-") => Some(token),
            Some(_) => return Err("invalid Grapheme secret run token".to_string()),
            None => None,
        },
        None => None,
    };
    Ok((token, Some(sanitized)))
}

/// Install one token's values for the duration of a single blocking Grapheme
/// execution. Capability interception runs on this same thread.
pub fn with_run_scope<T>(token: &str, execute: impl FnOnce() -> T) -> Result<T, String> {
    let run = {
        let now = Utc::now();
        let mut runs = RUNS.lock().expect("Grapheme secret run store");
        runs.retain(|_, run| run.expires_at_utc > now);
        runs.remove(token)
            .ok_or_else(|| "Grapheme secret run token is unknown or expired".to_string())?
    };
    let scope = ActiveScope {
        secrets: run
            .secrets
            .into_iter()
            .map(|material| ActiveSecret {
                grant_id: material.grant_id,
                handle: format!("gsecret-{}", Uuid::new_v4().simple()),
                secret_name: material.secret_name,
                allowed_hosts: material.allowed_hosts,
                value: material.value,
            })
            .collect(),
        authorized_http_calls: 0,
    };
    ACTIVE_SCOPE.with(|slot| {
        if slot.borrow().is_some() {
            return Err("nested Grapheme secret scopes are not supported".to_string());
        }
        slot.replace(Some(scope));
        Ok(())
    })?;
    let _guard = ActiveScopeGuard;
    Ok(execute())
}

pub fn try_secret_capability_call(call: &CapabilityCall) -> Option<Result<Value, HostCallError>> {
    let module = call
        .module
        .as_deref()
        .unwrap_or_else(|| call.capability.split('.').next().unwrap_or_default())
        .to_ascii_lowercase();
    match (module.as_str(), call.op.as_str()) {
        ("secrets", "get_secret_handle") => Some(get_secret_handle(&call.args)),
        ("secrets", "sign_request") => Some(sign_request(&call.args)),
        ("medousa", "authorized_http") => Some(authorized_http(&call.args)),
        _ => None,
    }
}

fn get_secret_handle(args: &Value) -> Result<Value, HostCallError> {
    let grant_id = required_string(args, "name")?;
    with_grant(grant_id, |secret| {
        Ok(json!({
            "handle": secret.handle,
            "name": secret.secret_name,
        }))
    })
}

fn sign_request(args: &Value) -> Result<Value, HostCallError> {
    let reference = required_string(args, "secret")?;
    let payload = args.get("payload").cloned().unwrap_or(Value::Null);
    with_handle(reference, |secret| {
        let bytes = match &payload {
            Value::String(text) => text.as_bytes().to_vec(),
            value => serde_json::to_vec(value)
                .map_err(|error| fatal(format!("serialize signing payload: {error}")))?,
        };
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.value.as_bytes())
            .map_err(|_| fatal("invalid HMAC key"))?;
        mac.update(&bytes);
        let signature =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(json!({
            "ok": true,
            "signature": signature,
            "algorithm": "hmac-sha256",
        }))
    })
}

fn authorized_http(args: &Value) -> Result<Value, HostCallError> {
    let reference = required_string(args, "secret")?;
    ACTIVE_SCOPE.with(|slot| {
        let mut scope = slot.borrow_mut();
        let scope = scope
            .as_mut()
            .ok_or_else(|| fatal("no approved Grapheme secret scope is active"))?;
        if scope.authorized_http_calls >= MAX_AUTHORIZED_HTTP_CALLS {
            return Err(fatal("authorized_http call limit exceeded for this run"));
        }
        let secret_index = scope
            .secrets
            .iter()
            .position(|secret| secret.handle == reference)
            .ok_or_else(|| fatal("secret handle is not approved for this Grapheme run"))?;
        scope.authorized_http_calls += 1;
        execute_authorized_http(&scope.secrets[secret_index], args)
    })
}

fn execute_authorized_http(secret: &ActiveSecret, args: &Value) -> Result<Value, HostCallError> {
    let raw_url = required_string(args, "url")?;
    let url = reqwest::Url::parse(raw_url).map_err(|_| fatal("authorized_http URL is invalid"))?;
    if url.scheme() != "https" {
        return Err(fatal("authorized_http requires HTTPS"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(fatal("authorized_http URL must not contain credentials"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| fatal("authorized_http URL has no host"))?
        .to_ascii_lowercase();
    let effective_port = url
        .port_or_known_default()
        .ok_or_else(|| fatal("authorized_http URL has no effective HTTPS port"))?;
    let requested_authority = if effective_port == 443 {
        host.clone()
    } else {
        format!("{host}:{effective_port}")
    };
    let allowed = authority_is_allowed(&secret.allowed_hosts, &host, effective_port);
    if !allowed {
        return Err(fatal(format!(
            "credential is not approved for HTTPS host {requested_authority}"
        )));
    }

    let method = args
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .trim()
        .to_ascii_uppercase();
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|_| fatal("authorized_http method is invalid"))?;
    if !matches!(
        method,
        reqwest::Method::GET
            | reqwest::Method::POST
            | reqwest::Method::PUT
            | reqwest::Method::PATCH
            | reqwest::Method::DELETE
    ) {
        return Err(fatal("authorized_http method is not allowed"));
    }

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| fatal(format!("build authorized HTTP client: {error}")))?;
    let mut request = client.request(method, url);
    request = apply_public_headers(request, args.get("headers"))?;

    let auth = args
        .get("auth")
        .and_then(Value::as_str)
        .unwrap_or("bearer")
        .trim()
        .to_ascii_lowercase();
    request = match auth.as_str() {
        "bearer" => request.bearer_auth(secret.value.as_str()),
        "header" => {
            let header = args
                .get("header")
                .and_then(Value::as_str)
                .unwrap_or("x-api-key");
            let name = reqwest::header::HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| fatal("authorized_http header name is invalid"))?;
            let prefix = args.get("prefix").and_then(Value::as_str).unwrap_or("");
            let value = reqwest::header::HeaderValue::from_str(&format!(
                "{prefix}{}",
                secret.value.as_str()
            ))
            .map_err(|_| fatal("credential cannot be represented as an HTTP header"))?;
            request.header(name, value)
        }
        _ => return Err(fatal("authorized_http auth must be bearer or header")),
    };

    if let Some(body) = args.get("body") {
        request = request.json(body);
    }
    let response = request.send().map_err(|error| {
        fatal(redact(
            &format!("authorized HTTP request failed: {error}"),
            secret.value.as_str(),
        ))
    })?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| fatal(format!("read authorized HTTP response: {error}")))?;
    let truncated = bytes.len() as u64 > MAX_RESPONSE_BYTES;
    if truncated {
        bytes.truncate(MAX_RESPONSE_BYTES as usize);
    }
    let body = redact(&String::from_utf8_lossy(&bytes), secret.value.as_str());
    Ok(json!({
        "status": status,
        "body": body,
        "truncated": truncated,
        "host": requested_authority,
    }))
}

fn authority_is_allowed(allowed_hosts: &[String], host: &str, port: u16) -> bool {
    let authority = if port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let default_port_authority = format!("{host}:443");
    allowed_hosts
        .iter()
        .any(|allowed| allowed == &authority || (port == 443 && allowed == &default_port_authority))
}

fn apply_public_headers(
    mut request: reqwest::blocking::RequestBuilder,
    headers: Option<&Value>,
) -> Result<reqwest::blocking::RequestBuilder, HostCallError> {
    let Some(headers) = headers else {
        return Ok(request);
    };
    let object = headers
        .as_object()
        .ok_or_else(|| fatal("authorized_http headers must be an object"))?;
    if object.len() > MAX_REQUEST_HEADERS {
        return Err(fatal("authorized_http has too many headers"));
    }
    for (raw_name, raw_value) in object {
        if raw_name.eq_ignore_ascii_case("authorization") {
            return Err(fatal(
                "authorization header is controlled by the credential capability",
            ));
        }
        let value = raw_value
            .as_str()
            .ok_or_else(|| fatal("authorized_http header values must be strings"))?;
        let name = reqwest::header::HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|_| fatal("authorized_http header name is invalid"))?;
        let value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| fatal("authorized_http header value is invalid"))?;
        request = request.header(name, value);
    }
    Ok(request)
}

fn with_grant<T>(
    grant_id: &str,
    use_secret: impl FnOnce(&ActiveSecret) -> Result<T, HostCallError>,
) -> Result<T, HostCallError> {
    ACTIVE_SCOPE.with(|slot| {
        let scope = slot.borrow();
        let scope = scope
            .as_ref()
            .ok_or_else(|| fatal("no approved Grapheme secret scope is active"))?;
        let secret = scope
            .secrets
            .iter()
            .find(|secret| secret.grant_id == grant_id)
            .ok_or_else(|| fatal("secret grant is not approved for this Grapheme run"))?;
        use_secret(secret)
    })
}

fn with_handle<T>(
    handle: &str,
    use_secret: impl FnOnce(&ActiveSecret) -> Result<T, HostCallError>,
) -> Result<T, HostCallError> {
    ACTIVE_SCOPE.with(|slot| {
        let scope = slot.borrow();
        let scope = scope
            .as_ref()
            .ok_or_else(|| fatal("no approved Grapheme secret scope is active"))?;
        let secret = scope
            .secrets
            .iter()
            .find(|secret| secret.handle == handle)
            .ok_or_else(|| fatal("secret handle is not approved for this Grapheme run"))?;
        use_secret(secret)
    })
}

fn required_string<'a>(args: &'a Value, field: &str) -> Result<&'a str, HostCallError> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| fatal(format!("missing {field}")))
}

fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[REDACTED]")
    }
}

fn fatal(message: impl Into<String>) -> HostCallError {
    HostCallError::Fatal(message.into())
}

/// Build a Stasis-compatible JSON payload whose internal token is stripped by
/// the Medousa workflow engine before Grapheme receives its initial state.
pub fn secure_payload_ref(source: &str, run_token: &str) -> String {
    let mut state = Map::new();
    state.insert(
        RUN_TOKEN_STATE_FIELD.to_string(),
        Value::String(run_token.to_string()),
    );
    format!(
        "grapheme:json:{}",
        json!({ "source": source, "state_current": Value::Object(state) })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::ports::outbound::runtime::workflow_engine::WorkflowEngine;

    fn material() -> GraphemeSecretMaterial {
        GraphemeSecretMaterial {
            grant_id: "sgrant-test".to_string(),
            secret_name: "TEST_KEY".to_string(),
            allowed_hosts: vec!["api.example.com".to_string()],
            value: Zeroizing::new("very-secret".to_string()),
        }
    }

    #[test]
    fn run_token_is_removed_before_grapheme_state() {
        let state = json!({ RUN_TOKEN_STATE_FIELD: "gsrun-test", "public": true });
        let (token, sanitized) = split_execution_state(Some(&state)).unwrap();
        assert_eq!(token.as_deref(), Some("gsrun-test"));
        assert_eq!(sanitized.unwrap(), json!({ "public": true }));
    }

    #[test]
    fn concrete_grants_are_detected_without_rejecting_documentation_placeholders() {
        assert_eq!(
            secret_grant_references("get(name: \"sgrant-0123456789abcdef0123456789abcdef\")"),
            vec!["sgrant-0123456789abcdef0123456789abcdef"]
        );
        assert!(!source_contains_secret_grant("get(name: \"sgrant-…\")"));
        assert_eq!(
            redact_secret_grant_references(
                "get(name: \"sgrant-0123456789abcdef0123456789abcdef\")"
            ),
            "get(name: \"[REDACTED_GRANT]\")"
        );
    }

    #[test]
    fn prepared_runs_replace_grants_with_daemon_only_aliases() {
        let grant = "sgrant-0123456789abcdef0123456789abcdef";
        let mut secret = material();
        secret.grant_id = grant.to_string();
        let source = format!(
            "import secrets from \"grapheme/secrets\"\nquery Run {{ secrets.get_secret_handle(name: \"{grant}\") {{ handle name }} }}"
        );
        let (guard, prepared) =
            prepare_run_guard("session-1".to_string(), &source, vec![secret]).unwrap();
        assert!(!prepared.contains(grant));
        assert!(prepared.contains("sref-"));
        let payload = secure_payload_ref(&prepared, guard.token());
        assert!(!payload.contains(grant));
        assert!(!payload.contains("very-secret"));
        let result = with_run_scope(guard.token(), || {
            let alias_start = prepared.find("sref-").unwrap();
            let alias = &prepared[alias_start..alias_start + 37];
            get_secret_handle(&json!({ "name": alias })).unwrap()
        })
        .unwrap();
        assert!(result["handle"].as_str().unwrap().starts_with("gsecret-"));
    }

    #[test]
    fn secret_scope_returns_handles_and_signatures_not_values() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let output = with_run_scope(&token, || {
            let handle = get_secret_handle(&json!({ "name": "sgrant-test" })).unwrap();
            let handle = handle["handle"].as_str().unwrap().to_string();
            let signed = sign_request(&json!({ "secret": handle, "payload": "hello" })).unwrap();
            (handle, signed)
        })
        .unwrap();
        assert!(output.0.starts_with("gsecret-"));
        assert_eq!(output.1["algorithm"], "hmac-sha256");
        assert!(!output.1.to_string().contains("very-secret"));
        assert!(with_run_scope(&token, || ()).is_err());
    }

    #[test]
    fn secret_calls_fail_without_an_active_scope() {
        let error = get_secret_handle(&json!({ "name": "sgrant-test" })).unwrap_err();
        assert!(format!("{error:?}").contains("no approved"));
    }

    #[test]
    fn configured_grapheme_engine_dispatches_secrets_to_the_host_scope() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let source = r#"import secrets from "grapheme/secrets"
query SecretHandle {
  secrets.get_secret_handle(name: "sgrant-test") { handle name }
  |> secrets.sign_request(secret: $current.handle, payload: "hello") { ok signature algorithm }
}"#;
        let engine = crate::grapheme_medousa_bridge::configure_grapheme_engine_builder(
            grapheme_sdk::GraphemeEngine::builder(),
        )
        .build();
        let result = with_run_scope(&token, || engine.execute_source(source))
            .unwrap()
            .expect("Grapheme secret script");
        let current = result.final_state.get("current").unwrap();
        assert_eq!(current.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            current.get("algorithm").and_then(Value::as_str),
            Some("hmac-sha256")
        );
        assert!(current.get("signature").and_then(Value::as_str).is_some());
        assert!(!result.final_state.to_string().contains("very-secret"));
    }

    #[tokio::test]
    async fn workflow_engine_consumes_internal_token_and_hides_secret_state() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let source = r#"import secrets from "grapheme/secrets"
query SecretSignature {
  secrets.get_secret_handle(name: "sgrant-test") { handle name }
  |> secrets.sign_request(secret: $current.handle, payload: "hello") { ok signature algorithm }
}"#;
        let state = json!({
            RUN_TOKEN_STATE_FIELD: token,
            "public": "kept outside the internal field",
        });
        let output = crate::grapheme_medousa_bridge::MedousaWorkflowEngine::new()
            .execute_grapheme_source(source, Some(&state))
            .await
            .expect("workflow execution");
        let serialized = output.final_state.to_string();
        assert!(!serialized.contains(RUN_TOKEN_STATE_FIELD));
        assert!(!serialized.contains("gsrun-"));
        assert!(!serialized.contains("very-secret"));
        assert_eq!(output.final_state["current"]["algorithm"], "hmac-sha256");
    }

    #[test]
    fn authenticated_http_is_denied_before_network_for_unapproved_hosts() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let error = with_run_scope(&token, || {
            let handle = get_secret_handle(&json!({ "name": "sgrant-test" })).unwrap();
            let handle = handle["handle"].as_str().unwrap().to_string();
            authorized_http(&json!({
                "secret": handle,
                "url": "https://evil.example/v1"
            }))
            .unwrap_err()
        })
        .unwrap();
        assert!(format!("{error:?}").contains("not approved"));
    }

    #[test]
    fn grant_ids_cannot_bypass_handle_exchange() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let error = with_run_scope(&token, || {
            sign_request(&json!({ "secret": "sgrant-test", "payload": "hello" })).unwrap_err()
        })
        .unwrap();
        assert!(format!("{error:?}").contains("handle is not approved"));
    }

    #[test]
    fn authorized_http_calls_are_bounded_per_run() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let error = with_run_scope(&token, || {
            let handle = get_secret_handle(&json!({ "name": "sgrant-test" })).unwrap();
            let handle = handle["handle"].as_str().unwrap();
            for _ in 0..MAX_AUTHORIZED_HTTP_CALLS {
                let error = authorized_http(&json!({
                    "secret": handle,
                    "url": "https://evil.example/v1"
                }))
                .unwrap_err();
                assert!(format!("{error:?}").contains("not approved"));
            }
            authorized_http(&json!({
                "secret": handle,
                "url": "https://evil.example/v1"
            }))
            .unwrap_err()
        })
        .unwrap();
        assert!(format!("{error:?}").contains("call limit exceeded"));
    }

    #[test]
    fn https_authorities_are_exact_and_normalize_the_default_port() {
        assert!(authority_is_allowed(
            &["api.example.com".to_string()],
            "api.example.com",
            443
        ));
        assert!(authority_is_allowed(
            &["api.example.com:443".to_string()],
            "api.example.com",
            443
        ));
        assert!(!authority_is_allowed(
            &["api.example.com".to_string()],
            "api.example.com",
            8443
        ));
        assert!(authority_is_allowed(
            &["api.example.com:8443".to_string()],
            "api.example.com",
            8443
        ));
    }

    #[test]
    fn configured_engine_denies_unapproved_authorized_http_before_network() {
        let token = register_run("session-1".to_string(), vec![material()]).unwrap();
        let source = r#"import secrets from "grapheme/secrets"
import medousa from "grapheme/medousa"
query UnauthorizedCall {
  secrets.get_secret_handle(name: "sgrant-test") { handle name }
  |> medousa.authorized_http(secret: $current.handle, url: "https://evil.example/v1") { status body truncated host }
}"#;
        let engine = crate::grapheme_medousa_bridge::configure_grapheme_engine_builder(
            grapheme_sdk::GraphemeEngine::builder(),
        )
        .build();
        let result = with_run_scope(&token, || engine.execute_source(source))
            .unwrap()
            .expect("Grapheme returns a structured fatal outcome");
        assert_eq!(format!("{:?}", result.execution.outcome), "FatalFailure");
        assert!(result.final_state.to_string().contains("not approved"));
    }
}
