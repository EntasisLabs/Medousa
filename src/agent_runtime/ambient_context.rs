//! Runtime ambient context: wall clock, daypart, channel surface, and operator conduct hooks.

use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;

use crate::daemon_api::TurnSurfaceContext;

pub use crate::daemon_api::TurnSurfaceContext as AmbientTurnSurfaceContext;

#[derive(Debug, Clone, Default)]
pub struct ChannelAmbientPolicy {
    pub proactive_allowed: Option<bool>,
    pub identity_channel_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AmbientContextInput<'a> {
    pub session_id: &'a str,
    pub surface: Option<&'a TurnSurfaceContext>,
    pub channel_policy: Option<&'a ChannelAmbientPolicy>,
}

#[derive(Debug, Clone)]
pub struct AmbientContextBlock {
    pub appendix: String,
}

/// Timezone label for ambient blocks (identity env, then `TZ`, else `UTC`).
pub fn resolve_operator_timezone_label() -> String {
    std::env::var("MEDOUSA_IDENTITY_USER_TIMEZONE")
        .ok()
        .or_else(|| std::env::var("TZ").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UTC".to_string())
}

pub fn resolve_operator_timezone() -> Tz {
    resolve_operator_timezone_label()
        .parse::<Tz>()
        .unwrap_or(chrono_tz::UTC)
}

pub fn operator_zoned_now() -> DateTime<Tz> {
    Utc::now().with_timezone(&resolve_operator_timezone())
}

fn daypart_label(hour: u32) -> &'static str {
    match hour {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=21 => "evening",
        _ => "night",
    }
}

fn surface_tone(surface: &str) -> &'static str {
    match surface.trim().to_ascii_lowercase().as_str() {
        "telegram" | "whatsapp" => "concise_mobile_friendly",
        "discord" | "slack" => "threaded_markdown_ok",
        "cli" => "scriptable_terse",
        "tui" => "operator_console_full_detail",
        "api" => "integration_neutral",
        _ => "neutral",
    }
}

pub fn build_ambient_context(input: AmbientContextInput<'_>) -> AmbientContextBlock {
    let zoned = operator_zoned_now();
    let tz_label = resolve_operator_timezone_label();
    let weekday = zoned.format("%A").to_string();
    let local_time = zoned.format("%H:%M").to_string();
    let local_date = zoned.format("%Y-%m-%d").to_string();
    let daypart = daypart_label(zoned.hour());
    let utc_time = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut lines = vec![
        "[MEDOUSA_AMBIENT]".to_string(),
        "version=v2".to_string(),
        format!("utc_now={utc_time}"),
        format!("local_date={local_date}"),
        format!("local_time={local_time}"),
        format!("timezone={tz_label}"),
        format!("weekday={weekday}"),
        format!("daypart={daypart}"),
        format!("session_id={}", input.session_id.trim()),
        "operator_mode=proactive_chief_of_staff".to_string(),
        "conduct=anticipate_intent_one_step_ahead_be_direct_warm_and_useful".to_string(),
        "token_policy=prefer_short_replies_and_single_pass_tool_use_when_sufficient".to_string(),
        "early_exit=allowed_end_turn_early_with_cognition_turn_finish_or_prepare_final_when_done_or_ask_one_sharp_clarifying_question_instead_of_burning_remaining_rounds".to_string(),
    ];

    if let Some(surface) = input.surface {
        if let Some(channel) = surface
            .channel_surface
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("channel_surface={channel}"));
            lines.push(format!("channel_tone={}", surface_tone(channel)));
        }
        if let Some(channel_id) = surface
            .channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("channel_id={channel_id}"));
        }
        if let Some(user_id) = surface
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("user_id={user_id}"));
        }
    }

    if let Some(policy) = input.channel_policy {
        if let Some(channel_type) = policy
            .identity_channel_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("identity_channel_type={channel_type}"));
        }
        if let Some(proactive_allowed) = policy.proactive_allowed {
            lines.push(format!(
                "proactive_messages={}",
                if proactive_allowed { "allowed" } else { "denied" }
            ));
            if proactive_allowed {
                lines.push(
                    "proactive_policy=when_high_confidence_and_low_cost_offer_next_step_or_surface_risk_before_asked".to_string(),
                );
            }
        }
    }

    AmbientContextBlock {
        appendix: lines.join("\n"),
    }
}

pub fn append_ambient_context(prompt: &str, input: AmbientContextInput<'_>) -> String {
    let block = build_ambient_context(input);
    format!("{}\n\n{}", prompt.trim(), block.appendix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;

    #[test]
    fn ambient_block_includes_surface_and_timezone() {
        let block = build_ambient_context(AmbientContextInput {
            session_id: "medousa-frustrations",
            surface: Some(&TurnSurfaceContext::from_ingest(
                "telegram",
                "telegram:chat:1",
                "telegram:user:2",
            )),
            channel_policy: Some(&ChannelAmbientPolicy {
                proactive_allowed: Some(true),
                identity_channel_type: Some("tui".to_string()),
            }),
        });
        assert!(block.appendix.contains("[MEDOUSA_AMBIENT]"));
        assert!(block.appendix.contains("channel_surface=telegram"));
        assert!(block.appendix.contains("channel_tone=concise_mobile_friendly"));
        assert!(block.appendix.contains("timezone="));
        assert!(block.appendix.contains("proactive_messages=allowed"));
    }

    #[test]
    fn append_ambient_context_preserves_user_prompt() {
        let surface = TurnSurfaceContext::tui();
        let out = append_ambient_context(
            "hello operator",
            AmbientContextInput {
                session_id: "s1",
                surface: Some(&surface),
                channel_policy: None,
            },
        );
        assert!(out.starts_with("hello operator"));
        assert!(out.contains("channel_surface=tui"));
    }
}
