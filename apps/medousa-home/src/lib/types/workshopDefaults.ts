import type { DepthMode, StageRoutingMatrix } from "$lib/types/runtime";

export type WorkshopDefaultsTab =
  | "setup"
  | "tools"
  | "memory"
  | "diagnostics"
  | "quality"
  | "secrets"
  | "specialists";

export interface TuiDefaults {
  backend?: string | null;
  themeId?: string | null;
  provider?: string | null;
  model?: string | null;
  baseUrl?: string | null;
  envOverrides?: string | null;
  allowedModules?: string[] | null;
  toolCallMode?: string | null;
  maxToolRounds?: number | null;
  hostBusMaxToolRounds?: number | null;
  hostTurnBusMode?: string | null;
  activationToolIntentMaxRounds?: number | null;
  activationShortTurnMaxToolRounds?: number | null;
  continuationMaxToolRounds?: number | null;
  maxTextOnlyStuckContinues?: number | null;
  classifierRestrictedMaxToolRounds?: number | null;
  thinkingCapture?: boolean | null;
  stasisOtelEnabled?: boolean | null;
  thinkingMaxLines?: number | null;
  activationDirectAnswerMaxPromptChars?: number | null;
  activationLongSessionTurnThreshold?: number | null;
  activationLongSessionMaxPromptChars?: number | null;
  sliceHotWindowTurns?: number | null;
  sliceColdWindowTurns?: number | null;
  retryRuntimeMaxRetries?: number | null;
  retryRuntimeMaxRounds?: number | null;
  verifierMinCitationCoverage?: number | null;
  verifierMinAvgSupportStrength?: number | null;
  verifierMinSupportedClaimRatio?: number | null;
  verifierMinClaimSupportStrength?: number | null;
  responseDepthMode?: string | null;
  stageRouting?: StageRoutingMatrix | null;
  webSearchPreferredProvider?: string | null;
  webSearchTryFallbacks?: boolean | null;
  workCardHideAfterHours?: number | null;
  workCardWipeAfterDays?: number | null;
}

export const WORKSHOP_DEFAULTS_TABS: {
  id: WorkshopDefaultsTab;
  label: string;
}[] = [
  { id: "setup", label: "Setup" },
  { id: "tools", label: "Tools" },
  { id: "memory", label: "Memory" },
  { id: "diagnostics", label: "Diagnostics" },
  { id: "quality", label: "Quality" },
  { id: "secrets", label: "Secrets" },
  { id: "specialists", label: "Specialists" },
];

export const BACKEND_OPTIONS = ["surreal-mem", "in-memory", "surreal-kv"] as const;
export const TOOL_CALL_MODE_OPTIONS = ["auto", "strict"] as const;
export const HOST_TURN_BUS_OPTIONS = ["auto", "force", "off"] as const;
export const DEPTH_OPTIONS: DepthMode[] = ["concise", "standard", "deep"];

export const WEB_SEARCH_PROVIDER_OPTIONS = [
  { value: "", label: "Auto (capability order)" },
  { value: "duckduckgo", label: "DuckDuckGo" },
  { value: "google", label: "Google" },
  { value: "tavily", label: "Tavily" },
] as const;

export function defaultWorkshopDefaults(): Required<
  Pick<
    TuiDefaults,
    | "backend"
    | "provider"
    | "model"
    | "toolCallMode"
    | "hostTurnBusMode"
    | "maxToolRounds"
    | "hostBusMaxToolRounds"
    | "activationToolIntentMaxRounds"
    | "activationShortTurnMaxToolRounds"
    | "continuationMaxToolRounds"
    | "maxTextOnlyStuckContinues"
    | "classifierRestrictedMaxToolRounds"
    | "thinkingCapture"
    | "stasisOtelEnabled"
    | "thinkingMaxLines"
    | "activationDirectAnswerMaxPromptChars"
    | "activationLongSessionTurnThreshold"
    | "activationLongSessionMaxPromptChars"
    | "sliceHotWindowTurns"
    | "sliceColdWindowTurns"
    | "retryRuntimeMaxRetries"
    | "retryRuntimeMaxRounds"
    | "verifierMinCitationCoverage"
    | "verifierMinAvgSupportStrength"
    | "verifierMinSupportedClaimRatio"
    | "verifierMinClaimSupportStrength"
    | "responseDepthMode"
  >
> {
  return {
    backend: "surreal-mem",
    provider: "ollama",
    model: "qwen2.5:7b",
    toolCallMode: "auto",
    hostTurnBusMode: "auto",
    maxToolRounds: 10,
    hostBusMaxToolRounds: 8,
    activationToolIntentMaxRounds: 4,
    activationShortTurnMaxToolRounds: 6,
    continuationMaxToolRounds: 8,
    maxTextOnlyStuckContinues: 2,
    classifierRestrictedMaxToolRounds: 4,
    thinkingCapture: true,
    stasisOtelEnabled: false,
    thinkingMaxLines: 300,
    activationDirectAnswerMaxPromptChars: 320,
    activationLongSessionTurnThreshold: 28,
    activationLongSessionMaxPromptChars: 420,
    sliceHotWindowTurns: 8,
    sliceColdWindowTurns: 24,
    retryRuntimeMaxRetries: 1,
    retryRuntimeMaxRounds: 12,
    verifierMinCitationCoverage: 0.6,
    verifierMinAvgSupportStrength: 0.7,
    verifierMinSupportedClaimRatio: 0.6,
    verifierMinClaimSupportStrength: 0.65,
    responseDepthMode: "standard",
  };
}

export function normalizeWorkshopDefaults(raw: TuiDefaults): TuiDefaults {
  const defaults = defaultWorkshopDefaults();
  return {
    backend: raw.backend?.trim() || defaults.backend,
    themeId: raw.themeId?.trim() || "medousa-default",
    provider: raw.provider?.trim() || defaults.provider,
    model: raw.model?.trim() || defaults.model,
    baseUrl: raw.baseUrl?.trim() || "",
    envOverrides: raw.envOverrides ?? "",
    allowedModules: raw.allowedModules ?? [],
    toolCallMode: raw.toolCallMode?.trim() || defaults.toolCallMode,
    maxToolRounds: raw.maxToolRounds ?? defaults.maxToolRounds,
    hostBusMaxToolRounds: raw.hostBusMaxToolRounds ?? defaults.hostBusMaxToolRounds,
    hostTurnBusMode: raw.hostTurnBusMode?.trim() || defaults.hostTurnBusMode,
    activationToolIntentMaxRounds:
      raw.activationToolIntentMaxRounds ?? defaults.activationToolIntentMaxRounds,
    activationShortTurnMaxToolRounds:
      raw.activationShortTurnMaxToolRounds ??
      defaults.activationShortTurnMaxToolRounds,
    continuationMaxToolRounds:
      raw.continuationMaxToolRounds ?? defaults.continuationMaxToolRounds,
    maxTextOnlyStuckContinues:
      raw.maxTextOnlyStuckContinues ?? defaults.maxTextOnlyStuckContinues,
    classifierRestrictedMaxToolRounds:
      raw.classifierRestrictedMaxToolRounds ??
      defaults.classifierRestrictedMaxToolRounds,
    thinkingCapture: raw.thinkingCapture ?? defaults.thinkingCapture,
    stasisOtelEnabled: raw.stasisOtelEnabled ?? defaults.stasisOtelEnabled,
    thinkingMaxLines: raw.thinkingMaxLines ?? defaults.thinkingMaxLines,
    activationDirectAnswerMaxPromptChars:
      raw.activationDirectAnswerMaxPromptChars ??
      defaults.activationDirectAnswerMaxPromptChars,
    activationLongSessionTurnThreshold:
      raw.activationLongSessionTurnThreshold ??
      defaults.activationLongSessionTurnThreshold,
    activationLongSessionMaxPromptChars:
      raw.activationLongSessionMaxPromptChars ??
      defaults.activationLongSessionMaxPromptChars,
    sliceHotWindowTurns: raw.sliceHotWindowTurns ?? defaults.sliceHotWindowTurns,
    sliceColdWindowTurns: raw.sliceColdWindowTurns ?? defaults.sliceColdWindowTurns,
    retryRuntimeMaxRetries: raw.retryRuntimeMaxRetries ?? defaults.retryRuntimeMaxRetries,
    retryRuntimeMaxRounds: raw.retryRuntimeMaxRounds ?? defaults.retryRuntimeMaxRounds,
    verifierMinCitationCoverage:
      raw.verifierMinCitationCoverage ?? defaults.verifierMinCitationCoverage,
    verifierMinAvgSupportStrength:
      raw.verifierMinAvgSupportStrength ?? defaults.verifierMinAvgSupportStrength,
    verifierMinSupportedClaimRatio:
      raw.verifierMinSupportedClaimRatio ?? defaults.verifierMinSupportedClaimRatio,
    verifierMinClaimSupportStrength:
      raw.verifierMinClaimSupportStrength ?? defaults.verifierMinClaimSupportStrength,
    responseDepthMode: raw.responseDepthMode?.trim() || defaults.responseDepthMode,
    stageRouting: raw.stageRouting ?? null,
    webSearchPreferredProvider: raw.webSearchPreferredProvider?.trim() || "",
    webSearchTryFallbacks: raw.webSearchTryFallbacks ?? true,
    workCardHideAfterHours: raw.workCardHideAfterHours ?? 24,
    workCardWipeAfterDays: raw.workCardWipeAfterDays ?? 7,
  };
}

export function allowedModulesToText(modules: string[] | null | undefined): string {
  return (modules ?? []).join(", ");
}

export function parseAllowedModulesText(value: string): string[] {
  return value
    .split(/[,\n]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}
