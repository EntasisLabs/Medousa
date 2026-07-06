import type { ComponentDef } from "$lib/types/environment";

export type CustomViewFeedBadge = "live" | "stale" | "none";

const LIVE_WINDOW_MS = 10 * 60 * 1000;

function patchCheckedAt(patch: Record<string, unknown> | null | undefined): number | null {
  if (!patch) return null;
  const raw = patch.checkedAt ?? patch.checked_at;
  if (typeof raw !== "string" || !raw.trim()) return null;
  const ms = Date.parse(raw);
  return Number.isFinite(ms) ? ms : null;
}

export function feedBadgeForComponents(
  components: ComponentDef[],
  feedStateByComponentId: Map<string, Record<string, unknown>>,
  nowMs = Date.now(),
): CustomViewFeedBadge {
  const subscribed = components.some((component) => component.feeds.length > 0);
  if (!subscribed) return "none";

  let latest: number | null = null;
  for (const component of components) {
    const patch = feedStateByComponentId.get(component.id) ?? null;
    const checkedAt = patchCheckedAt(patch);
    if (checkedAt !== null && (latest === null || checkedAt > latest)) {
      latest = checkedAt;
    }
  }

  if (latest !== null && nowMs - latest <= LIVE_WINDOW_MS) {
    return "live";
  }
  return "stale";
}

export function presetDisplayLabel(presetId: string, presetLabel?: string | null): string {
  if (presetId === "default") return "Full";
  if (presetId === "focus") return "Focus";
  return presetLabel ?? presetId;
}

export function presetDescription(presetId: string): string {
  if (presetId === "focus") {
    return "Quiet nav — hides web and workshop noise";
  }
  if (presetId === "default") {
    return "All destinations in the rail";
  }
  return "Custom nav layout";
}
