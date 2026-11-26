export interface McpServerConfig {
  id: string;
  title: string;
  enabled: boolean;
  transport: string;
  command?: string | null;
  args: string[];
  url?: string | null;
  bearerToken?: string | null;
  bearerTokenConfigured: boolean;
  allowedLanes: string[];
  allowedEffectClasses: string[];
  toolTags: Record<string, string[]>;
  disabledTools: string[];
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
  lastError?: string | null;
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
  clearBearerToken?: boolean;
  toolTags?: Record<string, string[]>;
  disabledTools?: string[];
  useMock?: boolean;
}

export interface McpGatewayTool {
  toolName: string;
  title: string;
  enabled: boolean;
  available: boolean;
  capabilityIds: string[];
  discoveryHints: string[];
}

export interface McpGatewayToolsResult {
  tools: McpGatewayTool[];
  message: string;
}

export interface McpToolUpdateRequest {
  serverId: string;
  toolName: string;
  enabled: boolean;
  discoveryHints: string[];
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

export interface ConnectorSetupField {
  key: string;
  title: string;
  sensitive: boolean;
  required: boolean;
  link?: string | null;
}

export interface ConnectorCard {
  id: string;
  title: string;
  publisher: string;
  categories: string[];
  transport: string;
  authKind: string;
  desktopOnly: boolean;
  needsClientId: boolean;
  googleAgentClient: boolean;
  elicitationFallbackUrl?: string | null;
  setup: ConnectorSetupField[];
}

export interface ConnectorInstallRequest {
  id: string;
  setup?: Record<string, string>;
  clientId?: string | null;
  clientSecret?: string | null;
}

export interface ConnectorInstallResult {
  ok: boolean;
  connectorId: string;
  serverId: string;
  toolNames: string[];
  skillImported: boolean;
  message: string;
}

export interface McpElicitationPrompt {
  id: string;
  server_id?: string | null;
  mode: string;
  message: string;
  requested_schema?: unknown;
  url?: string | null;
}

export interface McpOAuthStatus {
  serverId: string;
  status: string;
  connected: boolean;
  issuer: string | null;
  scopes: string[];
}

export interface CompleteMcpOAuthResult {
  connection: McpOAuthStatus;
}
