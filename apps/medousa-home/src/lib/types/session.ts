import type {
  SessionHistoryResponse as DaemonSessionHistoryResponse,
  TranscriptEntry,
} from "$lib/types/generated/daemon_api";

export interface SessionSetDisplayNameResponse {
  session_id: string;
  display_name: string;
}

export interface SessionDeleteResponse {
  session_id: string;
  /** Absent on pre-H02.4 remote workshops. */
  deletion_id?: string;
  /** Absent on pre-H02.4 remote workshops; derive from `deleted`. */
  status?: "deleting" | "complete" | "retryable_partial" | "blocked";
  deleted: boolean;
  locus_purged: boolean;
  locus_nodes_deleted: number;
  cancelled_active_turn: boolean;
  surfaces?: Array<{ surface: string; deleted: boolean; reason_class?: string | null }>;
}

export interface SessionSummary {
  session_id: string;
  display_name?: string | null;
  turns: number;
  verification_runs: number;
  last_timestamp?: string | null;
  preview: string;
  /** Present as `"shared"` for multi-member rooms. */
  catalog?: string | null;
  /** First sticky non-home host surface (`vscode` | `neovim` | `obsidian` | `browser`). */
  origin_surface?: string | null;
  /** Sticky once a Forge code binding was set. */
  has_code_work?: boolean;
}

export type SessionHistoryResponse = DaemonSessionHistoryResponse;
export type SessionTurn = TranscriptEntry;

export interface ActiveSessionTurn {
  turn_id: string;
  session_id: string;
  stream_url: string;
  phase: string;
  composer_handoff: boolean;
  started_at: string;
}

export interface ActiveSessionTurnResponse {
  active: boolean;
  turn?: ActiveSessionTurn;
}

export interface CancelActiveSessionTurnResponse {
  cancelled: boolean;
  turn_id?: string;
  message: string;
}

export type TurnTicketMode = "interactive" | "background";

export type AgentModeId = "general" | "coder";

export type TurnTicketPhase =
  | "accepted"
  | "streaming"
  | "worker_handoff"
  | "workshop_handoff"
  | "budget_blocked"
  | "done"
  | "error"
  | "cancelled";

export interface TurnTicketResponse {
  turn_id: string;
  session_id: string;
  mode: TurnTicketMode;
  phase: TurnTicketPhase;
  accepted_at_utc: string;
  stream_url: string;
  stream_ready: boolean;
  workspace_card_id?: string | null;
  daemon_notice?: string | null;
}

export interface TurnTicketRecord {
  turn_id: string;
  session_id: string;
  mode: TurnTicketMode;
  phase: TurnTicketPhase;
  stream_url: string;
  prompt_preview: string;
  workspace_card_id?: string | null;
  composer_handoff: boolean;
  started_at: string;
  updated_at: string;
}

export interface SessionTurnsResponse {
  session_id: string;
  turns: TurnTicketRecord[];
}

export interface CreateTurnTicketRequest {
  sessionId: string;
  prompt: string;
  agentMode?: AgentModeId;
  codeContext?: import("$lib/daemon").CodeIntentContext | null;
  codeProjectSetupAuthorized?: boolean;
  mode?: TurnTicketMode;
  provider?: string;
  model?: string;
  responseDepthMode?: string;
  reasoningEffort?: string;
  stageRouting?: import("$lib/types/runtime").StageRoutingMatrix;
  channelSurface?: string;
  mediaRefs?: import("$lib/types/media").MediaRef[];
  voicePresetId?: string;
  voiceAppendix?: string;
  identityUserId?: string;
}
