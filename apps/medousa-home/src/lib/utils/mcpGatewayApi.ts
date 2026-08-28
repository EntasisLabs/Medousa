import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import type {
  CompleteMcpOAuthResult,
  McpGatewayConfigLoadResult,
  McpGatewayRestartResult,
  McpGatewayStatusResult,
  McpGatewayTestResult,
  McpOAuthStatus,
  McpServerMutationResult,
  McpServerUpsertRequest,
} from "$lib/types/mcpGateway";

interface McpOAuthStatusWire {
  server_id: string;
  status: string;
  connected: boolean;
  issuer?: string | null;
  scopes?: string[];
}

interface CompleteMcpOAuthWire {
  connection: McpOAuthStatusWire;
}

function normalizeMcpOAuthStatus(status: McpOAuthStatusWire): McpOAuthStatus {
  return {
    serverId: status.server_id,
    status: status.status,
    connected: status.connected,
    issuer: status.issuer ?? null,
    scopes: status.scopes ?? [],
  };
}

export type { McpServerUpsertRequest };

interface McpGatewayConfigWire {
  path: string;
  fileExists: boolean;
  config: {
    gateway: {
      bind: string;
      daemon_policy_url: string;
      max_invoke_duration_ms: number;
      catalog_refresh_interval_secs: number;
      use_mock_fallback: boolean;
    };
    servers: Array<{
      id: string;
      title: string;
      enabled: boolean;
      transport: string;
      command?: string | null;
      args?: string[];
      url?: string | null;
      bearer_token?: string | null;
      allowed_lanes?: string[];
      allowed_effect_classes?: string[];
      use_mock?: boolean;
    }>;
  };
}

export async function loadMcpGatewayConfig(): Promise<McpGatewayConfigLoadResult> {
  if (!isTauri()) {
    return {
      path: "~/.config/medousa/mcp-gateway.toml",
      fileExists: false,
      config: { gateway: { bind: "127.0.0.1:7420", daemonPolicyUrl: "", maxInvokeDurationMs: 30000, catalogRefreshIntervalSecs: 300, useMockFallback: true }, servers: [] },
    };
  }
  const loaded = await invoke<McpGatewayConfigWire>("mcp_gateway_load_config");
  return {
    path: loaded.path,
    fileExists: loaded.fileExists,
    config: {
      gateway: {
        bind: loaded.config.gateway.bind,
        daemonPolicyUrl: loaded.config.gateway.daemon_policy_url,
        maxInvokeDurationMs: loaded.config.gateway.max_invoke_duration_ms,
        catalogRefreshIntervalSecs: loaded.config.gateway.catalog_refresh_interval_secs,
        useMockFallback: loaded.config.gateway.use_mock_fallback,
      },
      servers: loaded.config.servers.map((server) => ({
        id: server.id,
        title: server.title,
        enabled: server.enabled,
        transport: server.transport,
        command: server.command,
        args: server.args ?? [],
        url: server.url,
        bearerToken: server.bearer_token,
        allowedLanes: server.allowed_lanes ?? [],
        allowedEffectClasses: server.allowed_effect_classes ?? [],
        useMock: server.use_mock ?? false,
      })),
    },
  };
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

export async function fetchMcpOAuthStatus(serverId: string): Promise<McpOAuthStatus> {
  const status = await invoke<McpOAuthStatusWire>("mcp_oauth_status", { serverId });
  return normalizeMcpOAuthStatus(status);
}

export async function authorizeMcpOAuth(serverId: string): Promise<CompleteMcpOAuthResult> {
  const result = await invoke<CompleteMcpOAuthWire>("mcp_oauth_authorize", { serverId });
  return { connection: normalizeMcpOAuthStatus(result.connection) };
}

export async function disconnectMcpOAuth(serverId: string): Promise<void> {
  await invoke("mcp_oauth_disconnect", { serverId });
}

// Alias for clarity in UI
export type McpGatewayApplyServerRequest = McpServerUpsertRequest;
