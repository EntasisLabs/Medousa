//! App-owned projection only. Session authority and inference stay in Medousa.
use medousa_types::{InteractiveTurnRequest, TurnStreamEnvelopeV3, TurnStreamEventV3};

#[derive(Clone, Debug, Default)]
pub struct Chat {
    pub session: Option<String>,
    pub transcript: String,
    pub status: String,
    pub busy: bool,
    pub uncertain: bool,
    turn: Option<String>,
    seq: u64,
    segments: Vec<(String, String)>,
    prefix: String,
}
impl Chat {
    pub fn offline() -> Self {
        Self {
            status: "Offline preview · no connection · no inference".into(),
            ..Self::default()
        }
    }
    pub fn history(session: String, transcript: String, active: bool) -> Self {
        Self {
            session: Some(session),
            transcript,
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
        self.uncertain = false;
        self.turn = Some(turn);
        self.seq = 0;
        self.segments.clear();
        self.prefix = self.transcript.clone() + "\n\n### Medousa · active turn\n\n";
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
        self.seq = 0;
        self.turn = None;
        self.segments.clear();
        self.prefix = format!(
            "{}\n\n### You\n\n{}\n\n### Medousa\n\n",
            self.transcript, prompt
        );
        self.transcript = self.prefix.clone();
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
                    self.segments.push((segment_id, String::new()));
                }
            }
            ContentAppend { segment_id, text } => {
                if let Some(segment) = self.segments.iter_mut().find(|s| s.0 == segment_id) {
                    segment.1.push_str(&text);
                } else {
                    self.segments.push((segment_id, text));
                }
                self.transcript = self.prefix.clone()
                    + &self
                        .segments
                        .iter()
                        .map(|s| s.1.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
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
        assert!(c.transcript.ends_with("one\n\ntwo"));
        assert!(!c.busy);
        assert_eq!(c.status, "Cancelled");
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
        assert!(c.transcript.contains("hi"));
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
