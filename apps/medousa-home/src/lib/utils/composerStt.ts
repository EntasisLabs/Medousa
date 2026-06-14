/** Composer voice transcription via provider Whisper API (Tauri backend). */

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
  return invoke<ComposerSttStatus>("composer_stt_status");
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
