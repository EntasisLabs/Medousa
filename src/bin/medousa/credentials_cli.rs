use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use medousa::daemon_api::{DEFAULT_DAEMON_URL, resolve_daemon_url};

pub fn run_credentials(args: &[String]) -> Result<()> {
    let daemon_url = find_arg_value(args, "--daemon-url")
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(|| resolve_daemon_url(None));
    let client = medousa::local_daemon_auth::blocking_client_with_timeout(
        &daemon_url,
        medousa_local_credential::CLI_LOCAL_NAME,
        Duration::from_secs(10),
    )?;
    match args.first().map(String::as_str) {
        None | Some("list") | Some("status") => {
            let value = client
                .get(format!("{daemon_url}/v1/admin/local-credentials"))
                .send()
                .context("query local credential diagnostics")?
                .error_for_status()?
                .json::<Value>()?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Some("rotate") => {
            let name = args
                .get(1)
                .context("usage: medousa credentials rotate home-local|medousa-cli|medousa-tui")?;
            let value = client
                .post(format!(
                    "{daemon_url}/v1/admin/local-credentials/{name}/rotate"
                ))
                .send()
                .context("rotate local credential")?
                .error_for_status()?
                .json::<Value>()?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            eprintln!("Restart the rotated client so it reloads its credential.");
            Ok(())
        }
        Some("revoke") => {
            let name = args
                .get(1)
                .context("usage: medousa credentials revoke home-local|medousa-cli|medousa-tui")?;
            if name == medousa_local_credential::CLI_LOCAL_NAME {
                bail!(
                    "refusing to revoke the credential authenticating this command; rotate it instead"
                );
            }
            let value = client
                .delete(format!("{daemon_url}/v1/admin/local-credentials/{name}"))
                .send()
                .context("revoke local credential")?
                .error_for_status()?
                .json::<Value>()?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Some("help" | "--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some(other) => bail!("unknown credentials subcommand '{other}'"),
    }
}

fn print_help() {
    println!("USAGE:");
    println!("  medousa credentials list [--daemon-url <url>]");
    println!("  medousa credentials rotate <name> [--daemon-url <url>]");
    println!("  medousa credentials revoke <name> [--daemon-url <url>]");
    println!();
    println!("Names: home-local, medousa-cli, medousa-tui");
    println!("Default daemon: {DEFAULT_DAEMON_URL}");
}

fn find_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
