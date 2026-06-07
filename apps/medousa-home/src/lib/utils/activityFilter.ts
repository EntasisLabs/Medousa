import type { WorkspaceEvent } from "$lib/types/workspace";

const TERMINAL_NOISE = /dead_letter|workflow:\s*cognitio/i;

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
    if (isTerminalNoise(event)) {
      const key = event.summary.toLowerCase();
      const count = seen.get(key) ?? 0;
      if (count >= 2) continue;
      seen.set(key, count + 1);
    }
    filtered.push(event);
  }

  return filtered.reverse();
}

function isTerminalNoise(event: WorkspaceEvent): boolean {
  if (event.kind !== "job_failed") return false;
  return TERMINAL_NOISE.test(event.summary);
}
