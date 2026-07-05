import type { WorkspaceEvent } from "$lib/types/workspace";

const TERMINAL_NOISE = /dead_letter|workflow:\s*cognitio/i;
const TECHNICAL_SUMMARY = /dead_letter|workflow:\s*cognitio|cognition_[a-z0-9_]+/i;

/** Internal lifecycle events — hidden unless technical activity is enabled. */
const TECHNICAL_KINDS = new Set([
  "turn_accepted",
  "turn_completed",
  "locus_bridge_written",
]);

export interface ActivityFeedFilterOptions {
  showTechnical?: boolean;
  hiddenIds?: ReadonlySet<string>;
}

export function visibleActivityFeed(
  events: WorkspaceEvent[],
  options?: ActivityFeedFilterOptions,
): WorkspaceEvent[] {
  let filtered = events;
  if (options?.hiddenIds && options.hiddenIds.size > 0) {
    filtered = filtered.filter((event) => !options.hiddenIds!.has(event.id));
  }
  return filterOperatorActivity(filtered, {
    showTechnical: options?.showTechnical,
  });
}

export function filterOperatorActivity(
  events: WorkspaceEvent[],
  options?: { showTechnical?: boolean },
): WorkspaceEvent[] {
  if (options?.showTechnical) {
    return events;
  }

  const seen = new Map<string, number>();
  const filtered: WorkspaceEvent[] = [];

  for (const event of [...events].reverse()) {
    if (isOperatorHiddenEvent(event)) continue;

    if (isTerminalNoise(event)) {
      const key = event.summary.toLowerCase();
      const count = seen.get(key) ?? 0;
      if (count >= 1) continue;
      seen.set(key, count + 1);
    }
    filtered.push(event);
  }

  return filtered.reverse();
}

export function isTechnicalActivityEvent(event: WorkspaceEvent): boolean {
  if (TECHNICAL_KINDS.has(event.kind)) return true;
  if (TECHNICAL_SUMMARY.test(event.summary)) return true;
  if (isTerminalNoise(event)) return true;
  return false;
}

function isOperatorHiddenEvent(event: WorkspaceEvent): boolean {
  if (TECHNICAL_KINDS.has(event.kind)) return true;
  if (TECHNICAL_SUMMARY.test(event.summary)) return true;
  if (isTerminalNoise(event)) return true;
  return false;
}

function isTerminalNoise(event: WorkspaceEvent): boolean {
  if (event.kind !== "job_failed") return false;
  return TERMINAL_NOISE.test(event.summary);
}
