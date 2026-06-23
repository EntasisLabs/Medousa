/** Composer voice transcription via daemon STT profile (Tauri → workshop daemon). */

import { isTauri } from "$lib/window";

export interface ComposerSttStatus {
  available: boolean;
  reason: string | null;
}

export function appendComposerDraft(existing: string, spoken: string): string {
  const next = spoken.trim();
  if (!next) return existing;
  const base = existing.trimEnd();
  if (!base) return next;
  return `${base} ${next}`;
}

export async function composerSttStatus(): Promise<ComposerSttStatus> {
  if (!isTauri()) {
    return {
      available: false,
      reason: "Voice transcription requires the Medousa app.",
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<ComposerSttStatus>("composer_stt_status");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (message.includes("404")) {
      return {
        available: false,
        reason: "Dictation requires a newer workshop daemon.",
      };
    }
    return {
      available: false,
      reason: message,
    };
  }
}

export async function transcribeComposerAudio(blob: Blob): Promise<string> {
  if (!isTauri()) {
    throw new Error("Voice transcription requires the Medousa app.");
  }
  if (blob.size === 0) {
    throw new Error("Recording was empty — try again.");
  }

  const bytes = new Uint8Array(await blob.arrayBuffer());
  const { invoke } = await import("@tauri-apps/api/core");
  const result = await invoke<{ text: string }>("composer_stt_transcribe", {
    request: {
      audioBytes: Array.from(bytes),
      mimeType: blob.type || "audio/webm",
    },
  });
  return result.text.trim();
}
