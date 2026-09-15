import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

const STORAGE_KEY = "medousa-siri-preferences-v1";

export type SiriSpeechMode = "auto" | "always" | "never";

export type SiriPreferences = {
  speechMode: SiriSpeechMode;
  maxSpokenCharacters: number;
  defaultWorkshopId: string | null;
  defaultSessionId: string | null;
  fastResponseModel: string | null;
};

export const DEFAULT_SIRI_PREFERENCES: SiriPreferences = {
  speechMode: "auto",
  maxSpokenCharacters: 320,
  defaultWorkshopId: null,
  defaultSessionId: null,
  fastResponseModel: null,
};

let memoryPreferences = { ...DEFAULT_SIRI_PREFERENCES };

function speechMode(value: unknown): SiriSpeechMode {
  return value === "always" || value === "never" ? value : "auto";
}

function optionalText(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim().slice(0, 256) : null;
}

export function readSiriPreferences(): SiriPreferences {
  if (typeof localStorage === "undefined") return { ...memoryPreferences };
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<SiriPreferences>;
    return {
      speechMode: speechMode(parsed.speechMode),
      maxSpokenCharacters:
        typeof parsed.maxSpokenCharacters === "number"
          ? Math.round(Math.min(1_000, Math.max(80, parsed.maxSpokenCharacters)))
          : DEFAULT_SIRI_PREFERENCES.maxSpokenCharacters,
      defaultWorkshopId: optionalText(parsed.defaultWorkshopId),
      defaultSessionId: optionalText(parsed.defaultSessionId),
      fastResponseModel: optionalText(parsed.fastResponseModel),
    };
  } catch {
    return { ...memoryPreferences };
  }
}

async function syncNative(preferences: SiriPreferences): Promise<void> {
  if (!isTauri()) return;
  await invoke("siri_sync_preferences", { preferences });
}

export async function writeSiriPreferences(
  patch: Partial<SiriPreferences>,
): Promise<SiriPreferences> {
  const current = readSiriPreferences();
  const next = { ...current, ...patch };
  memoryPreferences = { ...next };
  if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  await syncNative(next);
  return next;
}

export async function syncSiriPreferences(): Promise<void> {
  await syncNative(readSiriPreferences());
}
