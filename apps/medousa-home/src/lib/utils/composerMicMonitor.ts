/** Waveform helpers for composer voice UI. */

export function formatVoiceElapsed(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

/** Rolling mic samples — not every bar is rendered; see `displayVoiceWaveform`. */
export const VOICE_WAVE_SAMPLE_COUNT = 24;

/** @deprecated use VOICE_WAVE_SAMPLE_COUNT */
export const VOICE_WAVE_BAR_COUNT = VOICE_WAVE_SAMPLE_COUNT;

export function idleVoiceWaveform(): number[] {
  return Array.from({ length: VOICE_WAVE_SAMPLE_COUNT }, () => 0.08);
}

export function pushVoiceWaveSample(levels: number[], level: number): number[] {
  const next = levels.slice(1);
  next.push(Math.max(0.08, Math.min(1, level * 1.12 + 0.05)));
  return next;
}

export function voiceWaveLevelFromMic(level: number): number {
  if (level <= 0.02) return 0.08 + Math.random() * 0.03;
  return Math.max(0.1, Math.min(1, level * 1.65));
}

/** Symmetric waveform for a centered, iMessage-style meter. */
export function displayVoiceWaveform(levels: number[], barCount = 32): number[] {
  const half = Math.floor(barCount / 2);
  const recent = levels.slice(-half);
  const pad = half - recent.length;
  const core =
    pad > 0
      ? [...Array<number>(pad).fill(0.08), ...recent]
      : recent.slice(-half);
  return [...core, ...core.slice().reverse()];
}
