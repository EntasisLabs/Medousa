//! Worker access-token renewal using the existing paired-device proof protocol.
use super::*;

// Only an unauthorized request enters this lock. Re-read the stored credential
// after admission so concurrent queries share one rotation instead of revoking
// each other's newly issued generations.
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const SESSION_RESPONSE_LIMIT: usize = 64 * 1024;

impl DaemonWorkerTransport {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn request_authenticated(
        &self,
        stored: &Stored,
        identity: &Identity,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
        response_limit: usize,
    ) -> Result<(u16, Vec<u8>), DelegatedTaskError> {
        let response = worker_request_bounded(
            &self.client,
            &stored.summary.daemon_url,
            stored.summary.iroh_ticket.as_deref(),
            method.clone(),
            path,
            Some(&stored.session_token),
            body,
            response_limit,
        )
        .await
        .map_err(|error| DelegatedTaskError::transport(error.to_string()))?;
        let expired_credential = response.0 == 401
            && serde_json::from_slice::<serde_json::Value>(&response.1)
                .ok()
                .is_some_and(|value| value["code"] == "invalid_credential");
        if !expired_credential {
            return Ok(response);
        }
        let refreshed = self.renew_session(stored, identity).await?;
        // Auth middleware rejected the original request before mesh admission.
        // Retry its exact signed envelope once; never propose or launch new work.
        worker_request_bounded(
            &self.client,
            &refreshed.summary.daemon_url,
            refreshed.summary.iroh_ticket.as_deref(),
            method,
            path,
            Some(&refreshed.session_token),
            body,
            response_limit,
        )
        .await
        .map_err(|error| DelegatedTaskError::transport(error.to_string()))
    }

    async fn renew_session(
        &self,
        stale: &Stored,
        identity: &Identity,
    ) -> Result<Stored, DelegatedTaskError> {
        let _rotation = REFRESH_LOCK.lock().await;
        let (current_identity, connections) = self.all_async().await?;
        if current_identity.device_id != identity.device_id
            || current_identity.key.verifying_key() != identity.key.verifying_key()
        {
            return Err(DelegatedTaskError::conflict(
                "worker source identity changed during renewal",
            ));
        }
        let mut current = connections
            .into_iter()
            .find(|c| same_pairing(c, stale))
            .ok_or_else(|| {
                DelegatedTaskError::conflict(
                    "worker pairing was removed or changed; pair the worker again",
                )
            })?;
        if current.session_token != stale.session_token {
            return Ok(current);
        }
        let token = self.prove_session(&current, &current_identity).await?;
        let root = self.root.clone();
        let client = self.client.clone();
        let expected = current.clone();
        let token_for_store = token.clone();
        crate::peer_completion_delivery::completion_execution_service()
            .run(
                medousa_forge::execution::ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || {
                    let pairing = DaemonWorkerPairing { root, client };
                    Ok(pairing.update_store(|store| {
                        let saved = store
                            .connections
                            .iter_mut()
                            .find(|c| same_pairing(c, &expected))
                            .context("worker pairing was removed or changed during renewal")?;
                        if saved.session_token != expected.session_token {
                            bail!("worker credential changed during renewal");
                        }
                        saved.session_token = token_for_store;
                        Ok(())
                    }))
                },
            )
            .await
            .map_err(|error| DelegatedTaskError::internal(error.to_string()))?
            .map_err(|error| DelegatedTaskError::transport(error.to_string()))?;
        tracing::info!(worker_device_id = %current.summary.workshop_device_id, "renewed paired worker session");
        current.session_token = token;
        Ok(current)
    }

    async fn prove_session(
        &self,
        stored: &Stored,
        identity: &Identity,
    ) -> Result<String, DelegatedTaskError> {
        use crate::pairing::{
            crypto,
            service::{session_refresh_challenge_message, session_refresh_issued_message},
        };
        let challenge: crate::pairing::PairSessionChallengeResponse = self
            .session_json(
                stored,
                "/pair/session/challenge",
                &serde_json::json!({
                    "pairingId": stored.summary.pairing_id, "phoneId": identity.device_id,
                }),
            )
            .await?;
        if challenge.status != "challenge" {
            return Err(refresh_rejected(challenge.reason.as_deref()));
        }
        let session = challenge
            .session_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                DelegatedTaskError::transport("worker renewal omitted challenge session")
            })?;
        let server_nonce = challenge
            .server_nonce
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                DelegatedTaskError::transport("worker renewal omitted challenge nonce")
            })?;
        if uuid::Uuid::parse_str(session).is_err()
            || !crypto::base64url_decode(server_nonce).is_ok_and(|nonce| nonce.len() == 32)
        {
            return Err(DelegatedTaskError::transport(
                "invalid worker renewal challenge",
            ));
        }
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        let nonce = crypto::base64url_encode(&nonce);
        let signed = crypto::sign_message(
            &identity.key,
            &session_refresh_challenge_message(
                session,
                &stored.summary.pairing_id,
                &identity.device_id,
                server_nonce,
                &nonce,
            ),
        );
        let issued: crate::pairing::PairSessionRefreshResponse = self.session_json(stored, "/pair/session/refresh", &serde_json::json!({
            "sessionId": session, "pairingId": stored.summary.pairing_id, "phoneId": identity.device_id,
            "phoneNonce": nonce, "signedNonce": signed,
        })).await?;
        if issued.status != "refreshed" {
            return Err(refresh_rejected(issued.reason.as_deref()));
        }
        let token = issued
            .session_token
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| DelegatedTaskError::transport("worker renewal omitted credential"))?;
        let expiry = issued
            .session_expires_at
            .filter(|expiry| *expiry > Utc::now())
            .ok_or_else(|| DelegatedTaskError::transport("worker renewal omitted valid expiry"))?;
        let signature = issued.server_signed_nonce.ok_or_else(|| {
            DelegatedTaskError::transport("worker renewal omitted destination signature")
        })?;
        let key = crypto::parse_verifying_key(&stored.summary.daemon_public_key)
            .map_err(|_| DelegatedTaskError::conflict("worker destination key is invalid"))?;
        crypto::verify_message(
            &key,
            &session_refresh_issued_message(
                session,
                &stored.summary.pairing_id,
                &nonce,
                &token,
                expiry.timestamp(),
            ),
            &signature,
        )
        .map_err(|_| {
            DelegatedTaskError::conflict(
                "worker renewal destination signature did not match the paired key",
            )
        })?;
        Ok(token)
    }

    async fn session_json<T: DeserializeOwned>(
        &self,
        stored: &Stored,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<T, DelegatedTaskError> {
        let body = serde_json::to_vec(payload)
            .map_err(|_| DelegatedTaskError::internal("encode worker renewal"))?;
        let (status, response) = worker_request_bounded(
            &self.client,
            &stored.summary.daemon_url,
            stored.summary.iroh_ticket.as_deref(),
            Method::POST,
            path,
            None,
            Some(&body),
            SESSION_RESPONSE_LIMIT,
        )
        .await
        .map_err(|error| DelegatedTaskError::transport(error.to_string()))?;
        if !(200..300).contains(&status) {
            // Never include a renewal response body: it can contain credentials.
            return Err(DelegatedTaskError::transport(format!(
                "worker session renewal returned HTTP {status}"
            )));
        }
        serde_json::from_slice(&response)
            .map_err(|_| DelegatedTaskError::transport("invalid worker renewal response"))
    }
}

fn same_pairing(left: &Stored, right: &Stored) -> bool {
    left.summary.id == right.summary.id
        && left.summary.workshop_device_id == right.summary.workshop_device_id
        && left.summary.pairing_id == right.summary.pairing_id
        && left.summary.daemon_public_key == right.summary.daemon_public_key
}

fn refresh_rejected(reason: Option<&str>) -> DelegatedTaskError {
    if matches!(reason, Some("rate_limited" | "busy" | "invalid_session")) {
        DelegatedTaskError::transport("worker session renewal is temporarily unavailable")
    } else {
        DelegatedTaskError::conflict(
            "worker pairing renewal was rejected; pair the worker again if its trust was revoked",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::{
        crypto,
        service::{session_refresh_challenge_message, session_refresh_issued_message},
    };
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use serde_json::{Value, json};
    use std::sync::{Mutex, atomic::AtomicUsize};

    #[derive(Clone)]
    struct Server {
        key: SigningKey,
        client_public: String,
        token: Arc<Mutex<String>>,
        requests: Arc<Mutex<Vec<Value>>>,
        rotations: Arc<AtomicUsize>,
        challenges: Arc<AtomicUsize>,
        reject: bool,
        bad_signature: bool,
        reject_retry: bool,
    }
    async fn challenge(State(state): State<Server>, Json(request): Json<Value>) -> Json<Value> {
        state.challenges.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request["pairingId"], "saved-pairing");
        if state.reject {
            return Json(json!({"status":"rejected","reason":"invalid_pairing"}));
        }
        Json(
            json!({"status":"challenge", "sessionId":"00000000-0000-4000-8000-000000000001", "serverNonce":crypto::base64url_encode(&[7;32])}),
        )
    }
    async fn refresh(State(state): State<Server>, Json(request): Json<Value>) -> Json<Value> {
        let message = session_refresh_challenge_message(
            request["sessionId"].as_str().unwrap(),
            request["pairingId"].as_str().unwrap(),
            request["phoneId"].as_str().unwrap(),
            &crypto::base64url_encode(&[7; 32]),
            request["phoneNonce"].as_str().unwrap(),
        );
        crypto::verify_message(
            &crypto::parse_verifying_key(&state.client_public).unwrap(),
            &message,
            request["signedNonce"].as_str().unwrap(),
        )
        .unwrap();
        state.rotations.fetch_add(1, Ordering::SeqCst);
        let expiry = Utc::now() + chrono::Duration::hours(24);
        let key = if state.bad_signature {
            SigningKey::from_bytes(&[99; 32])
        } else {
            state.key.clone()
        };
        let signed = crypto::sign_message(
            &key,
            &session_refresh_issued_message(
                request["sessionId"].as_str().unwrap(),
                "saved-pairing",
                request["phoneNonce"].as_str().unwrap(),
                "fresh-worker-token",
                expiry.timestamp(),
            ),
        );
        *state.token.lock().unwrap() = "fresh-worker-token".into();
        Json(
            json!({"status":"refreshed","sessionToken":"fresh-worker-token","sessionExpiresAt":expiry,"serverSignedNonce":signed}),
        )
    }
    async fn exchange(
        State(state): State<Server>,
        headers: HeaderMap,
        Json(wire): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        state.requests.lock().unwrap().push(wire.clone());
        if wire["deny"] == true {
            return (StatusCode::FORBIDDEN, Json(json!({"code":"forbidden"})));
        }
        if wire["provider_error"] == true {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"code":"upstream_provider_unauthorized"})),
            );
        }

        let expected = format!("Bearer {}", state.token.lock().unwrap());
        if state.reject_retry
            || headers.get("authorization").and_then(|h| h.to_str().ok()) != Some(expected.as_str())
        {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"code":"invalid_credential"})),
            );
        }
        let payload = json!({"ok":true});
        let envelope = sign_envelope(
            &state.key,
            "remote-worker",
            wire["envelope"]["sender_device_id"]
                .as_str()
                .unwrap_or("unused"),
            1,
            MeshCapability::TaskResult,
            &payload_hash_hex(&payload).unwrap(),
            chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
        );
        (
            StatusCode::OK,
            Json(serde_json::to_value(MeshEnvelopedRequest { envelope, payload }).unwrap()),
        )
    }
    async fn fixture(
        reject: bool,
        bad_signature: bool,
        reject_retry: bool,
    ) -> (
        tempfile::TempDir,
        DaemonWorkerTransport,
        Stored,
        Identity,
        Server,
        tokio::task::JoinHandle<()>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let pairing = DaemonWorkerPairing::new(temp.path().into());
        let identity = pairing.identity().unwrap();
        let state = Server {
            key: SigningKey::from_bytes(&[43; 32]),
            client_public: crypto::base64url_encode(identity.key.verifying_key().as_bytes()),
            token: Arc::new(Mutex::new("destination-token".into())),
            requests: Default::default(),
            rotations: Default::default(),
            challenges: Default::default(),
            reject,
            bad_signature,
            reject_retry,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/pair/session/challenge", post(challenge))
            .route("/pair/session/refresh", post(refresh))
            .route("/v1/query", post(exchange))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let stored = Stored {
            summary: DaemonWorkerConnection {
                id: "worker-remote".into(),
                label: "Worker".into(),
                workshop_device_id: "remote-worker".into(),
                daemon_url: url,
                pairing_id: "saved-pairing".into(),
                daemon_public_key: crypto::base64url_encode(state.key.verifying_key().as_bytes()),
                iroh_ticket: None,
                connected_at: Utc::now().to_rfc3339(),
            },
            session_token: "expired-worker-token".into(),
        };
        pairing
            .save(&Store {
                version: VERSION,
                connections: vec![stored.clone()],
            })
            .unwrap();
        let transport = DaemonWorkerTransport::new(temp.path().into(), reqwest::Client::new());
        (temp, transport, stored, identity, state, server)
    }

    #[tokio::test]
    async fn expired_worker_queries_share_one_verified_rotation_and_persist_for_restart() {
        let (temp, transport, stored, identity, state, server) = fixture(false, false, false).await;
        let body = b"{\"request\":\"same-signed-query\"}";
        let request = || {
            transport.request_authenticated(
                &stored,
                &identity,
                Method::POST,
                "/v1/query",
                Some(body),
                65536,
            )
        };
        let (left, right) = tokio::join!(request(), request());
        assert_eq!(left.unwrap().0, 200);
        assert_eq!(right.unwrap().0, 200);
        assert_eq!(state.rotations.load(Ordering::SeqCst), 1);
        assert_eq!(state.challenges.load(Ordering::SeqCst), 1);
        let pairing = DaemonWorkerPairing::new(temp.path().into());
        let saved = pairing.load().unwrap();
        assert_eq!(saved.connections[0].session_token, "fresh-worker-token");
        assert_eq!(saved.connections[0].summary, stored.summary);
        assert!(
            state
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|request| request == &json!({"request":"same-signed-query"}))
        );
        server.abort();
    }

    #[tokio::test]
    async fn rejected_or_wrong_key_refresh_never_replaces_saved_credentials() {
        for (reject, bad_signature) in [(true, false), (false, true)] {
            let (temp, transport, stored, identity, state, server) =
                fixture(reject, bad_signature, false).await;
            let result = transport
                .request_authenticated(
                    &stored,
                    &identity,
                    Method::POST,
                    "/v1/query",
                    Some(b"{}"),
                    65536,
                )
                .await;
            assert!(result.is_err());
            let error = result.unwrap_err().to_string();
            assert!(!error.contains("expired-worker-token"));
            assert!(!error.contains("fresh-worker-token"));
            assert_eq!(
                DaemonWorkerPairing::new(temp.path().into())
                    .load()
                    .unwrap()
                    .connections[0]
                    .session_token,
                stored.session_token
            );
            assert_eq!(state.requests.lock().unwrap().len(), 1);
            server.abort();
        }
    }

    #[tokio::test]
    async fn permission_denials_and_provider_auth_errors_do_not_rotate_pairing_credentials() {
        let (temp, transport, stored, identity, state, server) = fixture(false, false, false).await;
        let before = fs::read(temp.path().join("workers.json")).unwrap();
        for (body, status) in [
            (br#"{"deny":true}"#.as_slice(), 403),
            (br#"{"provider_error":true}"#.as_slice(), 401),
        ] {
            let response = transport
                .request_authenticated(
                    &stored,
                    &identity,
                    Method::POST,
                    "/v1/query",
                    Some(body),
                    65536,
                )
                .await
                .unwrap();
            assert_eq!(response.0, status);
        }
        assert_eq!(state.challenges.load(Ordering::SeqCst), 0);
        assert_eq!(state.rotations.load(Ordering::SeqCst), 0);
        assert_eq!(fs::read(temp.path().join("workers.json")).unwrap(), before);
        server.abort();
    }

    #[tokio::test]
    async fn renewed_request_retries_only_once_and_removed_pairings_stay_removed() {
        let (temp, transport, stored, identity, state, server) = fixture(false, false, true).await;
        let response = transport
            .request_authenticated(
                &stored,
                &identity,
                Method::POST,
                "/v1/query",
                Some(b"{}"),
                65536,
            )
            .await
            .unwrap();
        assert_eq!(response.0, 401);
        assert_eq!(state.requests.lock().unwrap().len(), 2);
        assert_eq!(state.rotations.load(Ordering::SeqCst), 1);
        DaemonWorkerPairing::new(temp.path().into())
            .remove(&stored.summary.id)
            .unwrap();
        assert!(
            transport
                .request_authenticated(
                    &stored,
                    &identity,
                    Method::POST,
                    "/v1/query",
                    Some(b"{}"),
                    65536
                )
                .await
                .is_err()
        );
        assert!(
            DaemonWorkerPairing::new(temp.path().into())
                .list()
                .unwrap()
                .is_empty()
        );
        assert_eq!(state.rotations.load(Ordering::SeqCst), 1);
        server.abort();
    }
}
