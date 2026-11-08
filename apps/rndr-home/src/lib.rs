//! App-owned projection only. Session authority and inference stay in Medousa.
use medousa_types::{InteractiveTurnRequest, TurnStreamEnvelopeV3, TurnStreamEventV3};

#[derive(Clone, Debug, Default)]
pub struct Chat {
    pub session: Option<String>,
    /// Structured pending execution text; never inferred by splitting Markdown.
    pub live_messages: Vec<(String, String)>,
    pub status: String,
    pub busy: bool,
    pub uncertain: bool,
    turn: Option<String>,
    seq: u64,
    // Segment IDs and byte lengths only; prose lives solely in live_messages.
    segments: Vec<(String, usize)>,
    rendered_segments: usize,
}
impl Chat {
    pub fn offline() -> Self {
        Self {
            status: "Offline preview · no connection · no inference".into(),
            ..Self::default()
        }
    }
    pub fn history(session: String, active: bool) -> Self {
        Self {
            session: Some(session),
            uncertain: active,
            status: if active {
                "Reattaching active turn…".into()
            } else {
                "History reconciled · non-atomic daemon reads; reload if activity changes".into()
            },
            ..Self::default()
        }
    }
    pub fn resume(&mut self, turn: String) {
        self.busy = true;
        self.live_messages = vec![("assistant".into(), String::new())];
        self.uncertain = false;
        self.turn = Some(turn);
        self.seq = 0;
        self.segments.clear();
        self.rendered_segments = 0;
    }
    pub fn begin(&mut self, prompt: &str) -> Result<(), &'static str> {
        if self.busy || self.uncertain {
            return Err("Reload history before sending another turn");
        }
        if self.session.is_none() {
            return Err("Create or open a chat first");
        }
        if prompt.trim().is_empty() {
            return Err("Write a message first");
        }
        self.busy = true;
        self.live_messages = vec![
            ("user".into(), prompt.into()),
            ("assistant".into(), String::new()),
        ];
        self.seq = 0;
        self.turn = None;
        self.segments.clear();
        self.rendered_segments = 0;
        self.status = "Starting turn…".into();
        Ok(())
    }
    pub fn admitted(&mut self, turn: String) {
        self.turn = Some(turn);
    }
    pub fn fail(&mut self, message: String) {
        self.busy = false;
        self.uncertain = true;
        self.status = format!(
            "{message} — turn state unknown; reload history before sending. No automatic resubmit."
        );
    }
    pub fn apply(&mut self, envelope: TurnStreamEnvelopeV3) {
        if !self.busy || self.turn.as_deref() != Some(&envelope.turn_id) || envelope.seq <= self.seq
        {
            return;
        }
        self.seq = envelope.seq;
        use TurnStreamEventV3::*;
        match envelope.event {
            AssistantTextStarted { segment_id, .. } => {
                if !self.segments.iter().any(|s| s.0 == segment_id) {
                    self.segments.push((segment_id, 0));
                }
            }
            ContentAppend { segment_id, text } => {
                let index = self
                    .segments
                    .iter()
                    .position(|s| s.0 == segment_id)
                    .unwrap_or_else(|| {
                        self.segments.push((segment_id, 0));
                        self.segments.len() - 1
                    });
                if let Some((_, body)) = self.live_messages.last_mut() {
                    // Materialize separators only when content arrives, matching
                    // chronological segment joining even for empty segments.
                    for _ in self.rendered_segments.max(1)..self.segments.len() {
                        body.push_str("\n\n");
                    }
                    self.rendered_segments = self.segments.len();
                    let offset =
                        self.segments[..=index].iter().map(|s| s.1).sum::<usize>() + 2 * index;
                    // The common case appends in place. Interleaved earlier
                    // segments shift only the live suffix, never chat history.
                    body.insert_str(offset, &text);
                    self.segments[index].1 += text.len();
                }
            }
            Status {
                phase,
                operator_message,
                ..
            } => self.status = operator_message.unwrap_or(phase),
            Progress { message, .. } => self.status = message,
            Error {
                operator_message, ..
            } => self.status = operator_message,
            PermissionRequest { message, .. } => {
                self.status = format!(
                    "Permission needed: {message}. Respond in the existing Medousa app; this port cannot approve."
                )
            }
            SecretRequest { label, .. } => {
                self.status = format!(
                    "Credential requested: {label}. Use existing Medousa app; never paste secrets here."
                )
            }
            BudgetApprovalRequired { reason, .. } => {
                self.status = format!("Budget approval needed: {reason}. Use existing Medousa app.")
            }
            TurnCompleted {
                outcome,
                operator_message,
                ..
            } => {
                // Aggregate text is not a replacement for chronological segments.
                self.busy = false;
                self.status = operator_message.unwrap_or_else(|| format!("{outcome:?}"));
            }
            _ => {}
        }
    }
}

pub fn turn_request(
    session: &str,
    prompt: &str,
    provider: &str,
    model: &str,
) -> InteractiveTurnRequest {
    // Deserialize the shared wire contract; no app-owned parallel protocol.
    serde_json::from_value(serde_json::json!({
        "session_id": session, "prompt": prompt, "persist_user_turn": true,
        "response_depth_mode": "standard", "provider": provider, "model": model,
        "stage_routing": medousa_types::StageRoutingMatrix::default_for(provider, model)
    }))
    .expect("static interactive request must match Medousa types")
}

/// Only the existing local credential boundary is supported in this slice.
/// No remote redirects, userinfo, or arbitrary credential forwarding.
pub fn local_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid daemon URL")?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !local
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "This slice requires a loopback HTTP(S) daemon origin, without userinfo/path/query"
                .into(),
        );
    }
    Ok(url)
}
pub fn stream_path(path: &str) -> Result<&str, String> {
    if !path.starts_with("/v1/") || path.contains("://") || path.contains('\\') {
        return Err(
            "Daemon returned an unsupported stream URL; refusing credential forwarding".into(),
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(seq: u64, event: serde_json::Value) -> TurnStreamEnvelopeV3 {
        serde_json::from_value(serde_json::json!({"schema_version":3,"turn_id":"t","seq":seq,"emitted_at_utc":"2026-11-07T00:00:00Z","event":event})).unwrap()
    }
    #[test]
    fn send_guards_and_request() {
        let mut c = Chat::default();
        assert!(c.begin("hello").is_err());
        c.session = Some("s".into());
        assert!(c.begin("  ").is_err());
        c.begin("hello").unwrap();
        assert!(c.begin("again").is_err());
        let r = turn_request("s", "hello", "provider", "model");
        assert!(r.persist_user_turn);
        assert!(!r.code_project_setup_authorized);
        assert!(r.wake.is_none());
        let wire = serde_json::to_value(r).unwrap();
        assert_eq!(wire["provider"], "provider");
        assert_eq!(wire["model"], "model");
        for role in medousa_types::StageRoutingMatrix::roles() {
            assert_eq!(wire["stage_routing"][role]["provider"], "provider");
            assert_eq!(wire["stage_routing"][role]["model"], "model");
        }
    }
    #[test]
    fn stream_dedup_chronology_terminal() {
        let mut c = Chat {
            session: Some("s".into()),
            ..Default::default()
        };
        c.begin("hi").unwrap();
        c.admitted("t".into());
        let chunk = event(
            1,
            serde_json::json!({"type":"content_append","segment_id":"a","text":"one"}),
        );
        c.apply(chunk.clone());
        c.apply(chunk);
        c.apply(event(
            2,
            serde_json::json!({"type":"content_append","segment_id":"b","text":"two"}),
        ));
        c.apply(event(3, serde_json::json!({"type":"turn_completed","outcome":"cancelled","aggregate_text":"not a replacement"})));
        assert_eq!(c.live_messages.last().unwrap().1, "one\n\ntwo");
        assert!(!c.busy);
        assert_eq!(c.status, "Cancelled");
    }
    #[test]
    fn interleaved_unicode_segments_use_one_in_place_body() {
        let mut c = Chat::history("s".into(), false);
        c.begin("prompt").unwrap();
        c.admitted("t".into());
        let body = &mut c.live_messages.last_mut().unwrap().1;
        body.reserve(4096);
        let allocation = body.as_ptr();
        for (seq, segment_id, text) in [
            (1, "a", "α"),
            (2, "b", "世界"),
            (3, "a", "β"),
            (4, "c", ""),
            (5, "b", "!"),
            (6, "c", "🙂"),
        ] {
            c.apply(event(
                seq,
                serde_json::json!({
                    "type":"content_append", "segment_id":segment_id, "text":text
                }),
            ));
            assert_eq!(c.live_messages.last().unwrap().1.as_ptr(), allocation);
        }
        assert_eq!(c.live_messages.last().unwrap().1, "αβ\n\n世界!\n\n🙂");
        assert_eq!(
            c.segments.iter().map(|s| s.1).sum::<usize>(),
            "αβ世界!🙂".len()
        );
        assert_eq!(c.live_messages[0].1, "prompt");
    }
    #[test]
    fn empty_started_segments_match_chronological_join_and_resume_resets() {
        let mut c = Chat::history("s".into(), true);
        assert!(c.begin("blocked").is_err());
        c.resume("t".into());
        for (seq, id) in [(1, "a"), (2, "b"), (3, "c")] {
            c.apply(event(
                seq,
                serde_json::json!({
                    "type":"assistant_text_started", "segment_id":id, "model_round":1
                }),
            ));
        }
        assert_eq!(c.live_messages.last().unwrap().1, "");
        c.apply(event(
            4,
            serde_json::json!({"type":"content_append", "segment_id":"b", "text":"middle"}),
        ));
        assert_eq!(c.live_messages.last().unwrap().1, "\n\nmiddle\n\n");
        c.apply(event(
            5,
            serde_json::json!({"type":"content_append", "segment_id":"a", "text":"first"}),
        ));
        assert_eq!(c.live_messages.last().unwrap().1, "first\n\nmiddle\n\n");
        c.resume("other".into());
        assert!(c.segments.is_empty());
        assert_eq!(c.rendered_segments, 0);
        c.apply(event(
            6,
            serde_json::json!({"type":"content_append", "segment_id":"a", "text":"wrong turn"}),
        ));
        assert_eq!(c.live_messages.last().unwrap().1, "");
    }
    #[test]
    fn error_blocks_resubmit() {
        let mut c = Chat {
            session: Some("s".into()),
            ..Default::default()
        };
        c.begin("hi").unwrap();
        c.fail("Disconnected".into());
        assert!(c.begin("retry").is_err());
        assert_eq!(c.live_messages[0], ("user".into(), "hi".into()));
    }
    #[test]
    fn credentials_stay_local() {
        for bad in [
            "https://example.com",
            "http://localhost@evil.test",
            "http://localhost/foo",
            "http://localhost/?x=1",
        ] {
            assert!(local_url(bad).is_err());
        }
        assert!(local_url("http://127.0.0.1:9777").is_ok());
        assert!(local_url("http://[::1]:9777").is_ok());
        assert!(stream_path("https://evil.test/v1/stream").is_err());
        assert!(stream_path("//evil.test").is_err());
    }
}
