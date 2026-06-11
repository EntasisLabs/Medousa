import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface ProvidersProbeResult {
  ollamaDetected: boolean;
  ollamaBaseUrl: string;
  ollamaModels: string[];
  networkOnline: boolean;
  suggestedOllamaModel?: string | null;
}

export interface ProvidersValidateKeyRequest {
  provider: string;
  apiKey: string;
  baseUrl?: string | null;
}

export interface ProvidersValidateKeyResult {
  ok: boolean;
  message: string;
  suggestedModel?: string | null;
}

export interface DaemonStartResult {
  started: boolean;
  alreadyRunning: boolean;
  pid?: number | null;
  logPath: string;
  message: string;
}

export interface DaemonWaitHealthResult {
  ok: boolean;
  message: string;
  attempts: number;
}

export interface WizardApplyScreen1Request {
  path: "managed" | "byok" | "offline";
  provider: string;
  model: string;
  baseUrl?: string | null;
  apiKey?: string | null;
  startCore?: boolean;
}

export interface WizardApplyScreen1Result {
  coreReady: boolean;
  coreMessage: string;
  provider: string;
  model: string;
}

const LOCAL_PROBE: ProvidersProbeResult = {
  ollamaDetected: false,
  ollamaBaseUrl: "http://127.0.0.1:11434/v1/",
  ollamaModels: [],
  networkOnline: true,
  suggestedOllamaModel: "llama3.2",
};

export async function probeProviders(): Promise<ProvidersProbeResult> {
  if (!isTauri()) return LOCAL_PROBE;
  return invoke<ProvidersProbeResult>("providers_probe");
}

export async function validateProviderKey(
  request: ProvidersValidateKeyRequest,
): Promise<ProvidersValidateKeyResult> {
  if (!isTauri()) {
    if (request.provider === "ollama") {
      return { ok: false, message: "Ollama probe requires the desktop app", suggestedModel: null };
    }
    if (!request.apiKey.trim()) {
      return { ok: false, message: "API key is required", suggestedModel: null };
    }
    return {
      ok: true,
      message: "Key accepted (dev browser mode — not validated)",
      suggestedModel: null,
    };
  }
  return invoke<ProvidersValidateKeyResult>("providers_validate_key", { request });
}

export async function startDaemonCore(): Promise<DaemonStartResult> {
  if (!isTauri()) {
    return {
      started: false,
      alreadyRunning: false,
      logPath: "",
      message: "Start Medousa Core from the terminal in dev browser mode",
    };
  }
  return invoke<DaemonStartResult>("daemon_start");
}

export async function waitForDaemonCore(
  timeoutSeconds = 30,
): Promise<DaemonWaitHealthResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", attempts: 0 };
  }
  return invoke<DaemonWaitHealthResult>("daemon_wait_healthy", {
    request: { timeoutSeconds, pollMs: 2000 },
  });
}

export async function applyWizardScreen1(
  request: WizardApplyScreen1Request,
): Promise<WizardApplyScreen1Result> {
  if (!isTauri()) {
    return {
      coreReady: false,
      coreMessage: "Saved locally in dev mode — run the desktop app to start Core",
      provider: request.provider,
      model: request.model,
    };
  }
  return invoke<WizardApplyScreen1Result>("wizard_apply_screen1", { request });
}
