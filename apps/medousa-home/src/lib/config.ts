import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import type { TuiDefaults } from "$lib/types/workshopDefaults";

export interface MedousaConfigPaths {
  dataDir: string;
  configDir: string;
  productConfig: string;
  tuiDefaults: string;
  capabilities: string;
  mcpGateway: string;
}

export interface TuiDefaultsSummary {
  provider?: string | null;
  model?: string | null;
  responseDepthMode?: string | null;
  stageRouting?: StageRoutingMatrix | null;
}

export async function getMedousaConfigPaths(): Promise<MedousaConfigPaths> {
  return invoke<MedousaConfigPaths>("medousa_config_paths");
}

export async function loadTuiDefaultsSummary(): Promise<TuiDefaultsSummary> {
  return invoke<TuiDefaultsSummary>("load_tui_defaults_summary");
}

export async function loadTuiDefaults(): Promise<TuiDefaults> {
  return invoke<TuiDefaults>("load_tui_defaults");
}

export async function persistTuiDefaults(defaults: TuiDefaults): Promise<void> {
  return invoke("persist_tui_defaults", { dto: defaults });
}

export async function persistTuiRuntimePrefs(
  provider: string,
  model: string,
  responseDepthMode: string,
  stageRouting?: StageRoutingMatrix,
): Promise<void> {
  return invoke("persist_tui_runtime_prefs", {
    provider,
    model,
    responseDepthMode,
    stageRouting: stageRouting ?? null,
  });
}

export async function openConfigPath(path: string): Promise<void> {
  await openPath(path);
}
