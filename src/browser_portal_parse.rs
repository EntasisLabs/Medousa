//! Invite query parsing and SSE frame splitting for the browser portal.
//!
//! These helpers stay free of the wasm host so the pairing message format can
//! be checked on any target.

/// Read one query parameter from a `medousa://` invite.
pub fn query_param(raw: &str, key: &str) -> Option<String> {
    let query = raw.split_once('?')?.1;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        if name == key {
            let decoded = percent_decode(value);
            if decoded.trim().is_empty() {
                return None;
            }
            return Some(decoded);
        }
    }
    None
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Incremental SSE splitter. Completed `data:` payloads are returned; `[DONE]` is dropped.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((end, separator_len)) = find_frame_end(&self.buffer) {
            let frame: Vec<u8> = self.buffer.drain(..end).collect();
            self.buffer.drain(..separator_len);
            if let Some(data) = frame_data(&frame).filter(|data| data != "[DONE]") {
                frames.push(data);
            }
        }
        frames
    }
}

fn find_frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    match (crlf, lf) {
        (Some(crlf), Some(lf)) => Some(if lf.0 < crlf.0 { lf } else { crlf }),
        (Some(separator), None) | (None, Some(separator)) => Some(separator),
        (None, None) => None,
    }
}

fn frame_data(frame: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(frame);
    let mut data = Vec::new();
    for line in text.split(['\n', '\r']) {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.trim_start_matches(' '));
        }
    }
    if data.is_empty() {
        None
    } else {
        Some(data.join("\n"))
    }
}

/// `http://127.0.0.1:7419` when `address` is loopback. Remote invites stay on Iroh.
pub fn loopback_http_origin(address: &str) -> Option<String> {
    let trimmed = address.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = if let Some(v6) = hostport.strip_prefix('[') {
        v6.split(']').next().unwrap_or(v6)
    } else {
        hostport.split(':').next().unwrap_or(hostport)
    };
    match host.trim().to_ascii_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "::1" => Some(url),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_query_decodes_the_iroh_ticket() {
        let raw =
            "medousa://pair/2.0?a=127.0.0.1%3A7419&d=abcd&t=token&s=sig&k=ticket%2Bvalue&n=Home";
        assert_eq!(query_param(raw, "a").as_deref(), Some("127.0.0.1:7419"));
        assert_eq!(query_param(raw, "k").as_deref(), Some("ticket+value"));
        assert_eq!(query_param(raw, "n").as_deref(), Some("Home"));
        assert!(query_param(raw, "missing").is_none());
    }

    #[test]
    fn loopback_origin_accepts_local_advertise_addresses() {
        assert_eq!(
            loopback_http_origin("127.0.0.1:7419").as_deref(),
            Some("http://127.0.0.1:7419")
        );
        assert_eq!(
            loopback_http_origin("http://localhost:7419/").as_deref(),
            Some("http://localhost:7419")
        );
        assert_eq!(
            loopback_http_origin("http://[::1]:7419").as_deref(),
            Some("http://[::1]:7419")
        );
        assert!(loopback_http_origin("http://192.168.1.20:7419").is_none());
    }

    #[test]
    fn sse_parser_rejoins_split_frames() {
        let mut parser = SseParser::default();
        assert!(parser.push(b"event: turn\ndata: {\"seq\":1").is_empty());
        let frames = parser.push(b"}\n\ndata: [DONE]\n\n");
        assert_eq!(frames, vec!["{\"seq\":1}".to_string()]);
    }
}
