import { isTauri } from "$lib/window";

export type EngineIssue =
  | "ok"
  | "not_running"
  | "stale_lock"
  | "port_blocked"
  | "wedged"
  | "binary_missing"
  | "remote";

export interface EngineDiagnosis {
  issue: EngineIssue;
  title: string;
  message: string;
  logPath?: string | null;
  lockPath?: string | null;
  bind?: string | null;
  canClearLock: boolean;
  canRestart: boolean;
}

const LOCAL_FALLBACK: EngineDiagnosis = {
  issue: "not_running",
  title: "Medousa isn't running",
  message: "Start Medousa on this computer to chat.",
  canClearLock: false,
  canRestart: true,
};

export async function diagnoseEngine(): Promise<EngineDiagnosis> {
  if (!isTauri()) return LOCAL_FALLBACK;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<EngineDiagnosis>("engine_diagnose");
}

export async function clearEngineStaleLock(): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("engine_clear_stale_lock");
}

export async function openEngineLog(logPath: string | null | undefined): Promise<void> {
  const path = logPath?.trim();
  if (!path || !isTauri()) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}
