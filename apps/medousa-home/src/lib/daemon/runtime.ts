import { invoke } from "@tauri-apps/api/core";
import type {
  ContinuationStatusResponse,
  DaemonStatsResponse,
  DeliveryHealthResponse,
  RuntimeConfigCommandResponse,
  RuntimeDefaultsResponse,
  RuntimeWorkerConfig,
  StageRouteCommandResponse,
  StageRoutingMatrix,
} from "$lib/types/runtime";
import type { TuiDefaults } from "$lib/types/workshopDefaults";
import type { DaemonHealth } from "./client";

export async function checkDaemonHealth(): Promise<DaemonHealth> {
  return invoke<DaemonHealth>("daemon_health");
}

export async function getRuntimeStats(): Promise<DaemonStatsResponse> {
  return invoke<DaemonStatsResponse>("runtime_get_stats");
}

export async function getRuntimeDefaults(): Promise<RuntimeDefaultsResponse> {
  return invoke<RuntimeDefaultsResponse>("runtime_get_defaults");
}

export async function getRuntimeWorkerConfig(): Promise<RuntimeWorkerConfig> {
  return invoke<RuntimeWorkerConfig>("runtime_get_worker_config");
}

export async function putRuntimeWorkerConfig(
  config: RuntimeWorkerConfig,
): Promise<RuntimeWorkerConfig> {
  return invoke<RuntimeWorkerConfig>("runtime_put_worker_config", { config });
}

export async function getEngineTuiDefaults(): Promise<TuiDefaults> {
  return invoke<TuiDefaults>("runtime_get_tui_defaults");
}

let hostCharterInflight: Promise<TuiDefaults> | null = null;

/** One in-flight host charter fetch — companion shells copy locally and reuse. */
export async function fetchHostCharter(): Promise<TuiDefaults> {
  hostCharterInflight ??= getEngineTuiDefaults().finally(() => {
    hostCharterInflight = null;
  });
  return hostCharterInflight;
}

export async function putEngineTuiDefaults(dto: TuiDefaults): Promise<void> {
  await invoke("runtime_put_tui_defaults", { dto });
}

export async function migrateGlobalTuiDefaultsToEngine(): Promise<boolean> {
  return invoke<boolean>("migrate_global_tui_defaults_to_engine");
}

export async function getDeliveryStatus(): Promise<DeliveryHealthResponse> {
  return invoke<DeliveryHealthResponse>("runtime_get_delivery_status");
}

export async function getContinuationStatus(): Promise<ContinuationStatusResponse> {
  return invoke<ContinuationStatusResponse>("runtime_get_continuation_status");
}

export async function sendRuntimeConfigCommand(request: {
  current_provider: string;
  current_model: string;
  draft_provider: string;
  draft_model: string;
  current_response_depth_mode: string;
  current_reasoning_effort?: string;
  command:
    | { command: "model"; args: string[] }
    | { command: "depth"; mode: string | null }
    | { command: "reasoning"; mode: string | null };
}): Promise<RuntimeConfigCommandResponse> {
  return invoke<RuntimeConfigCommandResponse>("runtime_config_command", {
    request,
  });
}

export async function sendStageRouteCommand(request: {
  stage_routing: StageRoutingMatrix;
  provider: string;
  model: string;
  command:
    | { command: "routes"; role: string | null }
    | {
        command: "set";
        role: string;
        target: string;
        policy_profile: string | null;
        fallback_chain: string[] | null;
      }
    | { command: "reset" };
}): Promise<StageRouteCommandResponse> {
  return invoke<StageRouteCommandResponse>("runtime_stage_route_command", {
    request,
  });
}

export interface StorageGovernorSettings {
  enabled: boolean;
  repository_cache_max_bytes: number;
  global_cache_max_bytes: number;
  free_disk_floor_bytes: number;
  min_inactive_age_hours: number;
}

export interface StorageCategoryUsage {
  physical_bytes: number;
  file_count: number;
}

export interface ForgeCacheUsage {
  repository_key: string;
  physical_bytes: number;
  file_count: number;
  last_used_unix_seconds: number;
  protected: boolean;
  protection_reason: string | null;
}

export interface StorageUsageReport {
  settings: StorageGovernorSettings;
  data_root: string;
  available_disk_bytes: number | null;
  total_managed_bytes: number;
  forge_metadata: StorageCategoryUsage;
  forge_worktrees: StorageCategoryUsage;
  build_caches: StorageCategoryUsage;
  detamu: StorageCategoryUsage;
  artifacts: StorageCategoryUsage;
  coder_evidence: StorageCategoryUsage;
  forge_caches: ForgeCacheUsage[];
  scan_warnings: string[];
}

export interface StorageMaintenanceReport {
  enabled: boolean;
  dry_run: boolean;
  before: StorageUsageReport;
  after: StorageUsageReport;
  selected_bytes: number;
  reclaimed_bytes: number;
  actions: Array<{
    repository_key: string;
    physical_bytes: number;
    reason: string;
    status: string;
  }>;
  pressure_remaining: boolean;
}

export async function getStorageStatus(): Promise<StorageUsageReport> {
  return invoke<StorageUsageReport>("storage_status");
}

export async function updateStorageSettings(
  request: StorageGovernorSettings,
): Promise<StorageUsageReport> {
  return invoke<StorageUsageReport>("storage_settings_update", { request });
}

export async function runStorageMaintenance(
  dryRun: boolean,
): Promise<StorageMaintenanceReport> {
  return invoke<StorageMaintenanceReport>("storage_maintenance_run", {
    request: { dry_run: dryRun },
  });
}
