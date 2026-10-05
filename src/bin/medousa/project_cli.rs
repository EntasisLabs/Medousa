//! Operator access to the daemon-owned Forge project lifecycle.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use medousa::daemon_api::resolve_daemon_url;
use reqwest::blocking::Response;
use serde_json::{Value, json};

use super::cli::{ProjectArgs, ProjectCommand};

pub fn run_project(args: ProjectArgs) -> Result<()> {
    let daemon_url = args.daemon_url.unwrap_or_else(|| resolve_daemon_url(None));
    let client = medousa::local_daemon_auth::blocking_client_with_timeout(
        &daemon_url,
        medousa_local_credential::CLI_LOCAL_NAME,
        Duration::from_secs(120),
    )?;
    match args.command {
        ProjectCommand::Create {
            title,
            brief,
            repo_path,
            base_ref,
        } => {
            let source = if repo_path.is_some() {
                "repository"
            } else {
                "blank"
            };
            let url = endpoint(&daemon_url, &["v1", "forge", "projects"])?;
            let response = client
                .post(url)
                .json(&json!({
                    "title": title,
                    "brief": brief,
                    "source": source,
                    "repo_path": repo_path,
                    "base_ref": base_ref,
                }))
                .send()
                .context(
                    "project creation outcome is uncertain; inspect project list before retrying",
                )?;
            let item = successful_json(response, "create project")?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&item)?);
            } else {
                print_project(&item)?;
            }
        }
        ProjectCommand::List { limit, cursor } => {
            if !(1..=500).contains(&limit) {
                bail!("--limit must be between 1 and 500");
            }
            let mut url = endpoint(&daemon_url, &["v1", "forge", "items"])?;
            url.query_pairs_mut()
                .append_pair("limit", &limit.to_string());
            if let Some(cursor) = cursor {
                url.query_pairs_mut().append_pair("cursor", &cursor);
            }
            let page = successful_json(client.get(url).send()?, "list projects")?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&page)?);
            } else {
                let items = page
                    .get("items")
                    .and_then(Value::as_array)
                    .context("invalid project list response")?;
                println!("WORK ID\tSTATE\tTITLE\tREPO ID");
                for item in items {
                    println!(
                        "{}\t{}\t{}\t{}",
                        field(item, "id")?,
                        field(item, "state")?,
                        field(item, "title")?,
                        project_repo_id(item).unwrap_or("-")
                    );
                }
                if let Some(next) = page.get("next_cursor").and_then(Value::as_str) {
                    eprintln!("next cursor: {next}");
                }
            }
        }
        ProjectCommand::Inspect { work_id } => {
            let url = endpoint(&daemon_url, &["v1", "forge", "items", &work_id])?;
            let item = successful_json(client.get(url).send()?, "inspect project")?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&item)?);
            } else {
                print_project(&item)?;
            }
        }
    }
    Ok(())
}

fn endpoint(base: &str, segments: &[&str]) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(base).context("invalid daemon URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

fn successful_json(response: Response, operation: &str) -> Result<Value> {
    let status = response.status();
    let body = response.text().context("read daemon response")?;
    if !status.is_success() {
        bail!("{operation} failed ({status}): {}", body.trim());
    }
    serde_json::from_str(&body).with_context(|| format!("decode {operation} response"))
}

fn field<'a>(item: &'a Value, name: &str) -> Result<&'a str> {
    item.get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("project response is missing {name}"))
}

fn project_repo_id(item: &Value) -> Option<&str> {
    item.pointer("/environment/repo/repo_id")
        .and_then(Value::as_str)
}

fn print_project(item: &Value) -> Result<()> {
    println!("work_id: {}", field(item, "id")?);
    println!("title: {}", field(item, "title")?);
    println!("state: {}", field(item, "state")?);
    println!("repo_id: {}", project_repo_id(item).unwrap_or("-"));
    println!(
        "repo_path: {}",
        item.pointer("/target/repo_path")
            .and_then(Value::as_str)
            .unwrap_or("-")
    );
    println!(
        "worktree: {}",
        item.pointer("/environment/worktree")
            .and_then(Value::as_str)
            .unwrap_or("-")
    );
    println!("brief: {}", field(item, "brief")?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_identity_uses_the_raw_forge_repo_id_for_bot_enrollment() {
        let item = json!({
            "id": "work-1",
            "environment": {"repo": {"repo_id": "repo-1"}},
        });
        assert_eq!(project_repo_id(&item), Some("repo-1"));
    }

    #[test]
    fn work_id_is_encoded_as_one_path_segment() {
        let url = endpoint("http://127.0.0.1:7419", &["v1", "forge", "items", "a/b"]).expect("URL");
        assert_eq!(url.as_str(), "http://127.0.0.1:7419/v1/forge/items/a%2Fb");
    }
}
