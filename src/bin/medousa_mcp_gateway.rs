use std::env;

use medousa::mcp_gateway::{McpGatewayConfig, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let config = McpGatewayConfig::from_env_and_args(&args);
    serve(config).await
}

fn print_help() {
    println!("medousa-mcp-gateway — MCP Client gateway for Medousa");
    println!();
    println!("Usage:");
    println!("  medousa_mcp_gateway [--bind <addr>] [--invokes-disabled]");
    println!();
    println!("Environment:");
    println!("  MEDOUSA_MCP_GATEWAY_BIND   default 127.0.0.1:7420");
    println!("  MEDOUSA_MCP_GATEWAY_TOKEN  bearer token for daemon → gateway calls");
    println!("  MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN  bearer token for admin kill switch");
}
