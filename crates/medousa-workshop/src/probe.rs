//! Outbound attach to a Medousa daemon. Iroh is not dialed from here.

use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Serialize;

use crate::contract::{ConnectTarget, HttpOrigin, Launch};
use crate::http::{self, HttpError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub exit_code: u8,
    pub line: String,
    pub reside: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadyLine<'a> {
    event: &'a str,
    package: &'a str,
    version: &'a str,
    sandbox_id: Option<&'a str>,
    kind: &'a str,
    #[serde(skip_serializing_if = "is_false")]
    kind_defaulted: bool,
    shape: Shape,
    connect: ConnectJson<'a>,
    mesh: Mesh<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Shape {
    vcpu_milli: Option<u32>,
    memory_mib: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectJson<'a> {
    mode: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heartbeat: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Mesh<'a> {
    in_guest: bool,
    owner: &'a str,
}

#[derive(Debug)]
struct Attach {
    mode: &'static str,
    ok: Option<bool>,
    url: Option<String>,
    http_status: Option<u16>,
    api_version: Option<String>,
    heartbeat: Option<&'static str>,
    reason: Option<&'static str>,
    detail: Option<String>,
}

pub fn execute(launch: &Launch) -> Execution {
    if launch.require_connect && launch.connect.is_none() {
        return Execution {
            exit_code: 2,
            line: crate::contract::ContractError {
                error: "connect_url_required",
                detail: "MEDOUSA_WORKSHOP_REQUIRE_CONNECT is set and MEDOUSA_CONNECT_URL is empty"
                    .to_string(),
            }
            .to_line(),
            reside: false,
        };
    }
    let attach = attach(launch);
    let failed_requirement = launch.require_connect && attach.ok != Some(true);
    let line = ready_line(launch, &attach);
    Execution {
        exit_code: if failed_requirement { 1 } else { 0 },
        reside: !failed_requirement && !launch.once,
        line,
    }
}

fn attach(launch: &Launch) -> Attach {
    match &launch.connect {
        None => Attach {
            mode: "absent",
            ok: None,
            url: None,
            http_status: None,
            api_version: None,
            heartbeat: None,
            reason: None,
            detail: None,
        },
        Some(ConnectTarget::Https { origin }) => Attach {
            mode: "https_unsupported",
            ok: Some(false),
            url: Some(origin.clone()),
            http_status: None,
            api_version: None,
            heartbeat: None,
            reason: Some("https_unsupported"),
            detail: Some(
                "This guest speaks cleartext HTTP/1.1 to a daemon origin. TLS and Iroh stay on the host."
                    .to_string(),
            ),
        },
        Some(ConnectTarget::Invite) => Attach {
            mode: "invite_host_side",
            ok: Some(false),
            url: None,
            http_status: None,
            api_version: None,
            heartbeat: None,
            reason: Some("iroh_host_side"),
            detail: Some(
                "medousa:// invites finish on the host daemon or Home over Iroh. The Wasmer guest does not dial medousa-http/1."
                    .to_string(),
            ),
        },
        Some(ConnectTarget::Http(origin)) => probe_http(origin, launch.session_token.as_deref()),
    }
}

fn probe_http(origin: &HttpOrigin, token: Option<&str>) -> Attach {
    let health = match exchange(origin, "/health", None) {
        Ok(response) => response,
        Err(error) => {
            return failed(origin, None, None, error);
        }
    };
    let api_version = match http::medousa_liveness(&health) {
        Ok(version) => version,
        Err(error) => return failed(origin, Some(health.status), None, error),
    };
    let Some(token) = token else {
        return Attach {
            mode: "http_health",
            ok: Some(true),
            url: Some(origin.display.clone()),
            http_status: Some(health.status),
            api_version: Some(api_version),
            heartbeat: Some("skipped"),
            reason: None,
            detail: None,
        };
    };
    // `/health` stays anonymous. An invalid bearer on that route is rejected
    // instead of falling back to public access.
    let heartbeat = match exchange(origin, "/pair/heartbeat", Some(token)) {
        Ok(response) => response,
        Err(error) => return failed(origin, None, Some(api_version), error),
    };
    match http::heartbeat_ok(&heartbeat) {
        Ok(()) => Attach {
            mode: "http_health",
            ok: Some(true),
            url: Some(origin.display.clone()),
            http_status: Some(health.status),
            api_version: Some(api_version),
            heartbeat: Some("ok"),
            reason: None,
            detail: None,
        },
        Err(error) => Attach {
            mode: "http_health",
            ok: Some(false),
            url: Some(origin.display.clone()),
            http_status: Some(heartbeat.status),
            api_version: Some(api_version),
            heartbeat: Some(if error.reason == "heartbeat_unauthorized" {
                "unauthorized"
            } else {
                "failed"
            }),
            reason: Some(error.reason),
            detail: Some(error.detail),
        },
    }
}

fn failed(
    origin: &HttpOrigin,
    http_status: Option<u16>,
    api_version: Option<String>,
    error: HttpError,
) -> Attach {
    Attach {
        mode: "http_health",
        ok: Some(false),
        url: Some(origin.display.clone()),
        http_status,
        api_version,
        heartbeat: None,
        reason: Some(error.reason),
        detail: Some(error.detail),
    }
}

fn exchange(
    origin: &HttpOrigin,
    path: &str,
    bearer: Option<&str>,
) -> Result<http::HttpResponse, HttpError> {
    let mut stream = dial(origin)?;
    http::exchange(&mut stream, &origin.host_header(), path, bearer)
}

fn dial(origin: &HttpOrigin) -> Result<TcpStream, HttpError> {
    let addrs = (origin.host.as_str(), origin.port)
        .to_socket_addrs()
        .map_err(HttpError::from_io)?;
    let mut last = None;
    let mut saw_addr = false;
    for addr in addrs {
        saw_addr = true;
        match connect_one(&addr) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                return Ok(stream);
            }
            Err(err) => last = Some(err),
        }
    }
    Err(last.map(HttpError::from_io).unwrap_or_else(|| HttpError {
        reason: "connection_failed",
        detail: if saw_addr {
            "connection failed".to_string()
        } else {
            "DNS returned no addresses".to_string()
        },
    }))
}

fn connect_one(addr: &SocketAddr) -> std::io::Result<TcpStream> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        TcpStream::connect_timeout(addr, Duration::from_secs(5))
    }
    #[cfg(target_arch = "wasm32")]
    {
        TcpStream::connect(*addr)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl HttpError {
    fn from_io(err: std::io::Error) -> Self {
        let reason = match err.kind() {
            ErrorKind::TimedOut | ErrorKind::WouldBlock => "timeout",
            _ => "connection_failed",
        };
        Self::new(reason, err.to_string())
    }
}

fn ready_line(launch: &Launch, attach: &Attach) -> String {
    let ready = ReadyLine {
        event: "workshop.ready",
        package: "medousa-workshop",
        version: env!("CARGO_PKG_VERSION"),
        sandbox_id: launch.sandbox_id.as_deref(),
        kind: &launch.kind,
        kind_defaulted: launch.kind_defaulted,
        shape: Shape {
            vcpu_milli: launch.vcpu_milli,
            memory_mib: launch.memory_mib,
        },
        connect: ConnectJson {
            mode: attach.mode,
            ok: attach.ok,
            url: attach.url.as_deref(),
            http_status: attach.http_status,
            api_version: attach.api_version.as_deref(),
            heartbeat: attach.heartbeat,
            reason: attach.reason,
            detail: attach.detail.as_deref(),
        },
        mesh: Mesh {
            in_guest: false,
            owner: "host_daemon",
        },
    };
    serde_json::to_string(&ready).unwrap_or_else(|_| {
        "{\"event\":\"workshop.error\",\"error\":\"report_failed\",\"detail\":\"ready line could not be encoded\"}"
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        CONNECT_URL, KIND, ONCE, REQUIRE_CONNECT, SANDBOX_ID, SESSION_TOKEN, VCPU_MILLI,
        parse_launch,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        }
    }

    fn json_line(pairs: &[(&str, &str)]) -> serde_json::Value {
        let execution = execute(&parse_launch(env(pairs)).unwrap());
        serde_json::from_str(&execution.line).unwrap()
    }

    #[test]
    fn absent_connect_stays_resident_until_once() {
        let launch = parse_launch(env(&[
            (SANDBOX_ID, "sbx"),
            (KIND, "workshop"),
            (VCPU_MILLI, "1000"),
        ]))
        .unwrap();
        let resident = execute(&launch);
        assert_eq!(resident.exit_code, 0);
        assert!(resident.reside);
        let once = execute(&parse_launch(env(&[(SANDBOX_ID, "sbx"), (ONCE, "1")])).unwrap());
        assert!(!once.reside);
        let value: serde_json::Value = serde_json::from_str(&resident.line).unwrap();
        assert_eq!(value["event"], "workshop.ready");
        assert_eq!(value["sandboxId"], "sbx");
        assert_eq!(value["kind"], "workshop");
        assert_eq!(value["shape"]["vcpuMilli"], 1000);
        assert!(value["shape"]["memoryMib"].is_null());
        assert_eq!(value["connect"]["mode"], "absent");
        assert!(value["connect"].get("ok").is_none());
        assert_eq!(value["mesh"]["inGuest"], false);
        assert_eq!(value["mesh"]["owner"], "host_daemon");
    }

    #[test]
    fn invite_and_https_do_not_echo_secrets_or_claim_success() {
        let invite = json_line(&[(
            CONNECT_URL,
            "medousa://pair/2.0?qrToken=supersecret&ticket=abc",
        )]);
        assert_eq!(invite["connect"]["mode"], "invite_host_side");
        assert_eq!(invite["connect"]["ok"], false);
        assert_eq!(invite["connect"]["reason"], "iroh_host_side");
        assert!(!invite.to_string().contains("supersecret"));
        let https = json_line(&[(CONNECT_URL, "https://brain.example:8443")]);
        assert_eq!(https["connect"]["mode"], "https_unsupported");
        assert_eq!(https["connect"]["url"], "https://brain.example:8443");
        let required = execute(
            &parse_launch(env(&[
                (CONNECT_URL, "medousa://pair/2.0?qrToken=supersecret"),
                (REQUIRE_CONNECT, "1"),
                (ONCE, "1"),
            ]))
            .unwrap(),
        );
        assert_eq!(required.exit_code, 1);
        assert!(!required.reside);
        assert!(!required.line.contains("supersecret"));
    }

    #[test]
    fn probes_real_health_and_heartbeat_paths() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut tmp = [0u8; 512];
                loop {
                    let read = socket.read(&mut tmp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&tmp[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let body = if text.starts_with("GET /health ") {
                    assert!(!text.contains("Authorization"));
                    br#"{"status":"ok","apiVersion":"v1","workerId":"hidden"}"#.to_vec()
                } else if text.starts_with("GET /pair/heartbeat ") {
                    assert!(text.contains("Authorization: Bearer session-token\r\n"));
                    br#"{"status":"ok"}"#.to_vec()
                } else {
                    b"nope".to_vec()
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(header.as_bytes()).unwrap();
                socket.write_all(&body).unwrap();
            }
        });
        let execution = execute(
            &parse_launch(env(&[
                (CONNECT_URL, &format!("http://{}", address)),
                (SESSION_TOKEN, "session-token"),
                (ONCE, "1"),
            ]))
            .unwrap(),
        );
        assert_eq!(execution.exit_code, 0);
        let value: serde_json::Value = serde_json::from_str(&execution.line).unwrap();
        assert_eq!(value["connect"]["ok"], true);
        assert_eq!(value["connect"]["mode"], "http_health");
        assert_eq!(value["connect"]["apiVersion"], "v1");
        assert_eq!(value["connect"]["heartbeat"], "ok");
        assert_eq!(value["connect"]["httpStatus"], 200);
        assert!(!execution.line.contains("session-token"));
        assert!(!execution.line.contains("hidden"));
    }

    #[test]
    fn unauthorized_heartbeat_is_not_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for expected in ["/health", "/pair/heartbeat"] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut tmp = [0u8; 512];
                loop {
                    let read = socket.read(&mut tmp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&tmp[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&request);
                assert!(text.starts_with(&format!("GET {expected} ")));
                let (status, body) = if expected == "/health" {
                    (200, r#"{"status":"ok","apiVersion":"v1"}"#)
                } else {
                    (401, r#"{"status":"unauthorized"}"#)
                };
                let bytes = body.as_bytes();
                let header = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                socket.write_all(header.as_bytes()).unwrap();
                socket.write_all(bytes).unwrap();
            }
        });
        let execution = execute(
            &parse_launch(env(&[
                (CONNECT_URL, &format!("http://{address}")),
                (SESSION_TOKEN, "nope"),
                (REQUIRE_CONNECT, "yes"),
            ]))
            .unwrap(),
        );
        assert_eq!(execution.exit_code, 1);
        assert!(!execution.reside);
        let value: serde_json::Value = serde_json::from_str(&execution.line).unwrap();
        assert_eq!(value["connect"]["heartbeat"], "unauthorized");
        assert_eq!(value["connect"]["reason"], "heartbeat_unauthorized");
        assert_eq!(value["connect"]["apiVersion"], "v1");
    }

    #[test]
    fn refused_port_is_a_ready_failure_not_a_contract_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let execution = execute(
            &parse_launch(env(&[(CONNECT_URL, &format!("http://127.0.0.1:{port}"))])).unwrap(),
        );
        assert_eq!(execution.exit_code, 0);
        assert!(execution.reside);
        let value: serde_json::Value = serde_json::from_str(&execution.line).unwrap();
        assert_eq!(value["event"], "workshop.ready");
        assert_eq!(value["connect"]["ok"], false);
        assert_eq!(value["connect"]["reason"], "connection_failed");
    }
}
