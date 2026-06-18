use futures_util::StreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};

pub async fn stream_sse_json<T, F>(
    app: &AppHandle,
    client: &Client,
    url: &str,
    event_name: &str,
    error_event: &str,
    mut on_payload: F,
    cancel: tokio::sync::watch::Receiver<bool>,
) where
    T: serde::de::DeserializeOwned,
    F: FnMut(T),
{
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(err) => {
            let _ = app.emit(error_event, serde_json::json!({ "message": err.to_string() }));
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = app.emit(
            error_event,
            serde_json::json!({ "message": format!("HTTP {status}: {body}") }),
        );
        return;
    }

    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    let mut cancel_rx = cancel;

    loop {
        if *cancel_rx.borrow() {
            break;
        }

        let next = tokio::select! {
            chunk = stream.next() => chunk,
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    break;
                }
                continue;
            }
        };

        let Some(chunk) = next else {
            if !*cancel_rx.borrow() {
                let _ = app.emit(
                    error_event,
                    serde_json::json!({ "message": "SSE stream ended unexpectedly" }),
                );
            }
            break;
        };

        let chunk = match chunk {
            Ok(bytes) => bytes,
            Err(err) => {
                let _ = app.emit(error_event, serde_json::json!({ "message": err.to_string() }));
                break;
            }
        };

        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buf.find("\n\n") {
            let frame = buf[..idx].to_string();
            buf = buf[idx + 2..].to_string();

            let Some(data) = parse_sse_data(&frame) else {
                continue;
            };

            match serde_json::from_str::<T>(&data) {
                Ok(payload) => {
                    on_payload(payload);
                    let _ = app.emit(event_name, &data);
                }
                Err(err) => {
                    let _ = app.emit(
                        error_event,
                        serde_json::json!({ "message": format!("invalid SSE JSON: {err}") }),
                    );
                }
            }
        }
    }
}

fn parse_sse_data(frame: &str) -> Option<String> {
    let mut data_lines = Vec::new();
    for line in frame.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    }
}
