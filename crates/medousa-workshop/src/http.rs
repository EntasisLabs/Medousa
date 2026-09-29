//! HTTP/1.1 over a byte stream. The daemon's anonymous liveness body is
//! `{"status":"ok","apiVersion":"v1"}`. Protected `/pair/heartbeat` uses the
//! same `Authorization: Bearer` header paired portals already send.

use std::io::{Read, Write};

const MAX_HTTP_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    pub reason: &'static str,
    pub detail: String,
}

impl HttpError {
    pub(crate) fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        let detail = detail.into().replace(['\n', '\r'], " ");
        let detail = if detail.chars().count() > 180 {
            let mut truncated: String = detail.chars().take(180).collect();
            truncated.push('…');
            truncated
        } else {
            detail
        };
        Self { reason, detail }
    }
}

pub fn encode_request(host_header: &str, path: &str, bearer: Option<&str>) -> String {
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: medousa-workshop/{}\r\nAccept: application/json\r\nConnection: close\r\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(token) = bearer {
        request.push_str("Authorization: Bearer ");
        request.push_str(token);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request
}

pub fn exchange(
    stream: &mut (impl Read + Write),
    host_header: &str,
    path: &str,
    bearer: Option<&str>,
) -> Result<HttpResponse, HttpError> {
    let request = encode_request(host_header, path, bearer);
    stream
        .write_all(request.as_bytes())
        .map_err(|err| HttpError::new("connection_failed", err.to_string()))?;
    stream
        .flush()
        .map_err(|err| HttpError::new("connection_failed", err.to_string()))?;
    read_response(stream)
}

pub fn read_response(stream: &mut impl Read) -> Result<HttpResponse, HttpError> {
    let mut buffered = Vec::new();
    let mut tmp = [0u8; 1024];
    let (header_end, separator_len) = loop {
        if buffered.len() > MAX_HTTP_BYTES {
            return Err(HttpError::new(
                "response_too_large",
                "HTTP headers exceeded 64KiB",
            ));
        }
        let read = stream
            .read(&mut tmp)
            .map_err(|err| HttpError::new("connection_failed", err.to_string()))?;
        if read == 0 {
            return Err(HttpError::new(
                "connection_failed",
                "connection closed before HTTP headers",
            ));
        }
        buffered.extend_from_slice(&tmp[..read]);
        if let Some(split) = header_split(&buffered) {
            break split;
        }
    };
    let header_text = std::str::from_utf8(&buffered[..header_end])
        .map_err(|_| HttpError::new("health_body", "HTTP headers are not UTF-8"))?
        .replace("\r\n", "\n");
    let mut lines = header_text.split('\n');
    let status_line = lines
        .next()
        .ok_or_else(|| HttpError::new("health_body", "HTTP status line is missing"))?;
    let status = parse_status(status_line)?;
    let mut content_length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            let parsed: usize = value
                .parse()
                .map_err(|_| HttpError::new("health_body", "Content-Length is not a number"))?;
            content_length = Some(parsed);
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
        {
            chunked = true;
        }
    }
    if chunked {
        return Err(HttpError::new(
            "chunked_unsupported",
            "chunked HTTP responses are not accepted; the daemon liveness body is a fixed JSON document",
        ));
    }
    let mut body = buffered[header_end + separator_len..].to_vec();
    match content_length {
        Some(length) if length > MAX_HTTP_BYTES => {
            return Err(HttpError::new(
                "response_too_large",
                "HTTP body exceeded 64KiB",
            ));
        }
        Some(length) => {
            while body.len() < length {
                let read = stream
                    .read(&mut tmp)
                    .map_err(|err| HttpError::new("connection_failed", err.to_string()))?;
                if read == 0 {
                    return Err(HttpError::new(
                        "connection_failed",
                        "connection closed before the HTTP body finished",
                    ));
                }
                body.extend_from_slice(&tmp[..read]);
                if body.len() > MAX_HTTP_BYTES {
                    return Err(HttpError::new(
                        "response_too_large",
                        "HTTP body exceeded 64KiB",
                    ));
                }
            }
            body.truncate(length);
        }
        None => loop {
            if body.len() > MAX_HTTP_BYTES {
                return Err(HttpError::new(
                    "response_too_large",
                    "HTTP body exceeded 64KiB",
                ));
            }
            let read = stream
                .read(&mut tmp)
                .map_err(|err| HttpError::new("connection_failed", err.to_string()))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..read]);
        },
    }
    Ok(HttpResponse { status, body })
}

fn header_split(buffered: &[u8]) -> Option<(usize, usize)> {
    buffered
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4))
        .or_else(|| {
            buffered
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| (index, 2))
        })
}

fn parse_status(status_line: &str) -> Result<u16, HttpError> {
    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| HttpError::new("health_body", "HTTP status line is missing"))?;
    if !version.starts_with("HTTP/") {
        return Err(HttpError::new(
            "health_body",
            "response did not start with an HTTP status line",
        ));
    }
    let code = parts
        .next()
        .ok_or_else(|| HttpError::new("health_body", "HTTP status code is missing"))?;
    code.parse::<u16>()
        .map_err(|_| HttpError::new("health_body", "HTTP status code is not a number"))
}

pub fn medousa_liveness(response: &HttpResponse) -> Result<String, HttpError> {
    if response.status != 200 {
        return Err(HttpError::new(
            "health_status",
            format!("GET /health returned HTTP {}", response.status),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body).map_err(|_| {
        HttpError::new(
            "health_body",
            "GET /health did not return the Medousa liveness JSON",
        )
    })?;
    let status = value.get("status").and_then(serde_json::Value::as_str);
    let api_version = value
        .get("apiVersion")
        .and_then(serde_json::Value::as_str)
        .filter(|version| !version.is_empty());
    match (status, api_version) {
        (Some("ok"), Some(version)) => Ok(version.to_string()),
        _ => Err(HttpError::new(
            "health_body",
            "GET /health did not return status ok and apiVersion",
        )),
    }
}

pub fn heartbeat_ok(response: &HttpResponse) -> Result<(), HttpError> {
    if response.status == 401 || response.status == 403 {
        return Err(HttpError::new(
            "heartbeat_unauthorized",
            format!("GET /pair/heartbeat returned HTTP {}", response.status),
        ));
    }
    if response.status != 200 {
        return Err(HttpError::new(
            "heartbeat_failed",
            format!("GET /pair/heartbeat returned HTTP {}", response.status),
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|_| HttpError::new("heartbeat_failed", "heartbeat response was not JSON"))?;
    if value.get("status").and_then(serde_json::Value::as_str) == Some("ok") {
        Ok(())
    } else {
        Err(HttpError::new(
            "heartbeat_failed",
            "heartbeat JSON status was not ok",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn request_omits_bearer_unless_asked() {
        let health = encode_request("127.0.0.1:7419", "/health", None);
        assert!(health.starts_with("GET /health HTTP/1.1\r\n"));
        assert!(health.contains("Host: 127.0.0.1:7419\r\n"));
        assert!(!health.contains("Authorization"));
        let heartbeat = encode_request("127.0.0.1:7419", "/pair/heartbeat", Some("tok"));
        assert!(heartbeat.contains("Authorization: Bearer tok\r\n"));
        assert!(heartbeat.contains("GET /pair/heartbeat "));
    }

    #[test]
    fn reads_fixed_liveness_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 33\r\nConnection: close\r\n\r\n{\"status\":\"ok\",\"apiVersion\":\"v1\"}";
        let response = read_response(&mut Cursor::new(&raw[..])).unwrap();
        assert_eq!(medousa_liveness(&response).unwrap(), "v1");
    }

    #[test]
    fn rejects_chunked_and_non_liveness_bodies() {
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert_eq!(
            read_response(&mut Cursor::new(&chunked[..]))
                .unwrap_err()
                .reason,
            "chunked_unsupported"
        );
        let other = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        let response = read_response(&mut Cursor::new(&other[..])).unwrap();
        assert_eq!(
            medousa_liveness(&response).unwrap_err().reason,
            "health_body"
        );
        let denied = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let response = read_response(&mut Cursor::new(&denied[..])).unwrap();
        assert_eq!(
            heartbeat_ok(&response).unwrap_err().reason,
            "heartbeat_unauthorized"
        );
    }
}
