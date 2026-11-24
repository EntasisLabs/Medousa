//! Catches an OAuth redirect on a loopback port, so a sign-in can finish in
//! the system browser while the engine keeps the tokens. The renderer-free
//! core of medousa-home's `oauth_browser`.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CALLBACK_PATH: &str = "/oauth/callback";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_REQUEST_BYTES: usize = 16 * 1024;

/// A listener waiting for one redirect.
pub struct Session {
    listener: TcpListener,
    redirect_uri: String,
}

/// The redirect as it arrived, with the browser still waiting on a page.
pub struct Callback {
    stream: TcpStream,
    url: String,
}

impl Session {
    pub async fn bind() -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| format!("start OAuth callback listener: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("read OAuth callback address: {error}"))?
            .port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Waits up to ten minutes for the browser to come back.
    pub async fn wait(self) -> Result<Callback, String> {
        let redirect_uri = self.redirect_uri;
        let listener = self.listener;
        let wait = async move {
            loop {
                let (mut stream, _) = listener
                    .accept()
                    .await
                    .map_err(|error| format!("accept OAuth callback: {error}"))?;
                match read_request(&mut stream).await? {
                    Some(target) => {
                        let suffix = target.strip_prefix(CALLBACK_PATH).unwrap_or_default();
                        return Ok(Callback {
                            stream,
                            url: format!("{redirect_uri}{suffix}"),
                        });
                    }
                    None => respond(&mut stream, false).await,
                }
            }
        };
        tokio::time::timeout(CALLBACK_TIMEOUT, wait)
            .await
            .map_err(|_| "Sign-in timed out".to_string())?
    }
}

impl Callback {
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Tells the browser how it went.
    pub async fn finish(mut self, success: bool) {
        respond(&mut self.stream, success).await;
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<Option<String>, String> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("read OAuth callback: {error}"))?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if request.len() >= MAX_REQUEST_BYTES {
            return Err("OAuth callback request was too large".to_string());
        }
    }
    Ok(target(&request))
}

/// The request target of a GET to the callback path.
fn target(request: &[u8]) -> Option<String> {
    let request = std::str::from_utf8(request).ok()?;
    let mut parts = request.lines().next()?.split_whitespace();
    let (method, target) = (parts.next()?, parts.next()?);
    (method == "GET"
        && (target == CALLBACK_PATH || target.starts_with(&format!("{CALLBACK_PATH}?"))))
    .then(|| target.to_string())
}

async fn respond(stream: &mut TcpStream, success: bool) {
    let (status, heading, detail) = if success {
        (
            "200 OK",
            "Connected",
            "You can close this page and return to Medousa.",
        )
    } else {
        (
            "400 Bad Request",
            "Could not connect",
            "Return to Medousa and try signing in again.",
        )
    };
    let body = format!(
        "<!doctype html><meta name=\"viewport\" content=\"width=device-width\"><title>{heading}</title><style>body{{font:16px system-ui;background:#111;color:#eee;display:grid;place-content:center;min-height:90vh;margin:0}}main{{max-width:28rem;padding:2rem}}h1{{font-size:1.5rem}}</style><main><h1>{heading}</h1><p>{detail}</p></main>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_get_to_the_callback_path_counts() {
        assert_eq!(
            target(b"GET /oauth/callback?code=x&state=y HTTP/1.1\r\n\r\n").as_deref(),
            Some("/oauth/callback?code=x&state=y")
        );
        assert!(target(b"GET /favicon.ico HTTP/1.1\r\n\r\n").is_none());
        assert!(target(b"POST /oauth/callback HTTP/1.1\r\n\r\n").is_none());
        assert!(target(b"GET /oauth/callbackevil HTTP/1.1\r\n\r\n").is_none());
    }
}
