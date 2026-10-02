//! Operator registration for external-agent Bots.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use medousa::daemon_api::resolve_daemon_url;
use serde_json::{Value, json};

use super::cli::{BotArgs, BotCommand};

pub fn run_bot(args: BotArgs) -> Result<()> {
    let daemon_url = args.daemon_url.unwrap_or_else(|| resolve_daemon_url(None));
    let client = medousa::local_daemon_auth::blocking_client_with_timeout(
        &daemon_url,
        medousa_local_credential::CLI_LOCAL_NAME,
        Duration::from_secs(10),
    )?;
    let response = client
        .get(format!("{daemon_url}/v1/bots"))
        .send()
        .context("list Bots")?;
    let status = response.status();
    let body = response.text().context("read Bot list")?;
    if !status.is_success() {
        bail!("Bot list failed ({status}): {body}");
    }
    let list: Value = serde_json::from_str(&body).context("decode Bot list")?;
    let bots = list
        .get("bots")
        .and_then(Value::as_array)
        .context("invalid Bot list")?;
    match args.command {
        BotCommand::List => {
            for bot in bots {
                println!(
                    "{}\t{}\t{}",
                    field(bot, "bot_id")?,
                    field(bot, "display_name")?,
                    bot.pointer("/external_agent/home_workshop_id")
                        .and_then(Value::as_str)
                        .unwrap_or("-")
                );
            }
            Ok(())
        }
        BotCommand::Enroll {
            bot,
            runtime,
            workshop_id,
            work_id,
            repo_id,
        } => {
            let candidates = bots
                .iter()
                .filter(|entry| {
                    field(entry, "bot_id").ok() == Some(bot.as_str())
                        || field(entry, "display_name")
                            .ok()
                            .is_some_and(|name| name.eq_ignore_ascii_case(&bot))
                })
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                bail!(
                    "Bot selector matched {} profiles; use the exact Bot ID from medousa bot list",
                    candidates.len()
                );
            }
            let existing = candidates[0];
            let bot_id = field(existing, "bot_id")?;
            let mut update = existing.clone();
            let object = update.as_object_mut().context("invalid Bot profile")?;
            object.insert(
                "expected_revision".into(),
                existing
                    .get("revision")
                    .cloned()
                    .context("Bot revision missing")?,
            );
            object.insert(
                "external_agent".into(),
                json!({
                    "runtime": runtime,
                    "home_workshop_id": workshop_id,
                    "forge_work_id": work_id,
                    "forge_repo_id": repo_id,
                    "session_contract": "fresh_per_job",
                }),
            );
            let response = client
                .put(format!("{daemon_url}/v1/bots/{bot_id}"))
                .json(&update)
                .send()
                .context("enroll Bot")?;
            let status = response.status();
            let body = response.text().context("read Bot enrollment response")?;
            if !status.is_success() {
                bail!("Bot enrollment failed ({status}): {body}");
            }
            let body: Value =
                serde_json::from_str(&body).context("decode Bot enrollment response")?;
            println!(
                "enrolled {} ({bot_id}) on {}",
                field(&body, "display_name")?,
                workshop_id
            );
            Ok(())
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Bot {key} missing"))
}
