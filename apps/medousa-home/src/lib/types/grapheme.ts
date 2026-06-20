export interface GraphemeModuleSummary {
  module_id: string;
  version: string;
  abi: string;
  entrypoint: string;
  op_count: number;
  effects: string[];
  required_capabilities: string[];
}

export interface GraphemeModulesListResponse {
  count: number;
  modules: GraphemeModuleSummary[];
}

export interface GraphemeCompactModuleOp {
  op: string;
  stability: string;
  effect: string;
  output_type: string;
}

export interface GraphemeModuleInfo {
  module_id: string;
  version: string;
  abi: string;
  entrypoint: string;
  required_capabilities: string[];
  limits: {
    max_cpu_ms: number;
    max_memory_mb: number;
    max_io_bytes: number;
    max_network_calls: number;
  };
  op_summary?: {
    total: number;
    by_effect: Record<string, number>;
  };
  exported_ops: GraphemeCompactModuleOp[];
}

export interface GraphemeModuleDetailResponse {
  info: GraphemeModuleInfo;
  examples: string[];
}

export interface GraphemeModuleOpRow {
  module_id: string;
  op: string;
  effect: string;
  stability: string;
  output_type: string;
}

export interface GraphemeModuleOpsResponse {
  module_id: string;
  query: string;
  matches: GraphemeModuleOpRow[];
}

export interface GraphemeScriptEntry {
  id: string;
  name: string;
  modules: string[];
  tags: string[];
  intent?: string | null;
  version: number;
  score?: number | null;
  line?: string | null;
  body_path?: string | null;
  body_hash?: string | null;
  created_at_utc?: string | null;
  updated_at_utc?: string | null;
  source_session_id?: string | null;
  body_preview?: string | null;
}

export interface GraphemeScriptsListResponse {
  count: number;
  scripts: GraphemeScriptEntry[];
}

export interface GraphemeScriptDetailResponse {
  script: GraphemeScriptEntry;
  body_preview: string;
  body_truncated: boolean;
}

export interface GraphemeRunResponse {
  result: {
    mode?: string;
    job_id?: string;
    succeeded?: boolean;
    attempt_outcome?: string;
    diagnostics?: unknown;
  };
}
