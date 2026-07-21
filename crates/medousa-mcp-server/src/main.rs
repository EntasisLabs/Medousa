//! Stdio MCP server entry — Medousa space for external agent runtimes.

use anyhow::Result;
use medousa_mcp_server::handle_jsonrpc;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> Result<()> {
    // MCP uses stdout for JSON-RPC; keep logs on stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        "medousa_mcp_server {} — space tools only (vault/calendar/artifacts)",
        env!("CARGO_PKG_VERSION")
    );

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(%err, "invalid JSON-RPC line");
                continue;
            }
        };
        if let Some(response) = handle_jsonrpc(&request) {
            let payload = serde_json::to_string(&response)?;
            stdout.write_all(payload.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
