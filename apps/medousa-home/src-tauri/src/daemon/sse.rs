use futures_util::StreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};

use crate::workshop_transport::WorkshopByteStream;

#[derive(Clone, serde::Serialize)]
struct StreamErrorEvent<'a> {
    message: &'a str,
    recoverable: bool,
    transport: &'a str,
    stage: &'a str,
}

fn emit_stream_error(
    app: &AppHandle,
    event_name: &str,
    message: &str,
    recoverable: bool,
    transport: &str,
    stage: &str,
) {
    let _ = app.emit(
        event_name,
        StreamErrorEvent {
            message,
            recoverable,
            transport,
            stage,
        },
    );
}

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
            emit_stream_error(app, error_event, &err.to_string(), true, "http", "connect");
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        emit_stream_error(
            app,
            error_event,
            &format!("HTTP {status}: {body}"),
            status.is_server_error() || status.as_u16() == 408 || status.as_u16() == 429,
            "http",
            "response_status",
        );
        return;
    }

    let mut stream = response.bytes_stream();
    pump_sse_stream(
        app,
        &mut stream,
        event_name,
        error_event,
        &mut on_payload,
        cancel,
    )
    .await;
}

pub async fn stream_sse_json_workshop<T, F>(
    app: &AppHandle,
    mut source: WorkshopByteStream,
    event_name: &str,
    error_event: &str,
    mut on_payload: F,
    cancel: tokio::sync::watch::Receiver<bool>,
) where
    T: serde::de::DeserializeOwned,
    F: FnMut(T),
{
    let mut cancel_rx = cancel;
    let mut buf = String::new();

    loop {
        if *cancel_rx.borrow() {
            break;
        }

        let next = tokio::select! {
            chunk = source.next_chunk() => chunk,
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    break;
                }
                continue;
            }
        };

        let chunk = match next {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(err) => {
                emit_stream_error(app, error_event, &err, true, "workshop", "read");
                break;
            }
        };

        buf.push_str(&String::from_utf8_lossy(&chunk));
        drain_sse_buffer(app, &mut buf, event_name, error_event, &mut on_payload);
    }

    if !*cancel_rx.borrow() {
        emit_stream_error(
            app,
            error_event,
            "SSE stream ended unexpectedly",
            true,
            "workshop",
            "eof",
        );
    }
}

async fn pump_sse_stream<S, T, F>(
    app: &AppHandle,
    stream: &mut S,
    event_name: &str,
    error_event: &str,
    on_payload: &mut F,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) where
    S: futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    T: serde::de::DeserializeOwned,
    F: FnMut(T),
{
    let mut buf = String::new();

    loop {
        if *cancel.borrow() {
            break;
        }

        let next = tokio::select! {
            chunk = stream.next() => chunk,
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    break;
                }
                continue;
            }
        };

        let Some(chunk) = next else {
            if !*cancel.borrow() {
                emit_stream_error(
                    app,
                    error_event,
                    "SSE stream ended unexpectedly",
                    true,
                    "http",
                    "eof",
                );
            }
            break;
        };

        let chunk = match chunk {
            Ok(bytes) => bytes,
            Err(err) => {
                emit_stream_error(app, error_event, &err.to_string(), true, "http", "read");
                break;
            }
        };

        buf.push_str(&String::from_utf8_lossy(&chunk));
        drain_sse_buffer(app, &mut buf, event_name, error_event, on_payload);
    }
}

fn drain_sse_buffer<T, F>(
    app: &AppHandle,
    buf: &mut String,
    event_name: &str,
    error_event: &str,
    on_payload: &mut F,
) where
    T: serde::de::DeserializeOwned,
    F: FnMut(T),
{
    while let Some(idx) = buf.find("\n\n") {
        let frame = buf[..idx].to_string();
        *buf = buf[idx + 2..].to_string();

        let Some(data) = parse_sse_data(&frame) else {
            continue;
        };

        match serde_json::from_str::<T>(&data) {
            Ok(payload) => {
                on_payload(payload);
                let _ = app.emit(event_name, &data);
            }
            Err(err) => {
                emit_stream_error(
                    app,
                    error_event,
                    &format!("invalid SSE JSON: {err}"),
                    false,
                    "sse",
                    "decode",
                );
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
