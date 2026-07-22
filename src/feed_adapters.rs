//! Adapters from existing runtime signals into environment feed bus.

use chrono::Utc;
use medousa_types::feed::{FeedRef, FeedSource, WORKSHOP_PULSE_FEED_ID};
use serde_json::{json, Value};

use crate::agent_runtime::turn_worker::{TurnWorkRecord, TurnWorkStatus};
use crate::feed_bus::{publish, FeedPublishRequest};
use crate::recurring_feed::FeedPayloadMode;

pub const TRIP_LONDON_TRAINS_FEED_ID: &str = "trip.london.trains";
pub const SUMMER_AI_DIGEST_FEED_ID: &str = "summer-ai-digest";

pub const WORKSHOP_PULSE: &str = WORKSHOP_PULSE_FEED_ID;

pub async fn publish_workshop_started(record: &TurnWorkRecord) {
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Bound workshop started — {}", truncate(&record.task_prompt, 80)),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "started",
            "goal": truncate(&record.task_prompt, 160),
            "workId": record.work_id,
            "sessionId": record.session_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_working(
    record: &TurnWorkRecord,
    round: u32,
    tools: &[String],
) {
    let tool_list: Vec<_> = tools.iter().take(12).cloned().collect();
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Workshop round {round} — {}", tool_list.join(", ")),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "working",
            "round": round,
            "tools": tool_list,
            "workId": record.work_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_synthesis(record: &TurnWorkRecord, excerpt: &str) {
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Synthesis ready — {}", truncate(excerpt, 80)),
        refs: workshop_refs(record),
        payload_slice: Some(json!({
            "phase": "synthesis",
            "excerpt": truncate(excerpt, 240),
            "workId": record.work_id,
        })),
        payload_max_bytes: None,
    })
    .await;
}

pub async fn publish_workshop_terminal(record: &TurnWorkRecord, phase: &str, excerpt: Option<&str>) {
    let status = match record.status {
        TurnWorkStatus::Completed => "done",
        TurnWorkStatus::Failed => "failed",
        TurnWorkStatus::Cancelled => "cancelled",
        _ => "wrapping_up",
    };
    let mut payload = json!({
        "phase": phase,
        "status": status,
        "workId": record.work_id,
    });
    if let Some(excerpt) = excerpt.filter(|value| !value.trim().is_empty()) {
        payload["excerpt"] = Value::String(truncate(excerpt, 240));
    }
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: WORKSHOP_PULSE.to_string(),
        source: medousa_types::feed::FeedSource::BoundWorkshop,
        summary: format!("Workshop {status}"),
        refs: workshop_refs(record),
        payload_slice: Some(payload),
        payload_max_bytes: None,
    })
    .await;
}

fn workshop_refs(record: &TurnWorkRecord) -> Vec<FeedRef> {
    vec![
        FeedRef {
            ref_type: "work".to_string(),
            ref_id: record.work_id.clone(),
        },
        FeedRef {
            ref_type: "session".to_string(),
            ref_id: record.session_id.clone(),
        },
    ]
}

fn truncate(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect::<String>() + "…"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobTerminalPhase {
    TickSucceeded,
    TickFailed,
}

impl JobTerminalPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::TickSucceeded => "tick_succeeded",
            Self::TickFailed => "tick_failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecurringTickContext {
    pub recurring_id: String,
    pub job_id: String,
    pub job_type: String,
    pub phase: JobTerminalPhase,
    pub output_excerpt: Option<String>,
    pub parsed_poll: Option<Value>,
    pub payload_mode: FeedPayloadMode,
}

pub async fn publish_recurring_tick(feed_id: &str, ctx: &RecurringTickContext) {
    let slice = build_recurring_tick_slice(ctx);
    let summary = match ctx.phase {
        JobTerminalPhase::TickSucceeded => format!(
            "Recurring tick succeeded — {}",
            ctx.job_type.trim()
        ),
        JobTerminalPhase::TickFailed => format!(
            "Recurring tick failed — {}",
            ctx.job_type.trim()
        ),
    };
    let _ = publish(FeedPublishRequest {
        profile_id: None,
        feed_id: feed_id.to_string(),
        source: FeedSource::RecurringJob,
        summary,
        refs: vec![
            FeedRef {
                ref_type: "job".to_string(),
                ref_id: ctx.job_id.clone(),
            },
            FeedRef {
                ref_type: "recurring".to_string(),
                ref_id: ctx.recurring_id.clone(),
            },
        ],
        payload_slice: Some(slice),
        payload_max_bytes: None,
    })
    .await;
}

pub fn build_recurring_tick_slice(ctx: &RecurringTickContext) -> Value {
    let checked_at = Utc::now().to_rfc3339();
    let status_code = ctx
        .parsed_poll
        .as_ref()
        .and_then(|parsed| parsed.get("statusCode"))
        .and_then(|v| v.as_i64());

    let excerpt_cap = match ctx.payload_mode {
        FeedPayloadMode::Summary => 120,
        FeedPayloadMode::ParsedPoll => 480,
        FeedPayloadMode::RawExcerpt => 960,
    };

    let excerpt = ctx
        .output_excerpt
        .as_deref()
        .map(|text| truncate(text, excerpt_cap))
        .or_else(|| {
            ctx.parsed_poll
                .as_ref()
                .and_then(|parsed| parsed.get("bodyExcerpt"))
                .and_then(|v| v.as_str())
                .map(|text| truncate(text, excerpt_cap))
        });

    let body_cap = match ctx.payload_mode {
        FeedPayloadMode::Summary => 480,
        FeedPayloadMode::ParsedPoll => 1600,
        FeedPayloadMode::RawExcerpt => 1800,
    };
    let body = ctx
        .output_excerpt
        .as_deref()
        .or_else(|| {
            ctx.parsed_poll
                .as_ref()
                .and_then(|parsed| parsed.get("bodyExcerpt"))
                .and_then(|v| v.as_str())
        })
        .map(|text| truncate(text, body_cap))
        .filter(|value| !value.is_empty());

    let mut payload = json!({
        "phase": ctx.phase.as_str(),
        "checkedAt": checked_at,
        "recurringId": ctx.recurring_id,
        "jobId": ctx.job_id,
        "jobType": ctx.job_type,
    });

    if let Some(status_code) = status_code {
        payload["statusCode"] = json!(status_code);
    }
    if let Some(excerpt) = excerpt.filter(|value| !value.is_empty()) {
        payload["excerpt"] = Value::String(excerpt);
    }
    if let Some(body) = body {
        payload["body"] = Value::String(body.clone());
        payload["datatype"] = Value::String(infer_feed_datatype(&body));
        payload["finishedAt"] = Value::String(checked_at.clone());
    }

    if ctx.payload_mode == FeedPayloadMode::ParsedPoll
        && let Some(parsed) = &ctx.parsed_poll {
            if let Some(headers) = parsed.get("headerCount") {
                payload["headerCount"] = headers.clone();
            }
            if let Some(body_len) = parsed.get("bodyLength") {
                payload["bodyLength"] = body_len.clone();
            }
        }

    payload
}

pub fn infer_feed_datatype(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "text".to_string();
    }
    if trimmed.starts_with("data:image/") || looks_like_image_ref(trimmed) {
        return "image".to_string();
    }
    if serde_json::from_str::<Value>(trimmed).is_ok() {
        return "json".to_string();
    }
    if looks_like_csv(trimmed) {
        return "csv".to_string();
    }
    if trimmed.contains("# ")
        || trimmed.contains("**")
        || trimmed.contains("\n- ")
        || trimmed.contains("\n* ")
    {
        return "md".to_string();
    }
    "text".to_string()
}

fn looks_like_image_ref(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("vault/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".svg")
}

fn looks_like_csv(text: &str) -> bool {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let comma_lines = lines
        .iter()
        .filter(|line| line.contains(','))
        .count();
    comma_lines >= 2 && comma_lines * 2 >= lines.len()
}

/// Parse http.fetch / http_poll grapheme output into bounded structured fields.
pub fn parse_http_poll_output(output: &str) -> Option<Value> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_str(trimmed)
        .ok()
        .or_else(|| extract_json_object(trimmed))?;

    let status_code = parsed
        .get("status")
        .or_else(|| parsed.get("statusCode"))
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)));

    let body = parsed
        .get("body")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let header_count = parsed
        .get("headers")
        .and_then(|v| v.as_array())
        .map(|headers| headers.len());

    Some(json!({
        "statusCode": status_code,
        "bodyExcerpt": body.as_deref().map(|text| truncate(text, 480)),
        "bodyLength": body.as_ref().map(|text| text.chars().count()),
        "headerCount": header_count,
    }))
}

fn extract_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

#[cfg(test)]
mod recurring_feed_tests {
    use super::*;

    #[test]
    fn parse_http_poll_output_reads_status_and_body() {
        let parsed = parse_http_poll_output(
            r#"{"status":200,"body":"train A on time","headers":[{"name":"content-type","value":"text/plain"}]}"#,
        )
        .expect("poll json");

        assert_eq!(parsed["statusCode"], json!(200));
        assert_eq!(parsed["headerCount"], json!(1));
    }

    #[test]
    fn build_recurring_tick_slice_respects_summary_mode() {
        let ctx = RecurringTickContext {
            recurring_id: "trip-poll".to_string(),
            job_id: "job-1".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            phase: JobTerminalPhase::TickSucceeded,
            output_excerpt: Some("x".repeat(500)),
            parsed_poll: Some(json!({"statusCode": 200})),
            payload_mode: FeedPayloadMode::Summary,
        };
        let slice = build_recurring_tick_slice(&ctx);
        let excerpt = slice["excerpt"].as_str().unwrap_or("");
        assert!(excerpt.chars().count() <= 121);
    }

    #[tokio::test]
    async fn trip_poll_demo_publishes_bounded_feed_event() {
        use crate::feed_store::feed_store;

        let ctx = RecurringTickContext {
            recurring_id: "trip-london-poll".to_string(),
            job_id: "job-trip-1".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            phase: JobTerminalPhase::TickSucceeded,
            output_excerpt: Some(
                r#"{"status":200,"body":"Platform 1 — on time","headers":[]}"#.to_string(),
            ),
            parsed_poll: parse_http_poll_output(
                r#"{"status":200,"body":"Platform 1 — on time","headers":[]}"#,
            ),
            payload_mode: FeedPayloadMode::ParsedPoll,
        };

        publish_recurring_tick(TRIP_LONDON_TRAINS_FEED_ID, &ctx).await;

        let events = feed_store()
            .tail(
                medousa_types::environment_default::DEFAULT_PROFILE_ID,
                TRIP_LONDON_TRAINS_FEED_ID,
                1,
            )
            .await;
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.feed_id, TRIP_LONDON_TRAINS_FEED_ID);
        assert_eq!(event.source, FeedSource::RecurringJob.as_str());
        let payload = event.payload.as_ref().expect("payload slice");
        assert_eq!(payload["phase"], "tick_succeeded");
        assert_eq!(payload["statusCode"], json!(200));
        assert_eq!(payload["recurringId"], "trip-london-poll");
        assert_eq!(payload["datatype"], json!("text"));
        assert!(payload["body"].as_str().is_some());
        assert!(payload["excerpt"].as_str().is_some());
        let serialized = serde_json::to_string(payload).expect("serialize");
        assert!(serialized.len() <= 2048);
    }

    #[tokio::test]
    async fn summer_digest_demo_publishes_markdown_body() {
        use crate::feed_store::feed_store;

        let digest = "# Summer AI digest\n\n- Gemini updates\n- Claude releases";
        let ctx = RecurringTickContext {
            recurring_id: "summer-digest-poll".to_string(),
            job_id: "job-digest-1".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            phase: JobTerminalPhase::TickSucceeded,
            output_excerpt: Some(digest.to_string()),
            parsed_poll: None,
            payload_mode: FeedPayloadMode::RawExcerpt,
        };

        publish_recurring_tick(SUMMER_AI_DIGEST_FEED_ID, &ctx).await;

        let latest = feed_store()
            .latest_good(
                medousa_types::environment_default::DEFAULT_PROFILE_ID,
                SUMMER_AI_DIGEST_FEED_ID,
            )
            .await
            .expect("latest good");
        assert_eq!(latest.feed_id, SUMMER_AI_DIGEST_FEED_ID);
        assert_eq!(latest.datatype, "md");
        assert!(latest.body.contains("Summer AI digest"));
        assert_eq!(latest.job_id.as_deref(), Some("job-digest-1"));
    }
}
