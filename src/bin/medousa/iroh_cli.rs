use anyhow::{Result, bail};

#[cfg(feature = "iroh-transport")]
use medousa::iroh_transport::{fetch_http_path, spawn_workshop_gateway};

pub fn run_iroh(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_iroh_help();
        return Ok(());
    }

    #[cfg(not(feature = "iroh-transport"))]
    {
        let _ = args;
        bail!("iroh transport is disabled — rebuild medousa with --features iroh-transport");
    }

    #[cfg(feature = "iroh-transport")]
    match args.first().map(String::as_str) {
        None | Some("help") => {
            print_iroh_help();
            Ok(())
        }
        Some("workshop") | Some("serve") => run_workshop(args),
        Some("curl") | Some("get") => run_curl(args),
        Some("ticket") => run_ticket(args),
        Some(other) => bail!(
            "unknown iroh subcommand '{other}'. run 'medousa iroh --help' for usage"
        ),
    }
}

#[cfg(feature = "iroh-transport")]
fn run_workshop(args: &[String]) -> Result<()> {
    let upstream = resolve_upstream(args).unwrap_or_else(|| "http://127.0.0.1:7419".to_string());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(async {
        let gateway = spawn_workshop_gateway(&upstream).await?;
        let info = gateway.info();
        println!("Medousa Iroh workshop gateway");
        println!("Upstream: {upstream}");
        println!("ALPN: medousa-http/1");
        println!("Endpoint ID: {}", info.endpoint_id);
        println!();
        println!("Ticket (share with phone / curl client):");
        println!("{}", info.ticket);
        println!();
        println!("Example: medousa iroh curl '{}' /health", info.ticket);
        println!("Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
        gateway.shutdown().await?;
        Ok(())
    })
}

#[cfg(feature = "iroh-transport")]
fn run_ticket(args: &[String]) -> Result<()> {
    run_workshop_once(args, true)
}

#[cfg(feature = "iroh-transport")]
fn run_curl(args: &[String]) -> Result<()> {
    let ticket = args
        .get(1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .context("usage: medousa iroh curl <ticket> <path>  (e.g. /health)")?;
    let path = args
        .get(2)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .unwrap_or("/health");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(async {
        let response = fetch_http_path(ticket, path).await?;
        print!("{response}");
        Ok(())
    })
}

#[cfg(feature = "iroh-transport")]
fn run_workshop_once(args: &[String], exit_after_ticket: bool) -> Result<()> {
    let upstream = resolve_upstream(args).unwrap_or_else(|| "http://127.0.0.1:7419".to_string());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(async {
        let gateway = spawn_workshop_gateway(&upstream).await?;
        println!("{}", gateway.info().ticket);
        if exit_after_ticket {
            gateway.shutdown().await?;
        } else {
            tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
            gateway.shutdown().await?;
        }
        Ok(())
    })
}

#[cfg(feature = "iroh-transport")]
fn resolve_upstream(args: &[String]) -> Option<String> {
    args.iter()
        .position(|arg| arg == "--upstream")
        .and_then(|index| args.get(index + 1))
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn print_iroh_help() {
    println!("Medousa Iroh transport (Phase 0 spike)");
    println!();
    println!("USAGE:");
    println!("  medousa iroh workshop [--upstream http://127.0.0.1:7419]");
    println!("  medousa iroh curl <ticket> [/health]");
    println!("  medousa iroh ticket [--upstream <url>]   # print ticket and exit");
    println!();
    println!("Requires build with --features iroh-transport");
    println!("Iroh gateway is on by default when built with iroh-transport (opt out: MEDOUSA_IROH=0)");
}
