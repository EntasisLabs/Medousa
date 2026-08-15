use bytes::BytesMut;
use futures_util::StreamExt;
use reqwest::Client;
use tauri::{AppHandle, Emitter};

use crate::workshop_transport::WorkshopByteStream;

const MAX_SSE_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct SseFrame {
    event: Option<String>,
    data: Vec<u8>,
    id: Option<String>,
    retry_ms: Option<u64>,
}

struct SseDecoder {
    buffer: BytesMut,
    max_frame_bytes: usize,
}

impl SseDecoder {
    fn new(max_frame_bytes: usize) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_frame_bytes,
        }
    }

    fn feed(&mut self, mut chunk: &[u8], mut on_frame: impl FnMut(SseFrame)) -> Result<(), String> {
        while !chunk.is_empty() {
            let capacity = self
                .max_frame_bytes
                .saturating_add(4)
                .saturating_sub(self.buffer.len());
            if capacity == 0 {
                return Err(format!("SSE frame exceeds {} bytes", self.max_frame_bytes));
            }
            let take = capacity.min(chunk.len());
            self.buffer.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            self.drain_frames(&mut on_frame)?;
            if self.buffer.len() > self.max_frame_bytes {
                return Err(format!("SSE frame exceeds {} bytes", self.max_frame_bytes));
            }
        }
        Ok(())
    }

    fn finish(&mut self, mut on_frame: impl FnMut(SseFrame)) -> Result<(), String> {
        self.drain_frames(&mut on_frame)?;
        if self.buffer.is_empty() {
            return Ok(());
        }
        let tail = self.buffer.split().freeze();
        if let Some(frame) = parse_sse_frame(&tail)? {
            on_frame(frame);
        }
        Ok(())
    }

    fn drain_frames(&mut self, on_frame: &mut impl FnMut(SseFrame)) -> Result<(), String> {
        while let Some((frame_len, consumed)) = frame_boundary(&self.buffer) {
            if frame_len > self.max_frame_bytes {
                return Err(format!("SSE frame exceeds {} bytes", self.max_frame_bytes));
            }
            let mut encoded = self.buffer.split_to(consumed);
            encoded.truncate(frame_len);
            if let Some(frame) = parse_sse_frame(&encoded)? {
                on_frame(frame);
            }
        }
        Ok(())
    }
}

fn frame_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    for index in 0..bytes.len() {
        if bytes[index] == b'\n' {
            if index >= 1 && bytes[index - 1] == b'\n' {
                return Some((index - 1, index + 1));
            }
            if index >= 3
                && bytes[index - 3] == b'\r'
                && bytes[index - 2] == b'\n'
                && bytes[index - 1] == b'\r'
            {
                return Some((index - 3, index + 1));
            }
        } else if bytes[index] == b'\r' && index >= 1 && bytes[index - 1] == b'\r' {
            return Some((index - 1, index + 1));
        }
    }
    None
}

fn parse_sse_frame(encoded: &[u8]) -> Result<Option<SseFrame>, String> {
    let mut event = None;
    let mut data = Vec::new();
    let mut saw_data = false;
    let mut id = None;
    let mut retry_ms = None;

    for raw_line in encoded.split(|byte| *byte == b'\n' || *byte == b'\r') {
        if raw_line.is_empty() || raw_line.starts_with(b":") {
            continue;
        }
        let (field, mut value) = raw_line
            .iter()
            .position(|byte| *byte == b':')
            .map_or((raw_line, &b""[..]), |colon| {
                (&raw_line[..colon], &raw_line[colon + 1..])
            });
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        match field {
            b"event" => event = Some(parse_sse_text(value, "event")?),
            b"data" => {
                if saw_data {
                    data.push(b'\n');
                }
                data.extend_from_slice(value);
                saw_data = true;
            }
            b"id" if !value.contains(&0) => id = Some(parse_sse_text(value, "id")?),
            b"retry" => {
                retry_ms = std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse().ok());
            }
            _ => {}
        }
    }

    if !saw_data || data == b"[DONE]" {
        return Ok(None);
    }
    Ok(Some(SseFrame {
        event,
        data,
        id,
        retry_ms,
    }))
}

fn parse_sse_text(bytes: &[u8], field: &str) -> Result<String, String> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| format!("invalid UTF-8 in SSE {field}: {error}"))
}

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

pub async fn stream_sse_json<T>(
    app: &AppHandle,
    client: &Client,
    url: &str,
    event_name: &str,
    error_event: &str,
    cancel: tokio::sync::watch::Receiver<bool>,
) where
    T: serde::de::DeserializeOwned + serde::Serialize,
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
    pump_sse_stream::<_, T>(app, &mut stream, event_name, error_event, cancel).await;
}

pub async fn stream_sse_json_workshop<T>(
    app: &AppHandle,
    mut source: WorkshopByteStream,
    event_name: &str,
    error_event: &str,
    cancel: tokio::sync::watch::Receiver<bool>,
) where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let mut cancel_rx = cancel;
    let mut decoder = SseDecoder::new(MAX_SSE_FRAME_BYTES);

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

        if let Err(error) = decoder.feed(&chunk, |frame| {
            emit_decoded_frame::<T>(app, frame, event_name, error_event);
        }) {
            emit_stream_error(app, error_event, &error, false, "sse", "frame");
            return;
        }
    }

    if !*cancel_rx.borrow() {
        if let Err(error) = decoder.finish(|frame| {
            emit_decoded_frame::<T>(app, frame, event_name, error_event);
        }) {
            emit_stream_error(app, error_event, &error, false, "sse", "eof_frame");
            return;
        }
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

async fn pump_sse_stream<S, T>(
    app: &AppHandle,
    stream: &mut S,
    event_name: &str,
    error_event: &str,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) where
    S: futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let mut decoder = SseDecoder::new(MAX_SSE_FRAME_BYTES);

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
                if let Err(error) = decoder.finish(|frame| {
                    emit_decoded_frame::<T>(app, frame, event_name, error_event);
                }) {
                    emit_stream_error(app, error_event, &error, false, "sse", "eof_frame");
                    break;
                }
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

        if let Err(error) = decoder.feed(&chunk, |frame| {
            emit_decoded_frame::<T>(app, frame, event_name, error_event);
        }) {
            emit_stream_error(app, error_event, &error, false, "sse", "frame");
            break;
        }
    }
}

fn emit_decoded_frame<T>(app: &AppHandle, frame: SseFrame, event_name: &str, error_event: &str)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    match serde_json::from_slice::<T>(&frame.data) {
        Ok(payload) => {
            let _ = app.emit(event_name, &payload);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_chunks(chunks: &[&[u8]]) -> Result<Vec<SseFrame>, String> {
        let mut decoder = SseDecoder::new(1024);
        let mut frames = Vec::new();
        for chunk in chunks {
            decoder.feed(chunk, |frame| frames.push(frame))?;
        }
        Ok(frames)
    }

    #[test]
    fn fragmented_utf8_and_crlf_decode_without_loss() {
        let utf8 = "👋".as_bytes();
        let prefix = b"event: turn\r\nid: 7\r\nretry: 250\r\ndata: hello ";
        let suffix = b"\r\n\r\n";
        let frames = feed_chunks(&[prefix, &utf8[..2], &utf8[2..], suffix]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event.as_deref(), Some("turn"));
        assert_eq!(frames[0].id.as_deref(), Some("7"));
        assert_eq!(frames[0].retry_ms, Some(250));
        assert_eq!(std::str::from_utf8(&frames[0].data).unwrap(), "hello 👋");
    }

    #[test]
    fn comments_and_multiline_data_follow_sse_field_rules() {
        let frames =
            feed_chunks(&[b": heartbeat\nunknown: ignored\ndata:first\ndata: second\n\n"]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, b"first\nsecond");
    }

    #[test]
    fn eof_dispatches_a_complete_partial_frame_once() {
        let mut decoder = SseDecoder::new(1024);
        let mut frames = Vec::new();
        decoder
            .feed(b"data: {\"ok\":true}", |frame| frames.push(frame))
            .unwrap();
        assert!(frames.is_empty());
        decoder.finish(|frame| frames.push(frame)).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, br#"{"ok":true}"#);
        decoder.finish(|frame| frames.push(frame)).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn oversized_unterminated_frame_is_rejected_at_the_bound() {
        let mut decoder = SseDecoder::new(16);
        let error = decoder.feed(&[b'x'; 64], |_| {}).unwrap_err();
        assert!(error.contains("exceeds 16 bytes"));
        assert!(decoder.buffer.len() <= 20);
    }

    #[test]
    fn done_and_comment_only_frames_are_not_dispatched() {
        let frames = feed_chunks(&[b"data: [DONE]\n\n: keep-alive\n\n"]).unwrap();
        assert!(frames.is_empty());
    }
}
