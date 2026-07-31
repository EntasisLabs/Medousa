import type {
  InteractiveTurnRequest,
  InteractiveTurnResponse,
  InteractiveTurnStreamEvent,
} from "./generated/daemon_api.js";

export type {
  InteractiveTurnRequest,
  InteractiveTurnResponse,
  InteractiveTurnStreamEvent,
};

export type MedousaSurface = "vscode" | "neovim" | "obsidian";

export interface Position {
  line: number;
  character: number;
}

export interface Diagnostic {
  message: string;
  severity?: "error" | "warning" | "info" | "hint";
  source?: string;
  range?: { start: Position; end: Position };
}

export interface MedousaContext {
  surface: MedousaSurface;
  workspace?: string;
  file?: string;
  language?: string;
  selection?: { text: string; start?: Position; end?: Position };
  diagnostics?: Diagnostic[];
  vaultRootId?: string;
  notePath?: string;
  sessionId?: string;
}

export interface HealthResponse {
  [key: string]: unknown;
}

export interface CapabilityListResponse {
  capabilities?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface SessionSummary {
  id?: string;
  session_id?: string;
  display_name?: string | null;
  [key: string]: unknown;
}

export interface CreateSessionRequest {
  session_id?: string;
  catalog?: string;
  member_profile_ids?: string[];
  agent_profile_id?: string;
  display_name?: string;
}

export interface CreateSessionResponse {
  session_id: string;
  catalog: string;
  display_name?: string | null;
  member_profile_ids?: string[];
  agent_profile_id?: string | null;
}

export interface SessionTurn {
  role: string;
  content: string;
  timestamp: string;
  tool_names?: string[];
  answer_state?: string | null;
}

export interface SessionHistoryResponse {
  session_id: string;
  turns: SessionTurn[];
}

export interface RuntimeDefaults {
  backend: string;
  provider: string;
  model: string;
  response_depth_mode: string;
  reasoning_effort: string;
  base_url?: string | null;
  stage_routing: Record<string, unknown>;
  [key: string]: unknown;
}

export interface ClientOptions {
  baseUrl: string;
  bearerToken?: string;
  fetch?: typeof globalThis.fetch;
}

export interface StreamOptions {
  signal?: AbortSignal;
  maxReconnectAttempts?: number;
  reconnectDelayMs?: (attempt: number) => number;
}

export interface ClientRequestOptions {
  signal?: AbortSignal;
}
