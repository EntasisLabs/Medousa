//! Real HTTP tests: declared and chunked lengths, auth, errors and decode bounds.
use medousa_sdk::{HttpTransport, Transport};
use std::io::{Read, Write};
fn server(response: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let _ = stream.write_all(&response);
        String::from_utf8(bytes).unwrap()
    });
    (format!("http://{address}"), task)
}
fn transport() -> HttpTransport {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        "Bearer fixture".parse().unwrap(),
    );
    HttpTransport::with_client(
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap(),
    )
}
#[tokio::test]
async fn bounded_json_post_keeps_credentials_and_request_body() {
    let (url, task) = server(
        b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}".to_vec(),
    );
    let result = transport()
        .post_json_bounded(&url, "/fetch", serde_json::json!({"session_id":"one"}), 11)
        .await
        .unwrap();
    assert_eq!(result["ok"], true);
    let request = task.join().unwrap();
    assert!(request.starts_with("POST /fetch HTTP/1.1"));
    assert!(
        request
            .to_lowercase()
            .contains("authorization: bearer fixture")
    );
    assert!(request.contains("\"session_id\":\"one\""));
}
#[tokio::test]
async fn bounded_json_post_rejects_declared_chunked_error_and_invalid_bodies() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n6\r\n123456\r\n6\r\n123456\r\n0\r\n\r\n".to_vec(),
        b"HTTP/1.1 500 Error\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbad".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbad".to_vec(),
    ] {
        let (url, task) = server(response);
        assert!(transport().post_json_bounded(&url, "/fetch", serde_json::json!({}), 10).await.is_err());
        task.join().unwrap();
    }
}
