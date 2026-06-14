/** Waveform helpers for composer voice UI. */

export function formatVoiceElapsed(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export const VOICE_WAVE_BAR_COUNT = 52;

export function idleVoiceWaveform(): number[] {
  return Array.from({ length: VOICE_WAVE_BAR_COUNT }, () => 0.1);
}

export function pushVoiceWaveSample(levels: number[], level: number): number[] {
  const next = levels.slice(1);
  next.push(Math.max(0.08, Math.min(1, level * 1.15 + 0.06)));
  return next;
}

export function voiceWaveLevelFromMic(level: number): number {
  if (level <= 0.02) return 0.08 + Math.random() * 0.04;
  return Math.max(0.12, Math.min(1, level * 1.8));
}
