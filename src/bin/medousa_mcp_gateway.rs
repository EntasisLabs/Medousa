use std::env;

use medousa::mcp_gateway::McpGatewayFullConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let config = McpGatewayFullConfig::from_env_and_args(&args);
    medousa::mcp_gateway::serve(config).await
}

fn print_help() {
    println!("medousa-mcp-gateway — MCP Client gateway for Medousa");
    println!();
    println!("Usage:");
    println!("  medousa_mcp_gateway [--bind <addr>] [--invokes-disabled]");
    println!();
    println!("Config file:");
    println!("  ~/.config/medousa/mcp-gateway.toml");
    println!();
    println!("Environment:");
    println!("  MEDOUSA_MCP_GATEWAY_BIND           default 127.0.0.1:7420");
    println!("  MEDOUSA_MCP_GATEWAY_TOKEN          bearer token for daemon → gateway calls");
    println!("  MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN    bearer token for admin routes");
    println!("  MEDOUSA_MCP_POLICY_TOKEN           bearer token for gateway → daemon policy");
    println!("  MEDOUSA_MCP_TURN_TOKEN_SECRET      HMAC secret for turn-scoped invoke tokens");
}
