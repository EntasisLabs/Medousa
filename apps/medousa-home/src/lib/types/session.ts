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
