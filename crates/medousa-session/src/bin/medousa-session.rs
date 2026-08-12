//! medousa-session — workshop shell session host binary.

// Hide the console on Windows release builds when launched as a workshop
// sidecar (daemon also passes CREATE_NO_WINDOW at spawn).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use medousa_session::server::{SessionHostConfig, SessionHostState, serve};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "medousa-session",
    about = "Medousa workshop shell session host"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:7862")]
    bind: SocketAddr,

    /// Workspace root (scripts library or Forge worktree).
    #[arg(long)]
    workspace: PathBuf,

    /// Extra allowed cwd roots (repeatable), e.g. Forge worktree paths.
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

    let allowed = args
        .allow_roots
        .into_iter()
        .map(|root| root.canonicalize().unwrap_or(root))
        .collect();

    let config = SessionHostConfig {
        bind: args.bind,
        workspace_root: workspace,
        allowed_roots: allowed,
    };
    serve(SessionHostState::new(config)).await
}
