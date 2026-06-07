export interface JobResultResponse {
  job_id: string;
  status: string;
  is_terminal: boolean;
  attempt_count: number;
  latest_outcome: string | null;
  latest_execution_id: string | null;
  output_text: string | null;
}

export interface EnqueueResponse {
  job_id: string;
  queue: string;
  accepted_at_utc: string;
}
