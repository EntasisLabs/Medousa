import { invoke } from "@tauri-apps/api/core";
import { isTauriIos } from "$lib/platform";

export type LiveVoicePhase =
  | "idle"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking"
  | "muted"
  | "failed";

export interface LiveVoiceStatus {
  available: boolean;
  active: boolean;
  muted: boolean;
  phase: LiveVoicePhase;
  workshopName?: string | null;
  sessionId?: string | null;
  error?: string | null;
}

const unavailable: LiveVoiceStatus = {
  available: false,
  active: false,
  muted: false,
  phase: "idle",
  error: "Medousa Live voice sessions require the iOS app.",
};

export async function liveVoiceStart(
  workshopName: string,
  sessionId: string,
): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_start", { workshopName, sessionId });
}

export async function liveVoiceSetMuted(muted: boolean): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_set_muted", { muted });
}

export async function liveVoiceStop(): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_stop");
}

export async function liveVoiceStatus(): Promise<LiveVoiceStatus> {
  if (!isTauriIos()) return unavailable;
  return invoke<LiveVoiceStatus>("live_voice_status");
}
