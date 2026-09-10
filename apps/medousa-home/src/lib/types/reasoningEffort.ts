export type ReasoningEffortMode =
  | `budget:${number}`
  | "none"
  | "default"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max";

export interface ReasoningEffortOption {
  id: ReasoningEffortMode;
  label: string;
  hint: string;
}

export const REASONING_EFFORT_OPTIONS: ReasoningEffortOption[] = [
  { id: "default", label: "Model default", hint: "Use the model’s default reasoning" },
  { id: "none", label: "Off", hint: "Disable reasoning" },
  { id: "minimal", label: "Minimal", hint: "Lightest native reasoning" },
  { id: "low", label: "Low", hint: "Fast reasoning, lower cost" },
  { id: "medium", label: "Medium", hint: "Balanced reasoning" },
  { id: "high", label: "High", hint: "Deeper reasoning" },
  { id: "xhigh", label: "Extra high", hint: "Extra reasoning depth" },
  { id: "max", label: "Max", hint: "Maximum supported reasoning depth" },
];

export function normalizeReasoningEffort(value: string | null | undefined): ReasoningEffortMode {
  const normalized = (value ?? "default").trim().toLowerCase();
  if (/^budget:\d+$/.test(normalized) && Number.isSafeInteger(Number(normalized.slice(7)))) return normalized as ReasoningEffortMode;
  if (REASONING_EFFORT_OPTIONS.some((option) => option.id === normalized)) {
    return normalized as ReasoningEffortMode;
  }
  return "default";
}

export function reasoningEffortLabel(mode: string): string {
  if (normalizeReasoningEffort(mode).startsWith("budget:")) return `${Number(mode.slice(7)).toLocaleString()} tokens`;
  return (
    REASONING_EFFORT_OPTIONS.find((option) => option.id === normalizeReasoningEffort(mode))
      ?.label ?? "Default"
  );
}

export interface ReasoningCapability {
  kind: "effort" | "budget" | "unsupported" | "unknown";
  levels: ReasoningEffortMode[];
  budgetMin: number | null;
  budgetMax: number | null;
  defaultLevel: string | null;
  source: string;
}
export const UNKNOWN_REASONING: ReasoningCapability = {
  kind: "unknown", levels: [], budgetMin: null, budgetMax: null, defaultLevel: null, source: "unknown",
};

export function normalizeReasoningCapability(raw: unknown): ReasoningCapability {
  if (!raw || typeof raw !== "object") return UNKNOWN_REASONING;
  const value = raw as Record<string, unknown>;
  if (!["effort", "budget", "unsupported", "unknown"].includes(String(value.kind))) return UNKNOWN_REASONING;
  const levels = Array.isArray(value.levels) ? [...new Set(value.levels.filter((level): level is ReasoningEffortMode =>
    typeof level === "string" && level !== "default" && REASONING_EFFORT_OPTIONS.some((option) => option.id === level)))] : [];
  const min = value.budgetMin;
  const max = value.budgetMax;
  if (value.kind === "budget" && !(typeof min === "number" && typeof max === "number"
    && Number.isSafeInteger(min) && Number.isSafeInteger(max) && min >= 0 && max >= min)) return UNKNOWN_REASONING;
  return { kind: value.kind as ReasoningCapability["kind"], levels,
    budgetMin: typeof min === "number" ? min : null, budgetMax: typeof max === "number" ? max : null,
    defaultLevel: typeof value.defaultLevel === "string" ? value.defaultLevel : null,
    source: typeof value.source === "string" ? value.source : "unknown" };
}

export function compatibleReasoning(value: string, capability: ReasoningCapability): ReasoningEffortMode {
  const normalized = normalizeReasoningEffort(value);
  if (normalized === "default") return normalized;
  if (capability.kind === "effort" && capability.levels.includes(normalized)) return normalized;
  if (capability.kind === "budget" && normalized.startsWith("budget:")) {
    const tokens = Number(normalized.slice(7));
    if (tokens >= capability.budgetMin! && tokens <= capability.budgetMax!) return normalized;
  }
  return "default";
}
