//! Runs in a fresh process so no test can touch the user's pairing/mesh store.
use super::*;
use crate::pairing::*;
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};

#[tokio::test]
async fn daemon_pair_refresh_delivery_and_forget_protocol() {
    const CHILD: &str = "MEDOUSA_PEER_PROTOCOL_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().canonicalize().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "peer_connections::protocol_tests::daemon_pair_refresh_delivery_and_forget_protocol", "--nocapture"])
            .env(CHILD, "1")
            .env("MEDOUSA_DATA_DIR", &root)
            .env("MEDOUSA_CONFIG_DIR", root.join("config"))
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let remote = Arc::new(PairingService::new(
        DeviceIdentity::generate_ephemeral(),
        address.clone(),
        "Remote".into(),
        None,
        None,
    ));
    let local = Arc::new(PairingService::new(
        DeviceIdentity::generate_ephemeral(),
        "127.0.0.1:1".into(),
        "Local".into(),
        None,
        None,
    ));
    let router = Router::new()
        .route("/pair/status", get(|State(s): State<Arc<PairingService>>| async move { Json(s.pair_status().await.unwrap()) }))
        .route("/pair/init", post(|State(s): State<Arc<PairingService>>, Json(r): Json<PairInitRequest>| async move { Json(s.pair_init(r, "127.0.0.1".parse().unwrap()).await.unwrap()) }))
        .route("/pair/verify", post(|State(s): State<Arc<PairingService>>, Json(r): Json<PairVerifyRequest>| async move { Json(s.pair_verify(r, "127.0.0.1".parse().unwrap()).await.unwrap()) }))
        .route("/pair/session/challenge", post(|State(s): State<Arc<PairingService>>, Json(r): Json<PairSessionChallengeRequest>| async move { Json(s.pair_session_challenge(r, "127.0.0.1".parse().unwrap()).await.unwrap()) }))
        .route("/pair/session/refresh", post(|State(s): State<Arc<PairingService>>, Json(r): Json<PairSessionRefreshRequest>| async move { Json(s.pair_session_refresh(r, "127.0.0.1".parse().unwrap()).await.unwrap()) }))
        .route("/v1/peer/messages", post(|State(s): State<Arc<PairingService>>, headers: axum::http::HeaderMap, Json(v): Json<Value>| async move {
            let envelope: crate::mesh::MeshEnvelope = serde_json::from_value(v["envelope"].clone()).unwrap();
            let peers = s.pair_status().await.unwrap();
            let peer = peers.paired_devices.iter().find(|p| p.phone_id == envelope.sender_device_id).unwrap();
            let record = crate::pairing::store::PairingStore::new(s.identity().signing_key()).get_by_phone_id(&peer.phone_id).unwrap().unwrap();
            crate::mesh::envelope::verify_envelope(crate::mesh::envelope::VerifyEnvelopeParams {
                envelope: &envelope,
                payload_hash: &crate::mesh::payload_hash_hex(&v["payload"]).unwrap(),
                sender_public_key_b64: &record.phone_public_key,
                expected_sender_device_id: &peer.phone_id,
                expected_recipient_device_id: s.device_id(),
                required_capability: crate::mesh::MeshCapability::Message,
                capability_granted: true,
                now: Utc::now(),
            }).unwrap();
            assert!(headers.get("authorization").unwrap().to_str().unwrap().starts_with("Bearer "));
            assert_eq!(v["payload"]["body"], "hello from daemon");
            Json(json!({"id":"accepted","seq":envelope.seq}))
        }))
        .with_state(remote.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let qr = remote.current_qr_with_options(true).await.unwrap();
    let request = || PairRequest {
        qr_url: Some(qr.url.clone()),
        daemon_url: format!("http://{address}"),
        short_code: None,
        expected_device_id: None,
    };
    // The signed invite must fail closed before trust is persisted.
    let mut forged = request();
    let mut forged_url = reqwest::Url::parse(&qr.url).unwrap();
    let pairs: Vec<(String, String)> = forged_url
        .query_pairs()
        .map(|(k, v)| {
            let value = if k == "t" {
                "forged-token".to_string()
            } else {
                v.into_owned()
            };
            (k.into_owned(), value)
        })
        .collect();
    forged_url.query_pairs_mut().clear().extend_pairs(pairs);
    forged.qr_url = Some(forged_url.to_string());
    assert!(pair(local.clone(), forged).await.is_err());
    assert!(list(local.clone()).await.unwrap().is_empty());
    let summary = pair(local.clone(), request()).await.unwrap();
    assert_eq!(summary.workshop_device_id, remote.device_id());
    assert!(summary.has_session_token);
    let inbound = remote.pair_status().await.unwrap();
    assert_eq!(inbound.paired_devices[0].phone_id, local.device_id());
    assert_eq!(inbound.paired_devices[0].role, "peer");
    // Force expiry of the local projection; refresh must use the real daemon protocol.
    let l = local.clone();
    store_io(move || {
        mutate(&l, |rows| {
            rows[0].session_expires_at = Utc::now() - chrono::Duration::seconds(1);
            Ok(())
        })
    })
    .await
    .unwrap();
    let payload = json!({"body":"hello from daemon"});
    let ack = message(local.clone(), remote.device_id(), payload.clone())
        .await
        .unwrap();
    assert_eq!(ack["id"], "accepted");
    let ack2 = message(local.clone(), remote.device_id(), payload)
        .await
        .unwrap();
    assert!(ack2["seq"].as_u64().unwrap() > ack["seq"].as_u64().unwrap());
    assert!(
        load(local.clone(), remote.device_id().into())
            .await
            .unwrap()
            .session_expires_at
            > Utc::now()
    );
    assert!(
        forget(
            local.clone(),
            remote.device_id().into(),
            "stale-pairing".into()
        )
        .await
        .is_err()
    );
    forget(local.clone(), remote.device_id().into(), summary.pairing_id)
        .await
        .unwrap();
    assert!(list(local).await.unwrap().is_empty());
    server.abort();
}
