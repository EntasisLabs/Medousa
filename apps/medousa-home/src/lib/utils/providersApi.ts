import type {
  ProviderCatalogEntry,
  ProvidersListResult,
} from "$lib/types/providers";
import { isTauri } from "$lib/window";

export type {
  ProviderCatalogEntry,
  ProviderCategory,
  ProvidersListResult,
} from "$lib/types/providers";

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

export interface ProvidersListModelsRequest {
  provider: string;
  apiKey?: string | null;
  baseUrl?: string | null;
}

export interface ProvidersListModelsResult {
  models: string[];
  source: string;
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

const LOCAL_CATALOG: ProvidersListResult = {
  categories: [
    { id: "featured", label: "Popular" },
    { id: "local", label: "On this device" },
    { id: "cloud", label: "More providers" },
  ],
  providers: [
    {
      id: "openai",
      label: "OpenAI",
      category: "featured",
      defaultModel: "gpt-5.4-mini",
      needsApiKey: true,
      supportsCustomBaseUrl: true,
      defaultBaseUrl: "https://api.openai.com/v1",
      keyHint: "sk-…",
      blurb: "GPT-5.4 family",
    },
    {
      id: "anthropic",
      label: "Anthropic",
      category: "featured",
      defaultModel: "claude-sonnet-4-6",
      needsApiKey: true,
      supportsCustomBaseUrl: false,
      defaultBaseUrl: null,
      keyHint: "sk-ant-…",
      blurb: "Claude 4.6",
    },
    {
      id: "deepseek",
      label: "DeepSeek",
      category: "featured",
      defaultModel: "deepseek-v4-flash",
      needsApiKey: true,
      supportsCustomBaseUrl: true,
      defaultBaseUrl: "https://api.deepseek.com/v1",
      keyHint: "sk-…",
      blurb: "DeepSeek V4",
    },
    {
      id: "ollama",
      label: "Ollama (local)",
      category: "local",
      defaultModel: "llama3.2",
      needsApiKey: false,
      supportsCustomBaseUrl: true,
      defaultBaseUrl: "http://127.0.0.1:11434/v1",
      keyHint: null,
      blurb: "Local Ollama",
    },
    {
      id: "custom",
      label: "Custom provider",
      category: "local",
      defaultModel: "default",
      needsApiKey: false,
      supportsCustomBaseUrl: true,
      defaultBaseUrl: null,
      keyHint: "Optional — sk-…",
      blurb: "OpenAI-compatible endpoint (vLLM, LiteLLM, etc.)",
    },
  ],
};

let catalogCache: ProvidersListResult | null = null;

export function findCatalogProvider(
  catalog: ProvidersListResult,
  id: string,
): ProviderCatalogEntry | undefined {
  const normalized = id.trim().toLowerCase();
  return catalog.providers.find((entry) => entry.id.toLowerCase() === normalized);
}

export async function listProviders(force = false): Promise<ProvidersListResult> {
  if (catalogCache && !force) return catalogCache;
  if (!isTauri()) {
    catalogCache = LOCAL_CATALOG;
    return catalogCache;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  catalogCache = await invoke<ProvidersListResult>("providers_list");
  return catalogCache;
}

export async function probeProviders(): Promise<ProvidersProbeResult> {
  if (!isTauri()) return LOCAL_PROBE;
  const { invoke } = await import("@tauri-apps/api/core");
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
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProvidersValidateKeyResult>("providers_validate_key", { request });
}

export async function listProviderModels(
  request: ProvidersListModelsRequest,
): Promise<ProvidersListModelsResult> {
  if (!isTauri()) {
    return { models: [], source: "browser-dev" };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProvidersListModelsResult>("providers_list_models", { request });
}

export async function startEngine(options?: { privateBrain?: boolean }): Promise<DaemonStartResult> {
  if (!isTauri()) {
    return {
      started: false,
      alreadyRunning: false,
      logPath: "",
      message: "Open the Medousa app (browser dev mode cannot start the engine)",
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DaemonStartResult>("daemon_start", {
    request: { privateBrain: options?.privateBrain ?? false },
  });
}

/** @deprecated Use startEngine */
export const startDaemonCore = startEngine;

export async function waitForEngine(
  timeoutSeconds = 30,
): Promise<DaemonWaitHealthResult> {
  if (!isTauri()) {
    return { ok: false, message: "Unavailable in browser dev mode", attempts: 0 };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DaemonWaitHealthResult>("daemon_wait_healthy", {
    request: { timeoutSeconds, pollMs: 2000 },
  });
}

/** @deprecated Use waitForEngine */
export const waitForDaemonCore = waitForEngine;

/** Start the engine and block until health is OK — throws if startup fails. */
export async function requireEngineReady(options?: {
  privateBrain?: boolean;
  timeoutSeconds?: number;
}): Promise<DaemonWaitHealthResult> {
  await startEngine({ privateBrain: options?.privateBrain ?? false });
  const health = await waitForEngine(options?.timeoutSeconds ?? 45);
  if (!health.ok) {
    throw new Error(health.message || "Medousa engine did not start");
  }
  return health;
}

export async function restartEngine(options?: {
  privateBrain?: boolean;
}): Promise<DaemonStartResult> {
  if (!isTauri()) {
    return {
      started: false,
      alreadyRunning: false,
      logPath: "",
      message: "Unavailable in browser dev mode",
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DaemonStartResult>("daemon_restart", {
    request: { privateBrain: options?.privateBrain ?? false },
  });
}

export async function applyWizardScreen1(
  request: WizardApplyScreen1Request,
): Promise<WizardApplyScreen1Result> {
  if (!isTauri()) {
    return {
      coreReady: false,
      coreMessage: "Saved locally in dev mode — open the Medousa app to start the engine",
      provider: request.provider,
      model: request.model,
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<WizardApplyScreen1Result>("wizard_apply_screen1", { request });
}
