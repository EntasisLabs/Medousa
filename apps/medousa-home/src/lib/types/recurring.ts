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
  display_name?: string | null;
  execution_mode?: string | null;
  delivery_label?: string | null;
  last_run_status?: string | null;
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

export type AutomationDeliveryMode = "in_app" | "telegram" | "quiet";

export interface RegisterRecurringRequest {
  prompt: string;
  cron_expr: string;
  display_name?: string;
  manuscript_id?: string;
  timezone?: string;
  execution_mode?: string;
  model_hint?: string;
  delivery_mode?: AutomationDeliveryMode;
  telegram_chat_id?: string;
}

export interface UpdateRecurringRequest {
  enabled?: boolean;
  cron_expr?: string;
  timezone?: string;
  display_name?: string;
  delivery?: Record<string, unknown> | null;
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

export interface RecurringRunEntry {
  job_id: string;
  status: string;
  is_terminal: boolean;
  attempt_count: number;
  latest_outcome?: string | null;
  output_text?: string | null;
  scheduled_at_utc: string;
  updated_at_utc: string;
}

export interface RecurringRunsResponse {
  recurring_id: string;
  count: number;
  runs: RecurringRunEntry[];
}

export interface RecurringDeliveryResponse {
  recurring_id: string;
  delivery_label: string;
  delivery?: Record<string, unknown> | null;
}

/** @deprecated use AutomationCreateDraft */
export interface CronCreateDraft {
  prompt: string;
  cron_expr: string;
  manuscript_id?: string;
  timezone?: string;
}

export interface AutomationCreateDraft {
  prompt: string;
  cron_expr: string;
  display_name?: string;
  manuscript_id?: string;
  timezone?: string;
  delivery_mode?: AutomationDeliveryMode;
  telegram_chat_id?: string;
}
