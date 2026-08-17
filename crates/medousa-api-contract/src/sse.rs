use thiserror::Error;

/// Incremental SSE decoder shared by generated language codecs.
#[derive(Debug, Default)]
pub struct SseCodec {
    buffer: Vec<u8>,
    event: String,
    id: Option<String>,
    data: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub id: Option<String>,
    pub data: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SseDecodeError {
    #[error("utf-8 split across chunks is buffered; invalid utf-8 at frame boundary")]
    InvalidUtf8,
    #[error("SSE frame exceeds the {0} byte limit")]
    FrameTooLarge(usize),
}

const DEFAULT_FRAME_LIMIT: usize = 1024 * 1024;

impl SseCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, SseDecodeError> {
        self.push_limited(chunk, DEFAULT_FRAME_LIMIT)
    }

    pub fn push_limited(
        &mut self,
        chunk: &[u8],
        frame_limit: usize,
    ) -> Result<Vec<SseEvent>, SseDecodeError> {
        if self.buffer.len().saturating_add(chunk.len()) > frame_limit {
            return Err(SseDecodeError::FrameTooLarge(frame_limit));
        }
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(split) = find_line(&self.buffer) {
            let line_bytes = self.buffer.drain(..split.end).collect::<Vec<_>>();
            let line = std::str::from_utf8(&line_bytes[..split.line_len])
                .map_err(|_| SseDecodeError::InvalidUtf8)?;
            if line.is_empty() {
                if let Some(event) = self.take_event() {
                    events.push(event);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, rest)) => (field, rest.strip_prefix(' ').unwrap_or(rest)),
                None => (line, ""),
            };
            match field {
                "event" => self.event = value.to_string(),
                "id" => self.id = Some(value.to_string()),
                "data" => self.data.push(value.to_string()),
                "retry" => {}
                _ => {}
            }
        }
        Ok(events)
    }

    fn take_event(&mut self) -> Option<SseEvent> {
        if self.event.is_empty() && self.data.is_empty() && self.id.is_none() {
            return None;
        }
        let event = SseEvent {
            event: if self.event.is_empty() {
                "message".into()
            } else {
                std::mem::take(&mut self.event)
            },
            id: self.id.take(),
            data: self.data.join("\n"),
        };
        self.data.clear();
        Some(event)
    }
}

struct LineSplit {
    line_len: usize,
    end: usize,
}

fn find_line(buffer: &[u8]) -> Option<LineSplit> {
    for index in 0..buffer.len() {
        match buffer[index] {
            b'\n' => {
                let line_len = if index > 0 && buffer[index - 1] == b'\r' {
                    index - 1
                } else {
                    index
                };
                return Some(LineSplit {
                    line_len,
                    end: index + 1,
                });
            }
            b'\r' if index + 1 < buffer.len() && buffer[index + 1] == b'\n' => {
                return Some(LineSplit {
                    line_len: index,
                    end: index + 2,
                });
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_crlf_and_utf8_across_chunks() {
        let mut codec = SseCodec::new();
        let café = "data: caf\n";
        // split the UTF-8 é across chunks after assembling a previous event
        let first = codec.push(b"event: token\ndata: hi\n\n").unwrap();
        assert_eq!(first[0].event, "token");
        assert_eq!(first[0].data, "hi");

        let bytes = "data: caf\u{e9}\n\n".as_bytes();
        let split_at = bytes.iter().position(|b| *b >= 0x80).unwrap();
        assert!(codec.push(&bytes[..split_at]).unwrap().is_empty());
        let second = codec.push(&bytes[split_at..]).unwrap();
        assert_eq!(second[0].data, "caf\u{e9}");
        let _ = café;
    }

    #[test]
    fn multiline_data_and_heartbeats() {
        let mut codec = SseCodec::new();
        let events = codec
            .push(b": keep-alive\n\nid: 3\ndata: line1\ndata: line2\n\n")
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.as_deref(), Some("3"));
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn oversized_frames_fail() {
        let mut codec = SseCodec::new();
        let err = codec.push_limited(&[b'x'; 8], 4).unwrap_err();
        assert!(matches!(err, SseDecodeError::FrameTooLarge(4)));
    }
}
