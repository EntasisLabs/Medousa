export type ReasoningEffortMode =
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
  { id: "default", label: "Default", hint: "Provider decides reasoning depth" },
  { id: "minimal", label: "Minimal", hint: "Lightest native reasoning" },
  { id: "low", label: "Low", hint: "Fast reasoning, lower cost" },
  { id: "medium", label: "Medium", hint: "Balanced reasoning" },
  { id: "high", label: "High", hint: "Deeper reasoning" },
  { id: "xhigh", label: "Extra high", hint: "OpenAI-class extra depth" },
  { id: "max", label: "Max", hint: "Anthropic-class maximum depth" },
];

export function normalizeReasoningEffort(value: string | null | undefined): ReasoningEffortMode {
  const normalized = (value ?? "default").trim().toLowerCase();
  if (REASONING_EFFORT_OPTIONS.some((option) => option.id === normalized)) {
    return normalized as ReasoningEffortMode;
  }
  return "default";
}

export function reasoningEffortLabel(mode: string): string {
  return (
    REASONING_EFFORT_OPTIONS.find((option) => option.id === normalizeReasoningEffort(mode))
      ?.label ?? "Default"
  );
}
