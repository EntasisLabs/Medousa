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
