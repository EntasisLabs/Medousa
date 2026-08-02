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
  turns?: number;
  last_timestamp?: string | null;
  preview?: string;
  catalog?: string | null;
  [key: string]: unknown;
}

export interface SessionSetDisplayNameResponse {
  session_id: string;
  display_name: string;
}

export interface SessionDeleteResponse {
  session_id: string;
  deleted: boolean;
  locus_purged?: boolean;
  locus_nodes_deleted?: number;
  cancelled_active_turn?: boolean;
}

export interface VaultWriteRequest {
  path?: string;
  content: string;
  session_id?: string;
  semantic_tags?: string[];
  auto_workshop_tags?: boolean;
}

export interface VaultNote {
  path: string;
  title: string;
  byte_size: number;
  content_hash: string;
  modified_at_utc: string;
  created_at_utc: string;
  tags: string[];
  wikilinks_out: string[];
  backlinks: string[];
  kind?: string;
}

export interface VaultNoteContentResponse {
  note: VaultNote;
  content: string;
}

export interface VaultWriteResponse {
  note: VaultNote;
  created: boolean;
  content?: string;
}

export interface VaultSearchHit {
  note: Pick<VaultNote, "path" | "title" | "modified_at_utc" | "kind">;
  score: number;
  matched_terms: string[];
  snippet?: string | null;
}

export interface VaultSearchResponse {
  query: string;
  hits: VaultSearchHit[];
}

export interface VaultBacklinksResponse {
  path: string;
  backlinks: string[];
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
  /** Return after a worker/workshop handoff instead of holding the composer. */
  stopOnHandoff?: boolean;
}

export interface ClientRequestOptions {
  signal?: AbortSignal;
}
