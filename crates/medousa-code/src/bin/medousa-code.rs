//! medousa-code — LSP Interoperability Orchestrator binary.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use medousa_code::registry::ServerRegistry;
use medousa_code::server::{serve, OrchestratorConfig, OrchestratorState};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "medousa-code", about = "Medousa LSP Interoperability Orchestrator")]
struct Args {
    /// Listen address (default 127.0.0.1:7861).
    #[arg(long, default_value = "127.0.0.1:7861")]
    bind: SocketAddr,

    /// Workspace root (scripts library or Forge worktree).
    #[arg(long)]
    workspace: PathBuf,

    /// Extra allowed roots (repeatable), e.g. Forge worktree paths.
    #[arg(long = "allow-root")]
    allow_roots: Vec<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let workspace = args
        .workspace
        .canonicalize()
        .unwrap_or(args.workspace.clone());
    std::fs::create_dir_all(&workspace)?;

    let mut allowed = args.allow_roots;
    if allowed.is_empty() {
        allowed.push(workspace.clone());
    }

    let config = OrchestratorConfig {
        bind: args.bind,
        workspace_root: workspace,
        allowed_roots: allowed,
    };
    let state = OrchestratorState::new(config, ServerRegistry::with_defaults());
    serve(state).await
}
