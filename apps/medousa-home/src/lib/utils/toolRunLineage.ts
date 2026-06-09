import type { ToolRunState } from "$lib/types/chat";
import { formatToolName } from "$lib/utils/formatTurn";

export interface ToolLineageSegment {
  key: string;
  toolName: string;
  displayName: string;
  count: number;
  runs: ToolRunState[];
  status: ToolRunState["status"];
  roundMin: number;
  roundMax: number;
}

function segmentStatus(runs: ToolRunState[]): ToolRunState["status"] {
  if (runs.some((run) => run.status === "running")) return "running";
  if (runs.some((run) => run.status === "failed")) return "failed";
  return "succeeded";
}

/** Collapse consecutive runs of the same tool into lineage steps (execution order). */
export function buildToolLineage(runs: ToolRunState[]): ToolLineageSegment[] {
  const segments: ToolLineageSegment[] = [];

  for (const run of runs) {
    const last = segments[segments.length - 1];
    if (last && last.toolName === run.toolName) {
      last.runs.push(run);
      last.count += 1;
      last.status = segmentStatus(last.runs);
      last.roundMax = run.round;
      continue;
    }

    segments.push({
      key: `${run.toolName}-${run.runId}`,
      toolName: run.toolName,
      displayName: formatToolName(run.toolName),
      count: 1,
      runs: [run],
      status: run.status,
      roundMin: run.round,
      roundMax: run.round,
    });
  }

  return segments;
}

export function formatSegmentLabel(segment: ToolLineageSegment): string {
  if (segment.count === 1) return segment.displayName;
  return `${segment.displayName} ×${segment.count}`;
}

/** Minimal collapsed label — save the full story for inside. */
export function formatCollapsedLabel(
  segments: ToolLineageSegment[],
  toolCount: number,
): { primary: string; secondary: string | null } {
  const steps = segments.length;
  const toolWord = toolCount === 1 ? "tool" : "tools";

  if (toolCount <= 1) {
    return { primary: `1 ${toolWord}`, secondary: null };
  }

  if (steps <= 1) {
    return { primary: `${toolCount} ${toolWord}`, secondary: null };
  }

  return {
    primary: `${toolCount} ${toolWord}`,
    secondary: null,
  };
}

/** Full breadcrumb — shown on expand / hover title only. */
export function formatLineagePreview(
  segments: ToolLineageSegment[],
  maxVisible = 8,
): string {
  if (segments.length === 0) return "";
  if (segments.length <= maxVisible) {
    return segments.map(formatSegmentLabel).join(" → ");
  }

  const head = segments.slice(0, 2).map(formatSegmentLabel);
  const tail = segments.slice(-2).map(formatSegmentLabel);
  const hidden = segments.length - head.length - tail.length;
  return [...head, `… +${hidden} …`, ...tail].join(" → ");
}

export function lineageStepCount(segments: ToolLineageSegment[]): number {
  return segments.length;
}

export type ToolLineageKind = "control" | "execute" | "discover" | "memory" | "finish" | "other";

/** Semantic bucket for dot accent — one glance, not a rainbow. */
export function segmentKind(toolName: string): ToolLineageKind {
  const name = toolName.toLowerCase();
  if (name.includes("turn_finish") || name.includes("turn_begin")) return "control";
  if (name.includes("finish")) return "finish";
  if (name.includes("memory") || name.includes("vault")) return "memory";
  if (name.includes("discover") || name.includes("spawn") || name.includes("capability_search")) {
    return "discover";
  }
  return "execute";
}

export function segmentAccentClass(
  toolName: string,
  status: ToolRunState["status"],
): string {
  if (status === "running") {
    return "bg-primary-400 shadow-[0_0_8px_rgba(167,139,250,0.65)]";
  }
  if (status === "failed") {
    return "bg-rose-400 shadow-[0_0_6px_rgba(251,113,133,0.45)]";
  }

  switch (segmentKind(toolName)) {
    case "control":
      return "bg-primary-300/90";
    case "finish":
      return "bg-emerald-400/90 shadow-[0_0_6px_rgba(52,211,153,0.35)]";
    case "discover":
      return "bg-sky-400/85";
    case "memory":
      return "bg-amber-400/85";
    default:
      return "bg-violet-400/75";
  }
}

export function segmentLabelClass(toolName: string): string {
  if (segmentKind(toolName) === "finish") {
    return "text-surface-100";
  }
  return "text-surface-200";
}
