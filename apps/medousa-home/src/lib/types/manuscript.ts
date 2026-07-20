import type { ManuscriptScriptEntry } from "$lib/types/catalog";

export interface ManuscriptScheduledToolEntry {
  tool: string;
  allowed_on_schedule: boolean;
  reason?: string | null;
}

export interface ManuscriptOpenshellSummary {
  enabled: boolean;
  policy_template?: string | null;
  sandbox_from?: string | null;
  allow_scheduled: boolean;
  default_path: string;
}

export interface ManuscriptDetailResponse {
  id: string;
  name: string;
  description?: string | null;
  scope: string;
  path: string;
  extends_from?: string | null;
  display_name?: string | null;
  voice_appendix?: string | null;
  task_template?: string | null;
  tools_allow: string[];
  schedule_cron?: string | null;
  schedule_execution_mode?: string | null;
  delivery_mode?: string | null;
  delivery_on_complete?: string | null;
  openshell: ManuscriptOpenshellSummary;
  has_scripts: boolean;
  scripts: ManuscriptScriptEntry[];
  scheduled_tools: ManuscriptScheduledToolEntry[];
  schedule_ready: boolean;
  schedule_validation_error?: string | null;
  palette_tools: string[];
}

export interface CreateManuscriptRequest {
  name: string;
  description?: string;
  scope?: string;
  template?: string;
}

export interface UpdateManuscriptRequest {
  name?: string;
  description?: string;
  clear_description?: boolean;
  display_name?: string;
  clear_display_name?: boolean;
  voice_appendix?: string;
  clear_voice_appendix?: boolean;
  task_template?: string;
  clear_task_template?: boolean;
  tools_allow?: string[];
  schedule_cron?: string;
  clear_schedule_cron?: boolean;
  schedule_execution_mode?: string;
  delivery_mode?: string;
  delivery_on_complete?: string;
  openshell_allow_scheduled?: boolean;
}

export interface ManuscriptImportRequest {
  path?: string;
  preset?: "hermes" | "openclaw" | "cursor";
  scope?: "user" | "project";
  force?: boolean;
}

export interface ManuscriptImportResultEntry {
  id: string;
  name: string;
  yaml_path: string;
  source: string;
}

export interface ManuscriptImportResponse {
  count: number;
  imported: ManuscriptImportResultEntry[];
}

export type ManuscriptImportPreset = NonNullable<ManuscriptImportRequest["preset"]>;
