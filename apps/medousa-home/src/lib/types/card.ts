import type { WorkCard } from "$lib/types/workspace";

export interface WorkCardAssociations {
  vault_paths: string[];
  artifact_ids: string[];
  locus_node_ids: string[];
}

export interface WorkCardDetail {
  card: WorkCard;
  kind: string;
  subtitle?: string | null;
  session_id?: string | null;
  correlation_id?: string | null;
  manuscript_id?: string | null;
  job_id?: string | null;
  work_id?: string | null;
  job_type?: string | null;
  user_ack?: string | null;
  wrapping_up_reasons: string[];
  terminal: boolean;
  error?: string | null;
  result_excerpt?: string | null;
  task_line?: string | null;
  tool_names?: string[] | null;
  associations: WorkCardAssociations;
}
