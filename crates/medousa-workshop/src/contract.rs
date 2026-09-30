//! Portal launch contract. These names are the environment variables the
//! Wasmer supervisor injects. The guest does not read `MEDOUSA_WASMER_PACKAGE`
//! or `MEDOUSA_WASMER_ARGS`; those stay on the host command line.

use std::fmt::Write as _;

pub const SANDBOX_ID: &str = "MEDOUSA_SANDBOX_ID";
pub const CONNECT_URL: &str = "MEDOUSA_CONNECT_URL";
pub const KIND: &str = "MEDOUSA_KIND";
pub const VCPU_MILLI: &str = "MEDOUSA_VCPU_MILLI";
pub const MEMORY_MIB: &str = "MEDOUSA_MEMORY_MIB";
pub const SESSION_TOKEN: &str = "MEDOUSA_SESSION_TOKEN";
pub const ONCE: &str = "MEDOUSA_WORKSHOP_ONCE";
pub const REQUIRE_CONNECT: &str = "MEDOUSA_WORKSHOP_REQUIRE_CONNECT";

const WORKSHOP_KIND: &str = "workshop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub sandbox_id: Option<String>,
    pub kind: String,
    pub kind_defaulted: bool,
    pub connect: Option<ConnectTarget>,
    pub vcpu_milli: Option<u32>,
    pub memory_mib: Option<u32>,
    pub session_token: Option<String>,
    pub once: bool,
    pub require_connect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectTarget {
    /// Cleartext daemon origin. The guest calls `GET /health`, then
    /// `GET /pair/heartbeat` when a session token is present.
    Http(HttpOrigin),
    /// Parsed and reported. This guest does not speak TLS.
    Https { origin: String },
    /// `medousa://` invite. Pairing and Iroh stay on the host.
    Invite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpOrigin {
    pub host: String,
    pub port: u16,
    pub display: String,
}

impl HttpOrigin {
    pub fn host_header(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    pub error: &'static str,
    pub detail: String,
}

impl ContractError {
    fn new(error: &'static str, detail: impl Into<String>) -> Self {
        Self {
            error,
            detail: detail.into(),
        }
    }

    pub fn to_line(&self) -> String {
        let mut line = String::from("{\"event\":\"workshop.error\",\"error\":");
        write_json_string(&mut line, self.error);
        line.push_str(",\"detail\":");
        write_json_string(&mut line, &self.detail);
        line.push('}');
        line
    }
}

pub fn parse_launch(get: impl Fn(&str) -> Option<String>) -> Result<Launch, ContractError> {
    let sandbox_id = clean_id(SANDBOX_ID, get(SANDBOX_ID))?;
    let (kind, kind_defaulted) = parse_kind(get(KIND))?;
    let connect = match trimmed(get(CONNECT_URL)) {
        None => None,
        Some(url) => Some(parse_connect_url(&url)?),
    };
    let vcpu_milli = parse_shape(VCPU_MILLI, get(VCPU_MILLI))?;
    let memory_mib = parse_shape(MEMORY_MIB, get(MEMORY_MIB))?;
    let session_token = clean_token(get(SESSION_TOKEN))?;
    let once = parse_flag(ONCE, get(ONCE))?;
    let require_connect = parse_flag(REQUIRE_CONNECT, get(REQUIRE_CONNECT))?;
    Ok(Launch {
        sandbox_id,
        kind,
        kind_defaulted,
        connect,
        vcpu_milli,
        memory_mib,
        session_token,
        once,
        require_connect,
    })
}

fn parse_kind(raw: Option<String>) -> Result<(String, bool), ContractError> {
    match trimmed(raw) {
        None => Ok((WORKSHOP_KIND.to_string(), true)),
        Some(value) if value.eq_ignore_ascii_case(WORKSHOP_KIND) => {
            Ok((WORKSHOP_KIND.to_string(), false))
        }
        Some(_) => Err(ContractError::new(
            "invalid_kind",
            "MEDOUSA_KIND must be workshop",
        )),
    }
}

fn parse_connect_url(raw: &str) -> Result<ConnectTarget, ContractError> {
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("medousa:") {
        return Ok(ConnectTarget::Invite);
    }
    let (https, rest) = if let Some(rest) = strip_prefix_ignore_ascii_case(raw, "https://") {
        (true, rest)
    } else if let Some(rest) = strip_prefix_ignore_ascii_case(raw, "http://") {
        (false, rest)
    } else {
        return Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL must be http://, https://, or medousa://",
        ));
    };
    if rest.is_empty()
        || rest.contains('?')
        || rest.contains('#')
        || rest.contains('@')
        || rest.contains(' ')
    {
        return Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL must be an origin without userinfo, a query, or a fragment",
        ));
    }
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, path),
        None => (rest, ""),
    };
    if !path.is_empty() {
        return Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL must be an origin with no path; the guest calls /health itself",
        ));
    }
    let default_port = if https { 443 } else { 80 };
    let (host, port) = split_host_port(authority, default_port)?;
    let origin = origin_display(if https { "https" } else { "http" }, &host, port);
    if https {
        Ok(ConnectTarget::Https { origin })
    } else {
        Ok(ConnectTarget::Http(HttpOrigin {
            host,
            port,
            display: origin,
        }))
    }
}

fn split_host_port(authority: &str, default_port: u16) -> Result<(String, u16), ContractError> {
    if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(|| {
            ContractError::new(
                "invalid_connect_url",
                "MEDOUSA_CONNECT_URL has an unclosed IPv6 host",
            )
        })?;
        let host = &authority[1..end];
        if host.is_empty() || host.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
            return Err(ContractError::new(
                "invalid_connect_url",
                "MEDOUSA_CONNECT_URL host is empty",
            ));
        }
        let rest = &authority[end + 1..];
        let port = if rest.is_empty() {
            default_port
        } else {
            let port = rest.strip_prefix(':').ok_or_else(|| {
                ContractError::new(
                    "invalid_connect_url",
                    "MEDOUSA_CONNECT_URL IPv6 host must be [addr] or [addr]:port",
                )
            })?;
            parse_port(port)?
        };
        return Ok((host.to_string(), port));
    }
    if authority.is_empty() || authority.contains(' ') {
        return Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL host is empty",
        ));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !port.is_empty() && !host.contains(':') => {
            Ok((host.to_string(), parse_port(port)?))
        }
        Some(_) => Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL host must bracket IPv6 addresses",
        )),
        None => Ok((authority.to_string(), default_port)),
    }
}

fn parse_port(raw: &str) -> Result<u16, ContractError> {
    let port: u16 = raw.parse().map_err(|_| {
        ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL port is not a number",
        )
    })?;
    if port == 0 {
        return Err(ContractError::new(
            "invalid_connect_url",
            "MEDOUSA_CONNECT_URL port must be between 1 and 65535",
        ));
    }
    Ok(port)
}

fn origin_display(scheme: &str, host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("{scheme}://[{host}]:{port}")
    } else {
        format!("{scheme}://{host}:{port}")
    }
}

fn parse_shape(name: &str, raw: Option<String>) -> Result<Option<u32>, ContractError> {
    let Some(value) = trimmed(raw) else {
        return Ok(None);
    };
    let parsed: u32 = value.parse().map_err(|_| {
        ContractError::new(
            "invalid_shape",
            format!("{name} must be a positive integer"),
        )
    })?;
    if parsed == 0 {
        return Err(ContractError::new(
            "invalid_shape",
            format!("{name} must be a positive integer"),
        ));
    }
    Ok(Some(parsed))
}

fn clean_id(name: &str, raw: Option<String>) -> Result<Option<String>, ContractError> {
    let Some(value) = trimmed(raw) else {
        return Ok(None);
    };
    if value.chars().any(char::is_control) || value.len() > 256 {
        return Err(ContractError::new(
            "invalid_sandbox_id",
            format!("{name} must be a single line of at most 256 characters"),
        ));
    }
    Ok(Some(value))
}

fn clean_token(raw: Option<String>) -> Result<Option<String>, ContractError> {
    let Some(value) = trimmed(raw) else {
        return Ok(None);
    };
    if value
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
        || value.len() > 4096
    {
        return Err(ContractError::new(
            "invalid_session_token",
            "MEDOUSA_SESSION_TOKEN must be a single bearer token",
        ));
    }
    Ok(Some(value))
}

fn parse_flag(name: &str, raw: Option<String>) -> Result<bool, ContractError> {
    match trimmed(raw).as_deref() {
        None => Ok(false),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "0" | "false" | "no" | "off" => Ok(false),
            "1" | "true" | "yes" | "on" => Ok(true),
            _ => Err(ContractError::new(
                "invalid_flag",
                format!("{name} must be true or false"),
            )),
        },
    }
}

fn trimmed(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&value[prefix.len()..])
    } else {
        None
    }
}

fn write_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn portal_env_parses_workshop_shape_and_origin() {
        let launch = parse_launch(env(&[
            (SANDBOX_ID, " sbx-1 "),
            (KIND, "Workshop"),
            (CONNECT_URL, "http://10.0.0.8:7419/"),
            (VCPU_MILLI, "1500"),
            (MEMORY_MIB, "512"),
            (SESSION_TOKEN, "session-token"),
            (ONCE, "true"),
        ]))
        .unwrap();
        assert_eq!(launch.sandbox_id.as_deref(), Some("sbx-1"));
        assert_eq!(launch.kind, "workshop");
        assert!(!launch.kind_defaulted);
        assert_eq!(launch.vcpu_milli, Some(1500));
        assert_eq!(launch.memory_mib, Some(512));
        assert_eq!(launch.session_token.as_deref(), Some("session-token"));
        assert!(launch.once);
        match launch.connect {
            Some(ConnectTarget::Http(origin)) => {
                assert_eq!(origin.host, "10.0.0.8");
                assert_eq!(origin.port, 7419);
                assert_eq!(origin.display, "http://10.0.0.8:7419");
                assert_eq!(origin.host_header(), "10.0.0.8:7419");
            }
            other => panic!("expected http origin, got {other:?}"),
        }
    }

    #[test]
    fn ipv6_origin_keeps_brackets() {
        let launch = parse_launch(env(&[(CONNECT_URL, "http://[::1]:7419")])).unwrap();
        match launch.connect {
            Some(ConnectTarget::Http(origin)) => {
                assert_eq!(origin.host, "::1");
                assert_eq!(origin.port, 7419);
                assert_eq!(origin.display, "http://[::1]:7419");
                assert_eq!(origin.host_header(), "[::1]:7419");
            }
            other => panic!("expected http origin, got {other:?}"),
        }
    }

    #[test]
    fn rejects_path_userinfo_and_non_workshop_kind() {
        let path = parse_launch(env(&[(CONNECT_URL, "http://127.0.0.1:7419/v1")]));
        assert_eq!(path.unwrap_err().error, "invalid_connect_url");
        let userinfo = parse_launch(env(&[(CONNECT_URL, "http://user:secret@127.0.0.1:7419")]));
        let err = userinfo.unwrap_err();
        assert_eq!(err.error, "invalid_connect_url");
        assert!(!err.to_line().contains("secret"));
        assert_eq!(
            parse_launch(env(&[(KIND, "worker")])).unwrap_err().error,
            "invalid_kind"
        );
        assert_eq!(
            parse_launch(env(&[(VCPU_MILLI, "0")])).unwrap_err().error,
            "invalid_shape"
        );
        assert_eq!(
            parse_launch(env(&[(SESSION_TOKEN, "two words")]))
                .unwrap_err()
                .error,
            "invalid_session_token"
        );
    }

    #[test]
    fn classifies_https_and_invites_without_echoing_secrets() {
        match parse_launch(env(&[(CONNECT_URL, "https://brain.example")]))
            .unwrap()
            .connect
        {
            Some(ConnectTarget::Https { origin }) => {
                assert_eq!(origin, "https://brain.example:443")
            }
            other => panic!("expected https, got {other:?}"),
        }
        match parse_launch(env(&[(
            CONNECT_URL,
            "medousa://pair/2.0?qrToken=supersecret",
        )]))
        .unwrap()
        .connect
        {
            Some(ConnectTarget::Invite) => {}
            other => panic!("expected invite, got {other:?}"),
        }
    }
}
