//! Provider-neutral sign-in coordinator. Packages own OAuth, the workshop owns
//! pending flow custody and writes completed credentials to its secret service.
use super::*;
use medousa_types::secrets::{IntegrationConnection, IntegrationSecretStatus};

type Flows = BTreeMap<String, (Instant, Arc<Mutex<Pending>>)>;
static FLOWS: LazyLock<Mutex<Flows>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
static FLOW_SLOTS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(16)));
struct Pending {
    _slot: Option<tokio::sync::OwnedSemaphorePermit>,
    provider: InstalledProvider,
    host: String,
    result: ReviewProviderAuthResult,
    expires: Instant,
    next_poll: Instant,
    connection_id: String,
    completed: Option<ReviewProviderAuthResponse>,
}
fn failure(message: &str) -> ReviewProviderError {
    provider_error(ReviewProviderErrorCode::ProviderError, message)
}
pub(super) async fn call(
    execution: &ForgeExecutionService,
    provider: &InstalledProvider,
    host: &str,
    request: ReviewProviderAuthOperation,
) -> Result<ReviewProviderAuthResult, ReviewProviderError> {
    let input = serde_json::to_vec(&ReviewProviderAuthInvocation {
        protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
        host: host.into(),
        request,
    })
    .map_err(|_| failure("Could not prepare provider sign-in."))?;
    let output = package_call(
        execution,
        provider.binary.clone(),
        "--auth",
        input,
        Duration::from_secs(90),
        64 * 1024,
    )
    .await
    .map_err(|_| failure("The review package could not complete sign-in. Try again."))?;
    let response: ReviewProviderAuthPackageResponse = serde_json::from_slice(&output)
        .map_err(|_| failure("The package returned an invalid sign-in response."))?;
    if response.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION {
        return Err(failure("Unsupported provider sign-in protocol."));
    }
    response.result
}
pub(super) async fn save_secret(
    execution: &ForgeExecutionService,
    id: &str,
    value: &str,
) -> Result<(), ReviewProviderError> {
    let id = ConnectionId::parse(id).map_err(|_| failure("Invalid connection identity."))?;
    let value = value.to_string();
    execution.run(ExecutionClass::StoreIo, 64 * 1024, move || {
        let root = crate::paths::medousa_data_dir();
        let installation_id = medousa_secrets::ensure_installation_id(&root).map_err(|e| ForgeError::Store(e.to_string()))?;
        medousa_secrets::save_daemon_secret(&root, &DaemonSecretPath::Integration {installation_id, connection_id:id, slot:IntegrationSecretSlot::OauthBundle}, &value)
            .map_err(|e| ForgeError::Store(e.to_string()))?;
        Ok(())
    }).await.map_err(|_| failure("Could not save the account token in the workshop's credential store. Retry sign-in completion."))
}
fn public_pending(
    id: &str,
    pending: &Pending,
) -> Result<ReviewProviderAuthResponse, ReviewProviderError> {
    match &pending.result {
        ReviewProviderAuthResult::Pending {
            user_code,
            verification_uri,
            interval,
            ..
        } => Ok(ReviewProviderAuthResponse::Pending {
            flow_id: id.into(),
            user_code: user_code.clone(),
            verification_uri: verification_uri.clone(),
            interval: *interval,
            expires_in: pending
                .expires
                .saturating_duration_since(Instant::now())
                .as_secs(),
        }),
        _ => Err(failure("Sign-in completion is being saved. Retry shortly.")),
    }
}
fn validate_pending(
    result: &ReviewProviderAuthResult,
) -> Result<(Duration, Duration), ReviewProviderError> {
    if let ReviewProviderAuthResult::Pending {
        state,
        user_code,
        verification_uri,
        expires_in,
        interval,
    } = result
    {
        let url = reqwest::Url::parse(verification_uri)
            .map_err(|_| failure("Invalid sign-in address."))?;
        if state.len() > 32 * 1024
            || user_code.is_empty()
            || user_code.len() > 128
            || *expires_in == 0
            || *expires_in > 900
            || *interval < 5
            || *interval > 300
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(failure("Invalid sign-in challenge from the package."));
        }
        Ok((
            Duration::from_secs(*expires_in),
            Duration::from_secs(*interval),
        ))
    } else {
        Err(failure("Expected a sign-in challenge from the package."))
    }
}
async fn run(
    execution: &Arc<ForgeExecutionService>,
    request: ReviewProviderAuthRequest,
) -> Result<ReviewProviderAuthResponse, ReviewProviderError> {
    if let ReviewProviderAuthRequest::Begin {
        provider_id,
        host,
        client_id,
    } = request
    {
        validate_query(&ReviewProviderQuery {
            provider_id: provider_id.clone(),
            host: host.clone(),
            connection_id: None,
            request: ReviewProviderOperation::ConnectionStatus,
        })?;
        let provider = installed(execution, &provider_id, "device_auth").await?;
        {
            let mut flows = FLOWS.lock().await;
            flows.retain(|_, (expiry, _)| *expiry > Instant::now());
            if flows.len() >= 128 {
                flows.retain(|_, (_, flow)| {
                    flow.try_lock()
                        .map_or(true, |pending| pending.completed.is_none())
                });
            }
        }
        let slot = FLOW_SLOTS
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("Too many pending sign-ins. Finish or cancel one first."))?;
        let result = call(
            execution,
            &provider,
            &host,
            ReviewProviderAuthOperation::Begin { client_id },
        )
        .await?;
        let (expiry, interval) = validate_pending(&result)?;
        let id = uuid::Uuid::new_v4().to_string();
        let pending = Pending {
            _slot: Some(slot),
            provider,
            host,
            result,
            expires: Instant::now() + expiry,
            next_poll: Instant::now() + interval,
            connection_id: uuid::Uuid::new_v4().to_string(),
            completed: None,
        };
        let response = public_pending(&id, &pending)?;
        FLOWS
            .lock()
            .await
            .insert(id, (pending.expires, Arc::new(Mutex::new(pending))));
        return Ok(response);
    }
    let (id, cancel) = match request {
        ReviewProviderAuthRequest::Poll { flow_id } => (flow_id, false),
        ReviewProviderAuthRequest::Cancel { flow_id } => (flow_id, true),
        _ => unreachable!(),
    };
    let flow = {
        let mut flows = FLOWS.lock().await;
        flows.retain(|_, (expiry, _)| *expiry > Instant::now());
        flows.get(&id).map(|(_, flow)| flow.clone())
    };
    let Some(flow) = flow else {
        if cancel {
            return Ok(ReviewProviderAuthResponse::Cancelled);
        }
        return Err(failure(
            "This sign-in expired or the workshop restarted. Start again.",
        ));
    };
    let mut pending = flow.lock().await;
    if cancel {
        pending.completed = Some(ReviewProviderAuthResponse::Cancelled);
        FLOWS.lock().await.remove(&id);
        return Ok(ReviewProviderAuthResponse::Cancelled);
    }
    if let Some(response) = &pending.completed {
        return Ok(response.clone());
    }
    if let ReviewProviderAuthResult::Pending { state, .. } = &pending.result {
        if Instant::now() < pending.next_poll {
            return public_pending(&id, &pending);
        }
        let result = call(
            execution,
            &pending.provider,
            &pending.host,
            ReviewProviderAuthOperation::Poll {
                state: state.clone(),
            },
        )
        .await?;
        if matches!(result, ReviewProviderAuthResult::Pending { .. }) {
            let (_, interval) = validate_pending(&result)?;
            pending.result = result;
            pending.next_poll = Instant::now() + interval;
            return public_pending(&id, &pending);
        }
        pending.result = result;
    }
    let ReviewProviderAuthResult::Authorized {
        credential,
        account,
    } = &pending.result
    else {
        unreachable!()
    };
    if credential.is_empty()
        || credential.len() > 32 * 1024
        || account.is_empty()
        || account.len() > 256
    {
        return Err(failure("Invalid account credential from the package."));
    }
    // Keep the authorized result pending until both storage operations succeed;
    // a temporary keychain failure never redeems a single-use code a second time.
    save_secret(execution, &pending.connection_id, credential).await?;
    let now = chrono::Utc::now();
    let record = IntegrationConnection {
        connection_id: ConnectionId::parse(&pending.connection_id)
            .map_err(|_| failure("Invalid connection identity."))?,
        kind: pending.provider.descriptor.id.clone(),
        label: Some(account.clone()),
        base_url: Some(format!("https://{}", pending.host)),
        secrets: IntegrationSecretStatus {
            oauth_bundle: true,
            ..Default::default()
        },
        created_at: now,
        updated_at: now,
    };
    crate::integration_connection::integration_connection_service()
        .upsert_record(record)
        .await
        .map_err(|_| {
            failure("Token saved, but connection metadata could not be saved. Retry completion.")
        })?;
    let response = ReviewProviderAuthResponse::Connected {
        connection_id: pending.connection_id.clone(),
        account: account.clone(),
    };
    // A lost HTTP response can retry this flow ID without saving another account.
    // Keep only metadata for that retry; release its active-flow slot and token.
    pending.completed = Some(response.clone());
    pending.result = ReviewProviderAuthResult::Authorized {
        credential: String::new(),
        account: String::new(),
    };
    pending._slot.take();
    Ok(response)
}
pub(super) async fn handle(
    State(state): State<AppState>,
    Json(request): Json<ReviewProviderAuthRequest>,
) -> Result<Json<ReviewProviderAuthResponse>, (StatusCode, String)> {
    run(&state.forge_execution, request)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn early_poll_does_not_invoke_the_package_and_cancellation_discards_the_flow() {
        let id = uuid::Uuid::new_v4().to_string();
        let pending = Pending {
            _slot: None,
            provider: InstalledProvider {
                descriptor: ReviewProviderDescriptor {
                    protocol_version: 1,
                    id: "example".into(),
                    label: "Example".into(),
                    hosts: vec![],
                    operations: vec![],
                },
                binary: "/must-not-be-invoked".into(),
            },
            host: "example.test".into(),
            result: ReviewProviderAuthResult::Pending {
                state: "private-device-secret".into(),
                user_code: "ABCD-EFGH".into(),
                verification_uri: "https://example.test/activate".into(),
                expires_in: 60,
                interval: 5,
            },
            expires: Instant::now() + Duration::from_secs(60),
            next_poll: Instant::now() + Duration::from_secs(5),
            connection_id: uuid::Uuid::new_v4().to_string(),
            completed: None,
        };
        FLOWS
            .lock()
            .await
            .insert(id.clone(), (pending.expires, Arc::new(Mutex::new(pending))));
        let execution = Arc::new(ForgeExecutionService::new());
        let result = run(
            &execution,
            ReviewProviderAuthRequest::Poll {
                flow_id: id.clone(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ReviewProviderAuthResponse::Pending { .. }));
        // An already-saved completion can be retried after a lost HTTP response.
        let flow = FLOWS.lock().await.get(&id).unwrap().1.clone();
        flow.lock().await.completed = Some(ReviewProviderAuthResponse::Connected {
            connection_id: "same-account".into(),
            account: "person".into(),
        });
        for _ in 0..2 {
            let response = run(
                &execution,
                ReviewProviderAuthRequest::Poll {
                    flow_id: id.clone(),
                },
            )
            .await
            .unwrap();
            assert!(
                matches!(response,ReviewProviderAuthResponse::Connected {connection_id,..} if connection_id == "same-account")
            );
        }
        let result = run(
            &execution,
            ReviewProviderAuthRequest::Cancel {
                flow_id: id.clone(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ReviewProviderAuthResponse::Cancelled));
        assert!(
            run(&execution, ReviewProviderAuthRequest::Poll { flow_id: id })
                .await
                .is_err()
        );
    }
    #[test]
    fn http_challenge_contains_no_private_state_or_tokens() {
        let result = ReviewProviderAuthResult::Pending {
            state: "private-device-secret".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://example.com/activate".into(),
            expires_in: 60,
            interval: 5,
        };
        assert!(validate_pending(&result).is_ok());
        let pending = Pending {
            _slot: None,
            provider: InstalledProvider {
                descriptor: ReviewProviderDescriptor {
                    protocol_version: 1,
                    id: "example".into(),
                    label: "Example".into(),
                    hosts: vec![],
                    operations: vec![],
                },
                binary: "unused".into(),
            },
            host: "example.com".into(),
            result,
            expires: Instant::now() + Duration::from_secs(60),
            next_poll: Instant::now(),
            connection_id: "unused".into(),
            completed: None,
        };
        let json = serde_json::to_string(&public_pending("flow", &pending).unwrap()).unwrap();
        assert!(!json.contains("private-device-secret"));
        assert!(!json.contains("credential"));
        assert!(json.contains("ABCD-EFGH"));
    }
}
