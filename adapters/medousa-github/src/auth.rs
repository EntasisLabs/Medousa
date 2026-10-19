//! GitHub device authorization. Device codes and token bundles stay on private IPC.
use super::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
struct DeviceState {
    client_id: String,
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_at: u64,
    interval: u64,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn pending(state: DeviceState) -> Result<ReviewProviderAuthResult, ReviewProviderError> {
    let expires_in = state.expires_at.saturating_sub(now());
    if expires_in == 0 {
        return Err(invalid("GitHub sign-in expired. Start again."));
    }
    Ok(ReviewProviderAuthResult::Pending {
        user_code: state.user_code.clone(),
        verification_uri: state.verification_uri.clone(),
        interval: state.interval,
        expires_in,
        state: serde_json::to_string(&state).map_err(|_| invalid("Invalid sign-in state."))?,
    })
}
// Public Medousa OAuth registration. Device flow uses no client secret or callback.
const MEDOUSA_CLIENT_ID: &str = "Ov23lisduhkFdBIix9ON";
fn client_id(value: Option<String>, host: &str) -> Result<String, ReviewProviderError> {
    let id = value
        .filter(|id| !id.trim().is_empty())
        .or_else(|| std::env::var("MEDOUSA_GITHUB_CLIENT_ID").ok())
        .or_else(|| option_env!("MEDOUSA_GITHUB_CLIENT_ID").map(str::to_owned))
        .or_else(|| (host == "github.com").then(|| MEDOUSA_CLIENT_ID.to_owned()))
        .ok_or_else(|| {
            error(ReviewProviderErrorCode::Unavailable, "This GitHub package is missing Medousa's sign-in registration. Update the GitHub package.")
        })?;
    if id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(invalid(
            "The GitHub package has an invalid sign-in registration.",
        ));
    }
    Ok(id)
}
async fn oauth(
    origin: &str,
    path: &str,
    form: &[(&str, &str)],
) -> Result<Value, ReviewProviderError> {
    let response = http_client()?
        .post(format!("{origin}/{path}"))
        .header("Accept", "application/json")
        .form(form)
        .send()
        .await
        .map_err(|_| {
            error(
                ReviewProviderErrorCode::Unavailable,
                "Could not reach GitHub sign-in. Try again.",
            )
        })?;
    decode_http_limited(response, 64 * 1024).await
}
fn oauth_error(value: &Value) -> Option<ReviewProviderError> {
    let message = match value["error"].as_str()? {
        "access_denied" => "GitHub sign-in was cancelled.",
        "expired_token" | "token_expired" => "GitHub sign-in expired. Start again.",
        "device_flow_disabled" => {
            "GitHub sign-in is unavailable for this package. Update the GitHub package."
        }
        "incorrect_client_credentials" => {
            "GitHub did not recognize Medousa's sign-in registration. Update the GitHub package."
        }
        "bad_refresh_token" => "GitHub sign-in has expired. Connect the account again.",
        _ => "GitHub could not complete sign-in. Start again.",
    };
    Some(error(ReviewProviderErrorCode::NotAuthenticated, message))
}
async fn bundle(
    host: &str,
    api_origin: &str,
    client_id: &str,
    mut token: Value,
    expected: Option<&str>,
) -> Result<ReviewProviderAuthResult, ReviewProviderError> {
    if let Some(error) = oauth_error(&token) {
        return Err(error);
    }
    let access = token["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("GitHub did not return an access token."))?;
    // Validate the actual account; never infer it from an old connection label.
    let user = api_http_base(api_origin, access, "GET", "user", None).await?;
    let account = text(&user, "login");
    let account_id = number(&user, "id").to_string();
    if account.is_empty() || account_id == "0" || expected.is_some_and(|id| id != account_id) {
        return Err(error(
            ReviewProviderErrorCode::NotAuthenticated,
            "The GitHub account identity changed. Connect it again.",
        ));
    }
    token["client_id"] = Value::String(client_id.into());
    token["account_id"] = Value::String(account_id);
    token["account"] = Value::String(account.clone());
    token["host"] = Value::String(host.into());
    if let Some(seconds) = token["expires_in"].as_u64() {
        token["expires_at"] = Value::from(now().saturating_add(seconds));
    }
    Ok(ReviewProviderAuthResult::Authorized {
        credential: token.to_string(),
        account,
    })
}

pub async fn invoke(
    input: ReviewProviderAuthInvocation,
) -> Result<ReviewProviderAuthResult, ReviewProviderError> {
    let web_origin = format!("https://{}", input.host);
    let api_origin = if input.host == "github.com" {
        "https://api.github.com".into()
    } else {
        format!("https://{}/api/v3", input.host)
    };
    invoke_with_endpoints(input, &web_origin, &api_origin).await
}
async fn invoke_with_endpoints(
    input: ReviewProviderAuthInvocation,
    web_origin: &str,
    api_origin: &str,
) -> Result<ReviewProviderAuthResult, ReviewProviderError> {
    if input.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION {
        return Err(invalid("Unsupported sign-in protocol."));
    }
    validate_host(&input.host)?;
    let host = input.host.as_str();
    match input.request {
        ReviewProviderAuthOperation::Begin {
            client_id: requested,
        } => {
            let client_id = client_id(requested, host)?;
            let value = oauth(
                web_origin,
                "login/device/code",
                &[("client_id", &client_id), ("scope", "repo read:user")],
            )
            .await?;
            if let Some(error) = oauth_error(&value) {
                return Err(error);
            }
            let verification_uri = text(&value, "verification_uri");
            let uri = Url::parse(&verification_uri)
                .map_err(|_| invalid("GitHub returned an invalid sign-in address."))?;
            if uri.scheme() != "https"
                || uri.host_str() != Some(host)
                || !uri.username().is_empty()
                || uri.password().is_some()
            {
                return Err(invalid("GitHub returned an unexpected sign-in address."));
            }
            let state = DeviceState {
                client_id,
                device_code: text(&value, "device_code"),
                user_code: text(&value, "user_code"),
                verification_uri,
                interval: number(&value, "interval").clamp(5, 300),
                expires_at: now() + number(&value, "expires_in").min(900),
            };
            if state.device_code.is_empty() || state.user_code.is_empty() {
                return Err(invalid("GitHub returned an incomplete sign-in challenge."));
            }
            pending(state)
        }
        ReviewProviderAuthOperation::Poll { state } => {
            let mut state: DeviceState =
                serde_json::from_str(&state).map_err(|_| invalid("Invalid sign-in state."))?;
            if state.expires_at <= now() {
                return Err(invalid("GitHub sign-in expired. Start again."));
            }
            let value = oauth(
                web_origin,
                "login/oauth/access_token",
                &[
                    ("client_id", &state.client_id),
                    ("device_code", &state.device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
            )
            .await?;
            match value["error"].as_str() {
                Some("authorization_pending") => pending(state),
                Some("slow_down") => {
                    state.interval = number(&value, "interval").max(state.interval + 5).min(300);
                    pending(state)
                }
                _ => bundle(host, api_origin, &state.client_id, value, None).await,
            }
        }
        ReviewProviderAuthOperation::Refresh { credential } => {
            let Ok(saved) = serde_json::from_str::<Value>(&credential) else {
                return Ok(ReviewProviderAuthResult::Authorized {
                    credential,
                    account: String::new(),
                });
            };
            if saved["host"].as_str().is_some_and(|value| value != host) {
                return Err(invalid("This credential belongs to another GitHub host."));
            }
            let expired = saved["expires_at"]
                .as_u64()
                .is_some_and(|expiry| expiry <= now() + 60);
            if !expired {
                return Ok(ReviewProviderAuthResult::Authorized {
                    account: text(&saved, "account"),
                    credential,
                });
            }
            let refresh = saved["refresh_token"]
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    error(
                        ReviewProviderErrorCode::NotAuthenticated,
                        "GitHub sign-in expired. Connect the account again.",
                    )
                })?;
            let id = saved["client_id"]
                .as_str()
                .ok_or_else(|| invalid("Connect the GitHub account again."))?;
            let value = oauth(
                web_origin,
                "login/oauth/access_token",
                &[
                    ("client_id", id),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                ],
            )
            .await?;
            bundle(host, api_origin, id, value, saved["account_id"].as_str()).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_never_echo_upstream_secrets_and_device_state_stays_private() {
        let value =
            serde_json::json!({"error":"access_denied", "error_description":"secret-token"});
        assert!(
            !oauth_error(&value)
                .unwrap()
                .message
                .contains("secret-token")
        );
        let state = DeviceState {
            client_id: "client".into(),
            device_code: "private-device-code".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_at: now() + 60,
            interval: 5,
        };
        let ReviewProviderAuthResult::Pending {
            state, user_code, ..
        } = pending(state).unwrap()
        else {
            panic!()
        };
        assert!(state.contains("private-device-code"));
        assert_eq!(user_code, "ABCD-EFGH");
        assert!(client_id(Some("not a public ID".into()), "github.com").is_err());
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        replies: Vec<(&'static str, &'static str, Value)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            for (path, contains, body) in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0u8; 4096];
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                assert!(request.lines().next().unwrap().contains(path));
                assert!(
                    request.contains(contains),
                    "Expected request marker {contains}"
                );
                assert!(!request.contains("client_secret"));
                let body = body.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        (origin, task)
    }
    fn input(request: ReviewProviderAuthOperation) -> ReviewProviderAuthInvocation {
        ReviewProviderAuthInvocation {
            protocol_version: 1,
            host: "github.com".into(),
            request,
        }
    }
    #[tokio::test]
    async fn device_flow_handles_pending_slowdown_native_token_and_refresh() {
        let (origin, task) = server(vec![
            ("/login/device/code", "client_id=public-client", json!({"device_code":"private-device","user_code":"ABCD-EFGH","verification_uri":"https://github.com/login/device","interval":5,"expires_in":900})),
            ("/login/oauth/access_token", "device_code=private-device", json!({"error":"authorization_pending"})),
            ("/login/oauth/access_token", "device_code=private-device", json!({"error":"slow_down","interval":10})),
            ("/login/oauth/access_token", "device_code=private-device", json!({"access_token":"fixture-token","refresh_token":"fixture-refresh","expires_in":3600})),
            ("/user", "Bearer fixture-token", json!({"id":10,"login":"person"})),
            ("/login/oauth/access_token", "refresh_token=fixture-refresh", json!({"access_token":"rotated-token","refresh_token":"rotated-refresh","expires_in":3600})),
            ("/user", "Bearer rotated-token", json!({"id":10,"login":"person"})),
        ]).await;
        let mut result = invoke_with_endpoints(
            input(ReviewProviderAuthOperation::Begin {
                client_id: Some("public-client".into()),
            }),
            &origin,
            &origin,
        )
        .await
        .unwrap();
        for expected in [5, 10] {
            let ReviewProviderAuthResult::Pending { state, .. } = result else {
                panic!()
            };
            result = invoke_with_endpoints(
                input(ReviewProviderAuthOperation::Poll { state }),
                &origin,
                &origin,
            )
            .await
            .unwrap();
            let ReviewProviderAuthResult::Pending { interval, .. } = &result else {
                panic!()
            };
            assert_eq!(*interval, expected);
        }
        let ReviewProviderAuthResult::Pending { state, .. } = result else {
            panic!()
        };
        let result = invoke_with_endpoints(
            input(ReviewProviderAuthOperation::Poll { state }),
            &origin,
            &origin,
        )
        .await
        .unwrap();
        let ReviewProviderAuthResult::Authorized {
            credential,
            account,
        } = result
        else {
            panic!()
        };
        assert_eq!(account, "person");
        let mut saved: Value = serde_json::from_str(&credential).unwrap();
        assert_eq!(saved["client_id"], "public-client");
        assert_eq!(saved["account_id"], "10");
        saved["expires_at"] = json!(now());
        let result = invoke_with_endpoints(
            input(ReviewProviderAuthOperation::Refresh {
                credential: saved.to_string(),
            }),
            &origin,
            &origin,
        )
        .await
        .unwrap();
        let ReviewProviderAuthResult::Authorized { credential, .. } = result else {
            panic!()
        };
        assert!(credential.contains("rotated-refresh"));
        task.await.unwrap();
    }
    #[tokio::test]
    async fn rejects_account_switch_on_refresh_and_sanitizes_authorization_denial() {
        let (origin, task) = server(vec![
            ("/login/oauth/access_token", "refresh_token=refresh", json!({"access_token":"private-token","refresh_token":"new-refresh","expires_in":3600})),
            ("/user", "Bearer private-token", json!({"id":20,"login":"another-person"})),
        ]).await;
        let result = invoke_with_endpoints(input(ReviewProviderAuthOperation::Refresh {credential:json!({"client_id":"client","host":"github.com","account_id":"10","refresh_token":"refresh","expires_at":1}).to_string()}), &origin, &origin).await;
        let error = result.err().unwrap();
        assert_eq!(error.code, ReviewProviderErrorCode::NotAuthenticated);
        assert!(!error.message.contains("private-token"));
        task.await.unwrap();
        let (origin, task) = server(vec![(
            "/login/oauth/access_token",
            "device_code=private-device",
            json!({"error":"access_denied","error_description":"private-device"}),
        )])
        .await;
        let state = DeviceState {
            client_id: "client".into(),
            device_code: "private-device".into(),
            user_code: "ABCD".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_at: now() + 60,
            interval: 5,
        };
        let result = invoke_with_endpoints(
            input(ReviewProviderAuthOperation::Poll {
                state: serde_json::to_string(&state).unwrap(),
            }),
            &origin,
            &origin,
        )
        .await;
        assert!(!result.err().unwrap().message.contains("private-device"));
        task.await.unwrap();
    }
}
