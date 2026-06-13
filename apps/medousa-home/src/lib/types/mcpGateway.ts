export interface McpServerConfig {
  id: string;
  title: string;
  enabled: boolean;
  transport: string;
  command?: string | null;
  args: string[];
  url?: string | null;
  bearerToken?: string | null;
  allowedLanes: string[];
  allowedEffectClasses: string[];
  useMock: boolean;
}

export interface McpGatewayFileConfig {
  gateway: {
    bind: string;
    daemonPolicyUrl: string;
    maxInvokeDurationMs: number;
    catalogRefreshIntervalSecs: number;
    useMockFallback: boolean;
  };
  servers: McpServerConfig[];
}

export interface McpGatewayConfigLoadResult {
  path: string;
  config: McpGatewayFileConfig;
  fileExists: boolean;
}

export interface McpGatewayHealth {
  status: string;
  invokesEnabled: boolean;
  registeredServers: number;
  connectedServers: number;
  catalogEntries: number;
}

export interface McpServerRuntime {
  serverId: string;
  title: string;
  enabled: boolean;
  connected: boolean;
  toolCount: number;
  allowedLanes: string[];
}

export interface McpGatewayStatusResult {
  gatewayUrl: string;
  reachable: boolean;
  message: string;
  health: McpGatewayHealth | null;
  servers: McpServerRuntime[];
  configPath: string;
}

export interface McpGatewayRestartResult {
  started: boolean;
  alreadyRunning: boolean;
  logPath: string;
  message: string;
}

export interface McpServerUpsertRequest {
  id: string;
  title: string;
  enabled?: boolean;
  transport?: string;
  command?: string | null;
  args?: string[];
  url?: string | null;
  bearerToken?: string | null;
  useMock?: boolean;
}

export interface McpServerMutationResult {
  ok: boolean;
  message: string;
  configPath: string;
}

export interface McpGatewayTestResult {
  ok: boolean;
  message: string;
  connected: boolean;
  toolCount: number;
}
