export interface RecurringDefinitionEntry {
  recurring_id: string;
  queue: string;
  job_type: string;
  cron_expr: string;
  timezone: string;
  enabled: boolean;
  next_run_at_utc: string;
  last_run_at_utc: string | null;
  manuscript_id?: string | null;
  prompt_excerpt?: string | null;
}

export interface RecurringListResponse {
  count: number;
  recurring: RecurringDefinitionEntry[];
}

export interface RegisterRecurringResponse {
  recurring_id: string;
  queue: string;
  next_run_at_utc: string;
  cron_expr: string;
  timezone: string;
}

export interface RegisterRecurringRequest {
  prompt: string;
  cron_expr: string;
  manuscript_id?: string;
  timezone?: string;
  execution_mode?: string;
  model_hint?: string;
}

export interface UpdateRecurringRequest {
  enabled?: boolean;
  cron_expr?: string;
  timezone?: string;
}

export interface UpdateRecurringResponse {
  recurring_id: string;
  enabled: boolean;
  cron_expr: string;
  timezone: string;
  next_run_at_utc: string;
}

export interface DeleteRecurringResponse {
  recurring_id: string;
  deleted: boolean;
}

export interface CronCreateDraft {
  prompt: string;
  cron_expr: string;
  manuscript_id?: string;
  timezone?: string;
}
