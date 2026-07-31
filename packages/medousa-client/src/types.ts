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
