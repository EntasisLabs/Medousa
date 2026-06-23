import type { InferenceProfile, InferenceProfiles, InferenceTarget } from "$lib/types/inferenceProfiles";
import { syncFlatFieldsFromProfiles } from "$lib/types/inferenceProfiles";
import type { TuiDefaults } from "$lib/types/workshopDefaults";
import { defaultSttModel } from "$lib/types/workshopDefaults";
import { resolveModelDisplayLabel } from "$lib/utils/modelCatalog";
import { resolveProviderLabel } from "$lib/utils/chatModelPicker";
import { customProviderHint } from "$lib/utils/customProvider";
import type { ProvidersListResult } from "$lib/types/providers";

export type ProfileKind = "main" | "vision" | "stt";

export type ModelPickerTarget =
  | { type: "favorite-add" }
  | { type: "primary"; profile: ProfileKind }
  | { type: "fallback"; profile: ProfileKind; index: 0 | 1 };

export function pickerTitle(target: ModelPickerTarget): string {
  switch (target.type) {
    case "favorite-add":
      return "Add favorite";
    case "primary":
      if (target.profile === "main") return "Chat model";
      if (target.profile === "vision") return "Vision model";
      return "Dictation model";
    case "fallback":
      return `${profileLabel(target.profile)} backup ${target.index + 1}`;
  }
}

export function pickerRequiresVision(target: ModelPickerTarget): boolean {
  if (target.type === "primary" && target.profile === "vision") return true;
  if (target.type === "fallback" && target.profile === "vision") return true;
  return false;
}

export function pickerAllowsClear(target: ModelPickerTarget): boolean {
  return target.type === "fallback";
}

function profileLabel(profile: ProfileKind): string {
  if (profile === "main") return "Chat";
  if (profile === "vision") return "Vision";
  return "Dictation";
}

function mergeProfiles(
  draft: TuiDefaults,
  patch: Partial<InferenceProfiles>,
): InferenceProfiles {
  return {
    main: draft.inferenceProfiles?.main ?? null,
    vision: draft.inferenceProfiles?.vision ?? null,
    stt: draft.inferenceProfiles?.stt ?? null,
    ...patch,
  };
}

export function profileForKind(
  draft: TuiDefaults,
  kind: ProfileKind,
): InferenceProfile | null {
  const profiles = draft.inferenceProfiles;
  if (kind === "main") {
    return (
      profiles?.main ?? {
        provider: draft.provider ?? "deepseek",
        model: draft.model ?? "",
        baseUrl: draft.baseUrl ?? null,
        fallbacks: [],
      }
    );
  }
  if (kind === "vision") {
    return profiles?.vision ?? null;
  }
  return (
    profiles?.stt ?? {
      provider: draft.sttProvider ?? "openai",
      model: draft.sttModel ?? defaultSttModel(draft.sttProvider ?? "openai"),
      baseUrl: draft.sttBaseUrl ?? null,
      fallbacks: [],
    }
  );
}

export function selectionForTarget(
  draft: TuiDefaults,
  target: ModelPickerTarget,
): InferenceTarget | null {
  if (target.type === "favorite-add") return null;
  if (target.type === "primary") {
    const profile = profileForKind(draft, target.profile);
    if (!profile?.provider?.trim() || !profile.model?.trim()) return null;
    return {
      provider: profile.provider,
      model: profile.model,
      baseUrl: profile.baseUrl ?? null,
    };
  }
  const profile = profileForKind(draft, target.profile);
  return profile?.fallbacks?.[target.index] ?? null;
}

export function rowLabelForTarget(
  draft: TuiDefaults,
  target: ModelPickerTarget,
  catalog: ProvidersListResult | null,
): { title: string; value: string; hint: string | null } {
  if (target.type === "favorite-add") {
    return { title: "Add favorite", value: "", hint: null };
  }
  const title =
    target.type === "primary"
      ? pickerTitle(target)
      : `${profileLabel(target.profile)} backup ${target.index + 1}`;
  const selection = selectionForTarget(draft, target);
  if (!selection) {
    return { title, value: "Not set", hint: null };
  }
  const providerHint =
    customProviderHint(selection.baseUrl) ??
    resolveProviderLabel(catalog, selection.provider);
  return {
    title,
    value: resolveModelDisplayLabel(selection.provider, selection.model),
    hint: providerHint,
  };
}

export function applyModelSelection(
  draft: TuiDefaults,
  target: ModelPickerTarget,
  selection: InferenceTarget | null,
): TuiDefaults {
  if (target.type === "favorite-add") return draft;

  if (target.type === "primary") {
    if (!selection) return draft;
    const nextProfile: InferenceProfile = {
      provider: selection.provider,
      model: selection.model,
      baseUrl: selection.baseUrl ?? null,
      fallbacks: profileForKind(draft, target.profile)?.fallbacks ?? [],
    };
    const patch: Partial<InferenceProfiles> =
      target.profile === "main"
        ? { main: nextProfile }
        : target.profile === "vision"
          ? { vision: nextProfile }
          : { stt: nextProfile };
    return syncFlatFieldsFromProfiles({
      ...draft,
      inferenceProfiles: mergeProfiles(draft, patch),
    });
  }

  const profile = profileForKind(draft, target.profile);
  if (!profile) return draft;
  const fallbacks = [...(profile.fallbacks ?? [])];
  while (fallbacks.length <= target.index) {
    fallbacks.push({ provider: "", model: "", baseUrl: null });
  }
  if (selection) {
    fallbacks[target.index] = selection;
  } else {
    fallbacks.splice(target.index, 1);
  }
  const trimmed = fallbacks.filter(
    (entry) => entry.provider.trim() && entry.model.trim(),
  );
  const nextProfile = { ...profile, fallbacks: trimmed };
  const patch: Partial<InferenceProfiles> =
    target.profile === "main"
      ? { main: nextProfile }
      : target.profile === "vision"
        ? { vision: nextProfile }
        : { stt: nextProfile };
  return syncFlatFieldsFromProfiles({
    ...draft,
    inferenceProfiles: mergeProfiles(draft, patch),
  });
}

export const PRIMARY_TARGETS: ModelPickerTarget[] = [
  { type: "primary", profile: "main" },
  { type: "primary", profile: "vision" },
  { type: "primary", profile: "stt" },
];

export function fallbackTargets(profile: ProfileKind): ModelPickerTarget[] {
  return [
    { type: "fallback", profile, index: 0 },
    { type: "fallback", profile, index: 1 },
  ];
}

export function providerIdsForTarget(target: ModelPickerTarget): string[] | null {
  if (target.type === "primary" && target.profile === "stt") {
    return ["openai", "groq"];
  }
  return null;
}

export function excludedProvidersForTarget(target: ModelPickerTarget): string[] {
  if (target.type === "primary" && target.profile === "stt") {
    return ["medousa-local", "ollama"];
  }
  return ["medousa-local"];
}
