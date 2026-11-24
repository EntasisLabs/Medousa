//! Channels: Telegram, Discord, Slack and WhatsApp, after medousa-home's
//! Messaging settings. Tokens go to the engine's integration store; the
//! rest of a channel's settings and its adapter live on the machine the
//! workshop runs on, so they're set up here only for a workshop on this Mac.
use medousa_desktop_host::channels::{Save, Status, Summary};
use std::collections::HashMap;

/// One channel as medousa-home describes it.
pub(crate) struct Meta {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tagline: &'static str,
    pub blurb: &'static str,
    pub guide: Option<(&'static str, &'static str)>,
    /// Token fields: draft key, engine slot, title, placeholder.
    pub tokens: &'static [Token],
}

pub(crate) struct Token {
    pub key: &'static str,
    pub slot: &'static str,
    pub title: &'static str,
    pub placeholder: &'static str,
}

pub(crate) const CHANNELS: [Meta; 4] = [
    Meta {
        id: "telegram",
        name: "Telegram",
        description: "DMs, groups, and topics",
        tagline: "Run Medousa from Telegram DMs, groups, and topics.",
        blurb: "Create a bot with BotFather, copy the token, and paste it below. Restrict who can reach Medousa with allowed user IDs.",
        guide: Some((
            "https://core.telegram.org/bots#6-botfather",
            "Open Telegram BotFather guide",
        )),
        tokens: &[Token {
            key: "telegram.token",
            slot: "bot_token",
            title: "Bot token",
            placeholder: "123456:ABC-DEF…",
        }],
    },
    Meta {
        id: "discord",
        name: "Discord",
        description: "Prefix commands in servers",
        tagline: "Invite Medousa to Discord servers and reach her with prefix commands.",
        blurb: "Create a Discord application, add a bot, and paste the bot token. Set a command prefix your server will recognize.",
        guide: Some((
            "https://discord.com/developers/applications",
            "Open Discord Developer Portal",
        )),
        tokens: &[Token {
            key: "discord.token",
            slot: "bot_token",
            title: "Bot token",
            placeholder: "Paste the bot token",
        }],
    },
    Meta {
        id: "slack",
        name: "Slack",
        description: "Socket Mode workspace bot",
        tagline: "Connect Medousa to Slack with Socket Mode — bot and app tokens required.",
        blurb: "Create a Slack app with Socket Mode enabled. You need both a bot token (xoxb) and an app-level token (xapp).",
        guide: Some(("https://api.slack.com/apps", "Open Slack app dashboard")),
        tokens: &[
            Token {
                key: "slack.bot",
                slot: "bot_token",
                title: "Bot token",
                placeholder: "xoxb-…",
            },
            Token {
                key: "slack.app",
                slot: "app_token",
                title: "App token",
                placeholder: "xapp-…",
            },
        ],
    },
    Meta {
        id: "whatsapp",
        name: "WhatsApp",
        description: "Bridge deliver bind",
        tagline: "Reach Medousa through a WhatsApp bridge — no bot token, bind address and allowed JIDs.",
        blurb: "Point Medousa at your bridge deliver bind and list the JIDs allowed to message the workshop.",
        guide: None,
        tokens: &[],
    },
];

/// The disclosure key a channel's form opens under.
pub(crate) fn open_key(channel: &str) -> &'static str {
    match channel {
        "telegram" => "ch.telegram",
        "discord" => "ch.discord",
        "slack" => "ch.slack",
        _ => "ch.whatsapp",
    }
}

/// The key a channel's Advanced group opens under.
pub(crate) fn advanced_key(channel: &str) -> &'static str {
    match channel {
        "telegram" => "ch.telegram.advanced",
        "discord" => "ch.discord.advanced",
        "slack" => "ch.slack.advanced",
        _ => "ch.whatsapp.advanced",
    }
}

/// medousa-home's `statusLabel`.
pub(crate) fn status_label(status: Status) -> &'static str {
    match status {
        Status::NeedsSetup => "Needs setup",
        Status::Ready => "Ready",
        Status::Connected => "Connected",
    }
}

pub(crate) fn ready(summary: &Summary) -> usize {
    CHANNELS
        .iter()
        .filter(|c| summary.status(c.id) != Status::NeedsSetup)
        .count()
}

/// Whether a token is already kept for this field.
pub(crate) fn token_set(summary: &Summary, key: &str) -> bool {
    match key {
        "telegram.token" => summary.telegram.credentials_set,
        "discord.token" => summary.discord.credentials_set,
        "slack.bot" => summary.slack.bot_token_set,
        "slack.app" => summary.slack.app_token_set,
        _ => false,
    }
}

fn csv<T: ToString>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The form's text fields and switch as saved, keyed as the view drafts
/// them. Tokens start empty: a kept token is never read back.
pub(crate) fn fill(summary: &Summary, channel: &str) -> (Vec<(&'static str, String)>, bool) {
    match channel {
        "telegram" => {
            let t = &summary.telegram;
            (
                vec![
                    ("telegram.users", csv(&t.allowed_user_ids)),
                    ("telegram.chats", csv(&t.heartbeat_chat_ids)),
                    ("telegram.token", String::new()),
                ],
                t.heartbeat_nudges_enabled,
            )
        }
        "discord" => {
            let d = &summary.discord;
            (
                vec![
                    ("discord.prefix", d.command_prefix.clone()),
                    ("discord.channels", csv(&d.heartbeat_channel_ids)),
                    ("discord.token", String::new()),
                ],
                d.heartbeat_nudges_enabled,
            )
        }
        "slack" => {
            let s = &summary.slack;
            (
                vec![
                    ("slack.users", s.allowed_user_ids.join(", ")),
                    ("slack.channels", s.heartbeat_channel_ids.join(", ")),
                    ("slack.bot", String::new()),
                    ("slack.app", String::new()),
                ],
                s.heartbeat_nudges_enabled,
            )
        }
        _ => {
            let w = &summary.whatsapp;
            (
                vec![
                    ("whatsapp.bind", w.deliver_bind.clone()),
                    ("whatsapp.url", w.deliver_url.clone().unwrap_or_default()),
                    ("whatsapp.db", w.session_db_path.clone().unwrap_or_default()),
                    ("whatsapp.users", w.allowed_user_ids.join(", ")),
                    ("whatsapp.jids", w.heartbeat_chat_jids.join(", ")),
                ],
                w.heartbeat_nudges_enabled,
            )
        }
    }
}

fn strings(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Numbers from a comma list; anything else is named rather than dropped.
fn numbers<T: std::str::FromStr>(raw: &str, what: &str) -> Result<Vec<T>, String> {
    strings(raw)
        .into_iter()
        .map(|t| {
            t.parse()
                .map_err(|_| format!("“{t}” isn’t a number — {what} are numeric IDs."))
        })
        .collect()
}

/// A token change: the engine slot and the new value, or `None` to forget it.
pub(crate) type TokenChange = (&'static str, Option<String>);

/// What one Save does: the channel's settings and its token changes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    pub kind: &'static str,
    pub save: Save,
    pub tokens: Vec<TokenChange>,
}

/// Builds a save from the form, as medousa-home's `saveSelected` does.
/// A typed token replaces the kept one; a token marked to forget is
/// cleared when none is typed.
pub(crate) fn plan(
    channel: &'static str,
    drafts: &HashMap<&'static str, String>,
    heartbeat: bool,
    forget: &std::collections::HashSet<&'static str>,
) -> Result<Plan, String> {
    let d = |k: &str| drafts.get(k).map(String::as_str).unwrap_or_default();
    let save = match channel {
        "telegram" => Save::Telegram {
            allowed_user_ids: numbers(d("telegram.users"), "Telegram user IDs")?,
            heartbeat_nudges_enabled: heartbeat,
            heartbeat_chat_ids: numbers(d("telegram.chats"), "Telegram chat IDs")?,
        },
        "discord" => Save::Discord {
            command_prefix: match d("discord.prefix").trim() {
                "" => "!".into(),
                p => p.into(),
            },
            heartbeat_nudges_enabled: heartbeat,
            heartbeat_channel_ids: numbers(d("discord.channels"), "Discord channel IDs")?,
        },
        "slack" => Save::Slack {
            allowed_user_ids: strings(d("slack.users")),
            heartbeat_nudges_enabled: heartbeat,
            heartbeat_channel_ids: strings(d("slack.channels")),
        },
        _ => {
            let optional = |k: &str| Some(d(k).trim().to_string()).filter(|v| !v.is_empty());
            Save::WhatsApp {
                deliver_bind: match d("whatsapp.bind").trim() {
                    "" => "127.0.0.1:7423".into(),
                    b => b.into(),
                },
                deliver_url: optional("whatsapp.url"),
                session_db_path: optional("whatsapp.db"),
                allowed_user_ids: strings(d("whatsapp.users")),
                heartbeat_nudges_enabled: heartbeat,
                heartbeat_chat_jids: strings(d("whatsapp.jids")),
            }
        }
    };
    let meta = CHANNELS.iter().find(|c| c.id == channel).unwrap();
    let tokens = meta
        .tokens
        .iter()
        .filter_map(|t| match d(t.key).trim() {
            "" if forget.contains(t.key) => Some((t.slot, None)),
            "" => None,
            v => Some((t.slot, Some(v.to_string()))),
        })
        .collect();
    Ok(Plan {
        kind: meta.id,
        save,
        tokens,
    })
}

/// The draft keys a channel's form owns.
pub(crate) fn keys(channel: &str) -> Vec<&'static str> {
    fill(&Summary::default(), channel)
        .0
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn a_form_round_trips_into_a_save_with_its_token_changes() {
        let mut summary = Summary::default();
        summary.telegram.allowed_user_ids = vec![7, 42];
        summary.telegram.heartbeat_nudges_enabled = true;
        let (fields, heartbeat) = fill(&summary, "telegram");
        let mut drafts: HashMap<&'static str, String> = fields.into_iter().collect();
        assert_eq!(drafts["telegram.users"], "7, 42");
        assert_eq!(drafts["telegram.token"], "", "a kept token is never shown");

        let none = HashSet::new();
        let p = plan("telegram", &drafts, heartbeat, &none).unwrap();
        assert!(p.tokens.is_empty(), "an empty token field keeps the token");
        assert_eq!(
            p.save,
            Save::Telegram {
                allowed_user_ids: vec![7, 42],
                heartbeat_nudges_enabled: true,
                heartbeat_chat_ids: vec![],
            }
        );

        drafts.insert("telegram.token", " 123:abc ".into());
        let p = plan("telegram", &drafts, false, &none).unwrap();
        assert_eq!(p.tokens, [("bot_token", Some("123:abc".into()))]);

        drafts.insert("telegram.token", String::new());
        let forget: HashSet<_> = ["telegram.token"].into();
        assert_eq!(
            plan("telegram", &drafts, false, &forget).unwrap().tokens,
            [("bot_token", None)]
        );

        drafts.insert("telegram.users", "7, me".into());
        let e = plan("telegram", &drafts, false, &none).unwrap_err();
        assert!(e.contains("“me”"), "{e}");
    }

    #[test]
    fn empty_prefix_and_bind_fall_back_like_medousa_home() {
        let none = HashSet::new();
        let drafts = HashMap::new();
        assert!(matches!(
            plan("discord", &drafts, false, &none).unwrap().save,
            Save::Discord { command_prefix, .. } if command_prefix == "!"
        ));
        match plan("whatsapp", &drafts, false, &none).unwrap().save {
            Save::WhatsApp {
                deliver_bind,
                deliver_url,
                ..
            } => {
                assert_eq!(deliver_bind, "127.0.0.1:7423");
                assert_eq!(deliver_url, None);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(keys("slack").len(), 4);
        assert_eq!(ready(&Summary::default()), 0);
    }
}
