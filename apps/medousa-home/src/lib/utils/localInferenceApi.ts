import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "$lib/window";

export interface LocalHardwareProfile {
  tier: string;
  tierLabel: string;
  recommendedModelId: string;
  recommendedDisplayName: string;
  probe: {
    totalRamMb: number;
    availableRamMb: number;
    cpuCores: number;
    cpuArch: string;
    gpuBackend: string;
    freeDiskGb: number;
  };
}

export interface LocalHardwareResponse {
  profile: LocalHardwareProfile;
  engineAvailable: boolean;
  compiledBackends?: string[];
  message: string;
}

export interface LocalCatalogModel {
  id: string;
  displayName: string;
  family: string;
  variant: string;
  tierMin: string;
  tierMax: string;
  tierRecommended?: boolean;
  sizeBytes: number;
  ramEstimateMb: number;
  modalities: string[];
  tags?: string[];
}

export interface LocalCatalogResponse {
  tier: string;
  tierLabel: string;
  familyDefault: string;
  recommendedModelId: string;
  models: LocalCatalogModel[];
}

export interface ModelDownloadProgress {
  jobId: string;
  modelId: string;
  phase: string;
  bytesDone: number;
  bytesTotal: number;
  percent: number;
  currentFile?: string | null;
  message: string;
  error?: string | null;
}

export interface InstalledLocalModel {
  modelId: string;
  repo: string;
  localPath: string;
  installedAt: string;
  bytesOnDisk: number;
  verified: boolean;
}

export interface LocalModelsResponse {
  installed: InstalledLocalModel[];
  activeDownloads: ModelDownloadProgress[];
}

export interface LocalEngineStatus {
  featureEnabled: boolean;
  loaded: boolean;
  baseUrl: string;
  bind?: string | null;
  modelRepo?: string | null;
  modelAlias?: string | null;
  inferenceBackend?: string | null;
  message: string;
}

export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value >= 10 || index === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}

export async function fetchLocalHardware(): Promise<LocalHardwareResponse> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<LocalHardwareResponse>("local_inference_hardware");
}

export async function fetchLocalCatalog(): Promise<LocalCatalogResponse> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<LocalCatalogResponse>("local_inference_catalog");
}

export async function fetchLocalModels(): Promise<LocalModelsResponse> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<LocalModelsResponse>("local_inference_models");
}

export async function startLocalModelDownload(modelId: string): Promise<ModelDownloadProgress> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<ModelDownloadProgress>("local_inference_start_download", { modelId });
}

export async function loadLocalEngine(modelId?: string | null): Promise<LocalEngineStatus> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<LocalEngineStatus>("local_inference_spawn_engine", { modelId: modelId ?? null });
}

export async function fetchLocalEngineStatus(): Promise<LocalEngineStatus> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<LocalEngineStatus>("local_inference_engine_status");
}

export async function removeLocalModel(modelId: string): Promise<void> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  await invoke("local_inference_remove_model", { modelId });
}

export async function streamLocalModelDownload(jobId: string): Promise<void> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  await invoke("local_inference_stream_download", { jobId });
}

export async function stopLocalModelDownloadStream(): Promise<void> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  await invoke("local_inference_stream_download_stop");
}

export function onModelDownloadProgress(
  handler: (progress: ModelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<string>("model_download_progress", (event) => {
    handler(JSON.parse(event.payload) as ModelDownloadProgress);
  });
}

export function onModelDownloadError(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<{ message: string }>("model_download_progress://error", (event) => {
    handler(event.payload.message);
  });
}

export async function fetchDownloadStatus(jobId: string): Promise<ModelDownloadProgress> {
  if (!isTauri()) throw new Error("Local inference requires the desktop app");
  return invoke<ModelDownloadProgress>("local_inference_download_status", { jobId });
}

export async function waitForModelDownload(
  jobId: string,
  onProgress?: (progress: ModelDownloadProgress) => void,
): Promise<ModelDownloadProgress> {
  const unlisten = await onModelDownloadProgress((progress) => {
    if (progress.jobId === jobId) {
      onProgress?.(progress);
    }
  });

  try {
    await streamLocalModelDownload(jobId);
    while (true) {
      const progress = await fetchDownloadStatus(jobId);
      onProgress?.(progress);
      if (progress.phase === "ready") {
        return progress;
      }
      if (progress.phase === "failed") {
        throw new Error(progress.error ?? progress.message);
      }
      await new Promise((resolve) => setTimeout(resolve, 1200));
    }
  } finally {
    unlisten();
    await stopLocalModelDownloadStream();
  }
}

export async function ensureLocalModelReady(
  modelId: string,
  onProgress?: (progress: ModelDownloadProgress) => void,
): Promise<ModelDownloadProgress> {
  const models = await fetchLocalModels();
  if (models.installed.some((entry) => entry.modelId === modelId)) {
    return {
      jobId: "installed",
      modelId,
      phase: "ready",
      bytesDone: 0,
      bytesTotal: 0,
      percent: 100,
      message: "Already installed",
    };
  }
  const active = models.activeDownloads.find((job) => job.modelId === modelId);
  const job = active ?? (await startLocalModelDownload(modelId));
  onProgress?.(job);
  if (job.phase === "ready") return job;
  return waitForModelDownload(job.jobId, onProgress);
}
