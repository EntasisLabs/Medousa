export interface InferenceTarget {
  provider: string;
  model: string;
  baseUrl?: string | null;
}

export interface InferenceProfile {
  provider: string;
  model: string;
  baseUrl?: string | null;
  fallbacks?: InferenceTarget[];
}

export interface InferenceProfiles {
  main?: InferenceProfile | null;
  vision?: InferenceProfile | null;
  stt?: InferenceProfile | null;
}

export function emptyInferenceProfiles(): InferenceProfiles {
  return { main: null, vision: null, stt: null };
}

export function profileReady(profile?: InferenceProfile | null): boolean {
  return Boolean(profile?.provider?.trim() && profile?.model?.trim());
}

export function visionProfileReady(profiles?: InferenceProfiles | null): boolean {
  return profileReady(profiles?.vision);
}

export function normalizeInferenceProfiles(
  raw: InferenceProfiles | null | undefined,
  defaults: {
    provider: string;
    model: string;
    baseUrl?: string | null;
    sttProvider: string;
    sttModel: string;
    sttBaseUrl?: string | null;
  },
): InferenceProfiles {
  const main =
    profileReady(raw?.main)
      ? {
          provider: raw!.main!.provider.trim(),
          model: raw!.main!.model.trim(),
          baseUrl: raw!.main!.baseUrl?.trim() || null,
          fallbacks: raw!.main!.fallbacks ?? [],
        }
      : {
          provider: defaults.provider,
          model: defaults.model,
          baseUrl: defaults.baseUrl?.trim() || null,
          fallbacks: [],
        };

  const stt =
    profileReady(raw?.stt)
      ? {
          provider: raw!.stt!.provider.trim(),
          model: raw!.stt!.model.trim(),
          baseUrl: raw!.stt!.baseUrl?.trim() || null,
          fallbacks: raw!.stt!.fallbacks ?? [],
        }
      : {
          provider: defaults.sttProvider,
          model: defaults.sttModel,
          baseUrl: defaults.sttBaseUrl?.trim() || null,
          fallbacks: [],
        };

  const vision = profileReady(raw?.vision)
    ? {
        provider: raw!.vision!.provider.trim(),
        model: raw!.vision!.model.trim(),
        baseUrl: raw!.vision!.baseUrl?.trim() || null,
        fallbacks: raw!.vision!.fallbacks ?? [],
      }
    : null;

  return { main, vision, stt };
}

export function syncFlatFieldsFromProfiles(
  draft: import("$lib/types/workshopDefaults").TuiDefaults,
): import("$lib/types/workshopDefaults").TuiDefaults {
  const profiles = draft.inferenceProfiles;
  if (!profiles) return draft;
  return {
    ...draft,
    provider: profiles.main?.provider ?? draft.provider,
    model: profiles.main?.model ?? draft.model,
    baseUrl: profiles.main?.baseUrl ?? draft.baseUrl,
    sttProvider: profiles.stt?.provider ?? draft.sttProvider,
    sttModel: profiles.stt?.model ?? draft.sttModel,
    sttBaseUrl: profiles.stt?.baseUrl ?? draft.sttBaseUrl,
  };
}
