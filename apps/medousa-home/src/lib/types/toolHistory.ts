import type { WorkflowRunRequest } from "$lib/types/workflow";

export interface ToolHistoryRunEntry {
  entry_id: string;
  session_id: string;
  slice_id: string;
  turn_index: number;
  tool_round: number;
  run_id: string;
  tool_name: string;
  status: string;
  input_summary: string;
  sanitized_input: Record<string, unknown>;
  args_hash: string;
  redacted: boolean;
  output_preview?: string | null;
  timestamp: string;
  session_preview?: string | null;
}

export interface ToolHistoryListResponse {
  count: number;
  runs: ToolHistoryRunEntry[];
}

export interface ToolHistorySliceRef {
  session_id: string;
  slice_id: string;
  tool_round?: number | null;
  run_id?: string | null;
}

export interface WorkflowFromSliceRequest {
  refs: ToolHistorySliceRef[];
  name?: string | null;
  run?: boolean;
}

export interface WorkflowFromSliceResponse {
  workflow_id?: string | null;
  draft: WorkflowRunRequest;
  promoted_count: number;
  notes: string[];
}

export function sliceRefFromRun(entry: ToolHistoryRunEntry): ToolHistorySliceRef {
  return {
    session_id: entry.session_id,
    slice_id: entry.slice_id,
    tool_round: entry.tool_round,
    run_id: entry.run_id,
  };
}

export function sliceRefFromChatToolRun(options: {
  sessionId: string;
  turnIndex: number;
  runId: string;
  toolRound: number;
}): ToolHistorySliceRef {
  return {
    session_id: options.sessionId,
    slice_id: `turn:${options.turnIndex}`,
    tool_round: options.toolRound,
    run_id: options.runId,
  };
}
