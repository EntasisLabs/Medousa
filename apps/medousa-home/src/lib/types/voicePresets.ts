export interface VoicePreset {
  id: string;
  name: string;
  description?: string;
  voiceAppendix: string;
  builtin?: boolean;
}

export const DEFAULT_VOICE_ID = "default";

export const BUILTIN_VOICE_PRESETS: VoicePreset[] = [
  {
    id: "default",
    name: "Default",
    description: "Runtime collaborator voice — no extra stance",
    voiceAppendix: "",
    builtin: true,
  },
  {
    id: "direct",
    name: "Direct",
    description: "Action-first — lead with the answer or next move",
    voiceAppendix:
      "Lead with the answer or next move. Be concise and action-first. Skip preamble, hedging, and recap unless uncertainty is material or the principal asked for reasoning.",
    builtin: true,
  },
];

export const MAX_CUSTOM_VOICE_PRESETS = 8;

export function normalizeCustomVoicePresets(
  raw: VoicePreset[] | null | undefined,
): VoicePreset[] {
  if (!raw?.length) return [];
  const seen = new Set<string>();
  const next: VoicePreset[] = [];
  for (const entry of raw) {
    const id = entry.id?.trim();
    const name = entry.name?.trim();
    const voiceAppendix = entry.voiceAppendix?.trim() ?? "";
    if (!id || !name || !voiceAppendix || seen.has(id)) continue;
    if (BUILTIN_VOICE_PRESETS.some((preset) => preset.id === id)) continue;
    seen.add(id);
    next.push({
      id,
      name,
      description: entry.description?.trim() || undefined,
      voiceAppendix,
    });
    if (next.length >= MAX_CUSTOM_VOICE_PRESETS) break;
  }
  return next;
}

export function allVoicePresets(custom: VoicePreset[] | null | undefined): VoicePreset[] {
  return [...BUILTIN_VOICE_PRESETS, ...normalizeCustomVoicePresets(custom)];
}

export function resolveVoicePreset(
  voiceId: string | null | undefined,
  custom: VoicePreset[] | null | undefined,
): VoicePreset {
  const trimmed = voiceId?.trim();
  const presets = allVoicePresets(custom);
  return presets.find((preset) => preset.id === trimmed) ?? presets[0];
}

export function voicePresetLabel(
  voiceId: string | null | undefined,
  custom: VoicePreset[] | null | undefined,
): string {
  return resolveVoicePreset(voiceId, custom).name;
}

export function slugifyVoicePresetId(name: string): string {
  const base = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return base || "voice";
}

export function uniqueVoicePresetId(name: string, existingIds: Set<string>): string {
  const base = slugifyVoicePresetId(name);
  if (!existingIds.has(base)) return base;
  for (let index = 2; index < 100; index += 1) {
    const candidate = `${base}-${index}`;
    if (!existingIds.has(candidate)) return candidate;
  }
  return `${base}-${Date.now()}`;
}
