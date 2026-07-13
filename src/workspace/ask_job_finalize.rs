//! Post-completion actions for persisted ask jobs — journal + channel notify.

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::channel_delivery::{self, ChannelDeliveryTarget};
use crate::load_product_config;
use crate::vault::store::vault_store;
use crate::workspace::ask_job_store::{AskJobRecord, ask_job_store};

pub struct AskJobCompleteActions {
    pub write_journal_path: Option<String>,
    pub notify_channel: Option<String>,
}

pub struct AskJobCompleteActionsResult {
    pub journal_path: Option<String>,
    pub notified_channel: Option<String>,
    pub message: String,
}

pub async fn apply_ask_job_complete_actions(
    job_id: &str,
    actions: AskJobCompleteActions,
    dispatch_client: &reqwest::Client,
) -> Result<AskJobCompleteActionsResult> {
    let record = ask_job_store()
        .get(job_id)
        .with_context(|| format!("ask job not found: {job_id}"))?;

    if record.status != crate::workspace::ask_job_store::AskJobStatus::Succeeded {
        bail!("ask job is not in succeeded state");
    }

    let output = record
        .output_text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("(no output captured)");

    let mut journal_path = None;
    if let Some(path) = actions
        .write_journal_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        journal_path = Some(write_ask_result_to_journal(path, &record, output)?);
        ask_job_store().set_journal_path(job_id, journal_path.clone().unwrap());
    }

    let mut notified_channel = None;
    if let Some(channel) = actions
        .notify_channel
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let text = compose_notify_text(&record, output);
        if let Some(target) = resolve_notify_target(channel) {
            channel_delivery::dispatch_channel_message(dispatch_client, &target, &text)
                .await
                .with_context(|| format!("notify via {channel}"))?;
            notified_channel = Some(channel.to_string());
            ask_job_store().set_notified_channel(job_id, channel.to_string());
        }
    }

    let mut parts = Vec::new();
    if journal_path.is_some() {
        parts.push("journal updated".to_string());
    }
    if notified_channel.is_some() {
        parts.push(format!("notified via {}", notified_channel.as_deref().unwrap()));
    }
    let message = if parts.is_empty() {
        "no completion actions applied".to_string()
    } else {
        parts.join(" · ")
    };

    Ok(AskJobCompleteActionsResult {
        journal_path,
        notified_channel,
        message,
    })
}

pub fn compose_ask_journal_body(record: &AskJobRecord, output: &str) -> String {
    let header = format!(
        "\n\n---\n## Ask · {}\n**Finished:** {}\n\n**Prompt:** {}\n\n",
        record.job_id,
        record.finished_at_utc.unwrap_or_else(Utc::now),
        record.prompt.trim()
    );
    let interim = record
        .interim_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let result = output.trim();

    let mut sections = Vec::new();
    if let Some(follow_up) = interim
        && !texts_equivalent(follow_up, result) {
            sections.push(format!("**Follow-up:**\n{follow_up}\n"));
        }
    if result.is_empty() {
        sections.push("**Result:**\n(no output captured)\n".to_string());
    } else if sections.is_empty() {
        sections.push(format!("{result}\n"));
    } else {
        sections.push(format!("**Result:**\n{result}\n"));
    }

    format!("{header}{}", sections.join("\n"))
}

fn compose_notify_text(record: &AskJobRecord, output: &str) -> String {
    let interim = record
        .interim_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut body = String::new();
    if let Some(follow_up) = interim
        && !texts_equivalent(follow_up, output) {
            body.push_str(&format!(
                "Follow-up:\n{}\n\n",
                truncate(follow_up, 600)
            ));
        }
    body.push_str(&truncate(output, 1200));
    format!(
        "✓ Ask completed · {}\n\n{}\n\n— {}",
        truncate(&record.prompt, 120),
        body.trim(),
        record.job_id
    )
}

fn texts_equivalent(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn write_ask_result_to_journal(path: &str, record: &AskJobRecord, output: &str) -> Result<String> {
    let normalized = path.trim().trim_start_matches('/');
    if normalized.is_empty() || normalized.contains("..") {
        bail!("invalid journal path");
    }

    let body = compose_ask_journal_body(record, output);
    let store = vault_store();
    if store.get_entry(normalized).is_some() {
        let existing = store.read_content(normalized)?;
        let updated = format!("{existing}{body}");
        store.write_content(normalized, &updated, None)?;
    } else {
        store.write_content(normalized, body.trim_start(), None)?;
    }
    Ok(normalized.to_string())
}

pub fn default_journal_path_for_today() -> String {
    Utc::now().format("journal/%Y-%m-%d.md").to_string()
}

fn heartbeat_delivery_target(channel: &str, channel_id: &str) -> ChannelDeliveryTarget {
    ChannelDeliveryTarget {
        channel: channel.to_string(),
        user_id: "medousa:system:ask-complete".to_string(),
        channel_id: channel_id.to_string(),
        session_id: "medousa-ask-complete".to_string(),
        stream_id: None,
    }
}

fn resolve_notify_target(channel: &str) -> Option<ChannelDeliveryTarget> {
    let config = load_product_config();
    let normalized = channel.to_ascii_lowercase();
    match normalized.as_str() {
        "telegram" => config.telegram.heartbeat_chat_ids.first().map(|chat_id| {
            heartbeat_delivery_target(
                "telegram",
                &format!("telegram:chat:{chat_id}"),
            )
        }),
        "discord" => config.discord.heartbeat_channel_ids.first().map(|channel_id| {
            heartbeat_delivery_target(
                "discord",
                &format!("discord:channel:{channel_id}"),
            )
        }),
        "slack" => config.slack.heartbeat_channel_ids.first().map(|channel_id| {
            heartbeat_delivery_target(
                "slack",
                &format!("slack:channel:{channel_id}"),
            )
        }),
        "whatsapp" => config.whatsapp.heartbeat_chat_jids.first().map(|chat_jid| {
            heartbeat_delivery_target(
                "whatsapp",
                &format!("whatsapp:chat:{chat_jid}"),
            )
        }),
        _ => None,
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::ask_job_store::AskJobStatus;

    fn sample_record() -> AskJobRecord {
        AskJobRecord {
            job_id: "medousa-daemon-ask-1".to_string(),
            prompt: "Research OpenClaw trends".to_string(),
            status: AskJobStatus::Succeeded,
            output_text: Some("Here is the full synthesized report.".to_string()),
            interim_text: Some(
                "On it — researching OpenClaw trends, give me a moment.".to_string(),
            ),
            error: None,
            session_id: "sess-1".to_string(),
            manuscript_id: None,
            additional_manuscript_ids: None,
            suggested_capability_ids: None,
            model_hint: None,
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            finished_at_utc: Some(Utc::now()),
            archived: false,
            journal_path: None,
            notified_channel: None,
        }
    }

    #[test]
    fn journal_body_includes_follow_up_and_result() {
        let body = compose_ask_journal_body(
            &sample_record(),
            "Here is the full synthesized report.",
        );
        assert!(body.contains("**Follow-up:**"));
        assert!(body.contains("give me a moment"));
        assert!(body.contains("**Result:**"));
        assert!(body.contains("full synthesized report"));
    }

    #[test]
    fn journal_body_omits_duplicate_follow_up_when_same_as_result() {
        let mut record = sample_record();
        record.interim_text = Some("Same answer".to_string());
        let body = compose_ask_journal_body(&record, "Same answer");
        assert!(!body.contains("**Follow-up:**"));
        assert!(body.contains("Same answer"));
    }
}
