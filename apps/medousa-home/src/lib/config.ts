import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import type { TuiDefaults } from "$lib/types/workshopDefaults";
import type { VoicePreset } from "$lib/types/voicePresets";
import type { FavoriteModel } from "$lib/utils/modelCatalog";

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
  reasoningEffort?: string | null;
  stageRouting?: StageRoutingMatrix | null;
  favoriteModels?: FavoriteModel[] | null;
  activeVoiceId?: string | null;
  customVoicePresets?: VoicePreset[] | null;
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
  reasoningEffort?: string,
  stageRouting?: StageRoutingMatrix,
): Promise<void> {
  return invoke("persist_tui_runtime_prefs", {
    provider,
    model,
    responseDepthMode,
    reasoningEffort: reasoningEffort ?? null,
    stageRouting: stageRouting ?? null,
  });
}

export async function persistTuiFavoriteModels(models: FavoriteModel[]): Promise<void> {
  return invoke("persist_tui_favorite_models", { models });
}

export async function persistTuiVoicePrefs(prefs: {
  activeVoiceId: string;
  customVoicePresets?: VoicePreset[];
}): Promise<void> {
  return invoke("persist_tui_voice_prefs", {
    activeVoiceId: prefs.activeVoiceId,
    customVoicePresets: prefs.customVoicePresets ?? null,
  });
}

export async function openConfigPath(path: string): Promise<void> {
  await openPath(path);
}

export async function openConnectionRunbook(): Promise<void> {
  const path = await invoke<string>("connection_runbook_path");
  await openPath(path);
}
