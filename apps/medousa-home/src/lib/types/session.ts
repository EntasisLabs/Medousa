export interface SessionSetDisplayNameResponse {
  session_id: string;
  display_name: string;
}

export interface SessionSummary {
  session_id: string;
  display_name?: string | null;
  turns: number;
  verification_runs: number;
  last_timestamp?: string | null;
  preview: string;
}

export interface SessionHistoryResponse {
  session_id: string;
  turns: SessionTurn[];
}

export interface SessionTurn {
  role: string;
  content: string;
  timestamp: string;
  tool_names?: string[];
  answer_state?: string | null;
  parts?: import("$lib/types/turnParts").TurnPart[] | null;
}

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

export type TurnTicketPhase =
  | "accepted"
  | "streaming"
  | "worker_handoff"
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
  mode?: TurnTicketMode;
  provider?: string;
  model?: string;
  responseDepthMode?: string;
  stageRouting?: import("$lib/types/runtime").StageRoutingMatrix;
  channelSurface?: string;
  mediaRefs?: import("$lib/types/media").MediaRef[];
}
