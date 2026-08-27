import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";
import type {
  BeginMcpOAuthResult,
  CompleteMcpOAuthResult,
  McpGatewayConfigLoadResult,
  McpGatewayRestartResult,
  McpGatewayStatusResult,
  McpGatewayTestResult,
  McpOAuthStatus,
  McpServerMutationResult,
  McpServerUpsertRequest,
} from "$lib/types/mcpGateway";

const MCP_OAUTH_REDIRECT_URI = "medousa://mcp/oauth/callback";
const MCP_OAUTH_PENDING_KEY = "medousa-mcp-oauth-pending-v1";
export const MCP_OAUTH_CHANGED_EVENT = "medousa-mcp-oauth-changed";

interface McpOAuthStatusWire {
  server_id: string;
  status: string;
  connected: boolean;
  issuer?: string | null;
  scopes?: string[];
}

interface BeginMcpOAuthWire {
  server_id: string;
  login_id: string;
  authorization_url: string;
}

interface CompleteMcpOAuthWire {
  connection: McpOAuthStatusWire;
}

interface PendingMcpOAuthLogin {
  loginId: string;
  serverId: string;
  createdAt: number;
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

function readPendingMcpOAuthLogins(): Record<string, PendingMcpOAuthLogin> {
  if (typeof localStorage === "undefined") return {};
  try {
    const value = JSON.parse(localStorage.getItem(MCP_OAUTH_PENDING_KEY) ?? "{}");
    return value && typeof value === "object"
      ? value as Record<string, PendingMcpOAuthLogin>
      : {};
  } catch {
    return {};
  }
}

function writePendingMcpOAuthLogins(logins: Record<string, PendingMcpOAuthLogin>) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(MCP_OAUTH_PENDING_KEY, JSON.stringify(logins));
}

function authorizationState(authorizationUrl: string): string {
  const state = new URL(authorizationUrl).searchParams.get("state")?.trim();
  if (!state) throw new Error("MCP authorization did not provide a callback state");
  return state;
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

export async function beginMcpOAuth(serverId: string): Promise<BeginMcpOAuthResult> {
  const result = await invoke<BeginMcpOAuthWire>("mcp_oauth_begin", {
    request: {
      server_id: serverId,
      redirect_uri: MCP_OAUTH_REDIRECT_URI,
      scopes: [],
    },
  });
  const state = authorizationState(result.authorization_url);
  const pending = readPendingMcpOAuthLogins();
  for (const [pendingState, login] of Object.entries(pending)) {
    if (login.serverId === result.server_id || Date.now() - login.createdAt > 60 * 60 * 1000) {
      delete pending[pendingState];
    }
  }
  pending[state] = {
    loginId: result.login_id,
    serverId: result.server_id,
    createdAt: Date.now(),
  };
  writePendingMcpOAuthLogins(pending);
  return {
    serverId: result.server_id,
    loginId: result.login_id,
    authorizationUrl: result.authorization_url,
  };
}

export async function completeMcpOAuthCallback(
  callbackUrl: string,
): Promise<CompleteMcpOAuthResult> {
  const state = new URL(callbackUrl).searchParams.get("state")?.trim();
  const pending = readPendingMcpOAuthLogins();
  const login = state ? pending[state] : null;
  if (!state || !login) {
    throw new Error("This MCP sign-in is no longer pending; start it again from Settings → MCP");
  }
  const result = await invoke<CompleteMcpOAuthWire>("mcp_oauth_complete", {
    request: {
      login_id: login.loginId,
      callback_url: callbackUrl,
    },
  });
  delete pending[state];
  writePendingMcpOAuthLogins(pending);
  const normalized = { connection: normalizeMcpOAuthStatus(result.connection) };
  window.dispatchEvent(new CustomEvent(MCP_OAUTH_CHANGED_EVENT, { detail: normalized }));
  return normalized;
}

export async function disconnectMcpOAuth(serverId: string): Promise<void> {
  await invoke("mcp_oauth_disconnect", { serverId });
}

// Alias for clarity in UI
export type McpGatewayApplyServerRequest = McpServerUpsertRequest;
