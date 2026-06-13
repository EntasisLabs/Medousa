import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import type {
  McpGatewayConfigLoadResult,
  McpGatewayRestartResult,
  McpGatewayStatusResult,
  McpGatewayTestResult,
  McpServerMutationResult,
  McpServerUpsertRequest,
} from "$lib/types/mcpGateway";

export type { McpServerUpsertRequest };

export async function loadMcpGatewayConfig(): Promise<McpGatewayConfigLoadResult> {
  if (!isTauri()) {
    return {
      path: "~/.config/medousa/mcp-gateway.toml",
      fileExists: false,
      config: { gateway: { bind: "127.0.0.1:7420", daemonPolicyUrl: "", maxInvokeDurationMs: 30000, catalogRefreshIntervalSecs: 300, useMockFallback: true }, servers: [] },
    };
  }
  return invoke<McpGatewayConfigLoadResult>("mcp_gateway_load_config");
}

export async function fetchMcpGatewayStatus(): Promise<McpGatewayStatusResult> {
  if (!isTauri()) {
    return {
      gatewayUrl: "http://127.0.0.1:7420",
      reachable: false,
      message: "MCP gateway management requires the Medousa desktop app",
      health: null,
      servers: [],
      configPath: "",
    };
  }
  return invoke<McpGatewayStatusResult>("mcp_gateway_status");
}

export async function restartMcpGateway(): Promise<McpGatewayRestartResult> {
  if (!isTauri()) {
    return {
      started: false,
      alreadyRunning: false,
      logPath: "",
      message: "Unavailable in browser dev mode",
    };
  }
  return invoke<McpGatewayRestartResult>("mcp_gateway_restart");
}

export async function upsertMcpServer(
  request: McpServerUpsertRequest,
): Promise<McpServerMutationResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", configPath: "" };
  }
  return invoke<McpServerMutationResult>("mcp_gateway_upsert_server", { request });
}

export async function removeMcpServer(serverId: string): Promise<McpServerMutationResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", configPath: "" };
  }
  return invoke<McpServerMutationResult>("mcp_gateway_remove_server", { serverId });
}

export async function setMcpServerEnabled(
  serverId: string,
  enabled: boolean,
): Promise<McpServerMutationResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", configPath: "" };
  }
  return invoke<McpServerMutationResult>("mcp_gateway_set_server_enabled", {
    serverId,
    enabled,
  });
}

export async function applyMcpServer(
  request: McpServerUpsertRequest,
): Promise<McpGatewayTestResult> {
  if (!isTauri()) {
    return {
      ok: false,
      message: "Unavailable in browser dev mode",
      connected: false,
      toolCount: 0,
    };
  }
  return invoke<McpGatewayTestResult>("mcp_gateway_apply_server", { request });
}

// Alias for clarity in UI
export type McpGatewayApplyServerRequest = McpServerUpsertRequest;
