use anyhow::{Context, Result};

pub fn truncate_for_telegram(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

/// Escape model markdown for Telegram `MarkdownV2` delivery (preserves structure, escapes specials).
pub fn format_for_telegram_markdown_v2(text: &str) -> String {
    telegram_escape::tg_escape(&truncate_for_telegram(text))
}

pub fn parse_telegram_chat_id(channel_id: &str) -> Result<i64> {
    channel_id
        .strip_prefix("telegram:chat:")
        .or_else(|| channel_id.strip_prefix("telegram:"))
        .context("telegram channel_id must be telegram:chat:<id>")?
        .parse::<i64>()
        .context("telegram chat id must be numeric")
}

pub fn truncate_for_discord(text: &str) -> String {
    const MAX_CHARS: usize = 1900;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn truncate_for_slack(text: &str) -> String {
    const MAX_CHARS: usize = 3900;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn truncate_for_whatsapp(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn parse_discord_channel_id(channel_id: &str) -> Result<u64> {
    channel_id
        .strip_prefix("discord:channel:")
        .or_else(|| channel_id.strip_prefix("discord:"))
        .context("discord channel_id must be discord:channel:<id>")?
        .parse::<u64>()
        .context("discord channel id must be numeric")
}

pub fn parse_slack_channel_id(channel_id: &str) -> Result<String> {
    channel_id
        .strip_prefix("slack:channel:")
        .or_else(|| channel_id.strip_prefix("slack:"))
        .context("slack channel_id must be slack:channel:<id>")
        .map(str::to_string)
}

pub fn parse_whatsapp_chat_jid(channel_id: &str) -> Result<String> {
    channel_id
        .strip_prefix("whatsapp:chat:")
        .or_else(|| channel_id.strip_prefix("whatsapp:"))
        .context("whatsapp channel_id must be whatsapp:chat:<jid>")
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_telegram_chat_id_from_channel_id() {
        assert_eq!(
            parse_telegram_chat_id("telegram:chat:12345").unwrap(),
            12345
        );
    }

    #[test]
    fn truncate_for_telegram_caps_length() {
        let long = "x".repeat(5000);
        assert_eq!(truncate_for_telegram(&long).chars().count(), 4003);
    }

    #[test]
    fn telegram_markdown_v2_escapes_specials() {
        let escaped = format_for_telegram_markdown_v2("*hello* _world_");
        assert!(escaped.contains(r"\*hello\*"));
        assert!(escaped.contains(r"\_world\_"));
    }

    #[test]
    fn parse_slack_channel_id_from_channel_id() {
        assert_eq!(
            parse_slack_channel_id("slack:channel:C123").unwrap(),
            "C123"
        );
    }

    #[test]
    fn truncate_for_slack_caps_length() {
        let long = "x".repeat(5000);
        assert_eq!(truncate_for_slack(&long).chars().count(), 3903);
    }
}
