import type { ContextUsageLayer, ContextUsageReport } from "$lib/types/chat";

/** Stable colors for segmented bar + legend (Medousa layer ids). */
export const CONTEXT_LAYER_COLORS: Record<string, string> = {
  system_prompt: "#9ca3af",
  tool_definitions: "#a855f7",
  ambient: "#34d399",
  memory_identity: "#22c55e",
  tool_policy: "#f97316",
  tool_hints: "#fb923c",
  tool_slices: "#f59e0b",
  grapheme_scripts: "#38bdf8",
  runtime_learnings: "#f472b6",
  cold_history: "#f43f5e",
  prior_conversation: "#60a5fa",
  user_message: "#818cf8",
};

const FALLBACK_LAYER_COLOR = "#64748b";

export function layerColor(layerId: string): string {
  return CONTEXT_LAYER_COLORS[layerId] ?? FALLBACK_LAYER_COLOR;
}

export function formatTokenCount(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 10_000) {
    return `${(value / 1_000).toFixed(1)}k`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}k`;
  }
  return String(value);
}

export function usageFillPercent(report: ContextUsageReport): number | null {
  const limit = report.context_limit_tokens;
  if (!limit || limit <= 0) return null;
  return Math.min(100, Math.round((report.total_tokens_estimate / limit) * 100));
}

export function usageSummaryLine(report: ContextUsageReport): string {
  const total = formatTokenCount(report.total_tokens_estimate);
  const limit = report.context_limit_tokens
    ? formatTokenCount(report.context_limit_tokens)
    : null;
  const pct = usageFillPercent(report);
  if (pct != null && limit) {
    return `${pct}% full · ~${total} / ${limit} tokens`;
  }
  return `~${total} tokens (${report.estimator})`;
}

export interface ContextUsageSegment {
  layer: ContextUsageLayer;
  widthPct: number;
  color: string;
}

export function contextUsageSegments(report: ContextUsageReport): ContextUsageSegment[] {
  const total = report.total_tokens_estimate || 1;
  return report.layers.map((layer) => ({
    layer,
    widthPct: (layer.tokens_estimate / total) * 100,
    color: layerColor(layer.id),
  }));
}
