import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard, WorkspaceEvent } from "$lib/types/workspace";
import {
  resolveActivityEnrichment,
  vaultRefPath,
} from "$lib/utils/activityEnrichment";
import {
  presentActivityEvent,
  type ActivityPresentation,
  type ActivityTone,
} from "$lib/utils/activityPresentation";

export interface ActivityStoryBeat {
  id: string;
  eventIds: string[];
  event: WorkspaceEvent;
  count: number;
  presentation: ActivityPresentation;
  kicker: string;
}

export interface ActivityStoryChapter {
  key: string;
  label: string;
  beats: ActivityStoryBeat[];
}

const COLLAPSE_WINDOW_MS = 3 * 60 * 60 * 1000;

function collapseKey(event: WorkspaceEvent): string {
  const path = vaultRefPath(event);
  if (
    (event.kind === "vault_note_updated" || event.kind === "vault_note_created") &&
    path
  ) {
    return `vault:${event.actor}:${event.kind}:${path}`;
  }
  return `solo:${event.id}`;
}

/** Only surface kickers that change the mood — vault/done/motion stay silent. */
function activityKicker(kind: string, tone: ActivityTone): string {
  switch (kind) {
    case "job_failed":
      return "Stuck";
    case "work_unblocked":
      return "Needs you";
    case "identity_remembered":
      return "Memory";
    default:
      if (tone === "attention") return "Attention";
      return "";
  }
}

function chapterForTimestamp(iso: string, now = new Date()): { key: string; label: string } {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return { key: "unknown", label: "Earlier" };
  }

  const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.round(
    (startToday.getTime() - startDate.getTime()) / (24 * 60 * 60 * 1000),
  );
  const ageMs = now.getTime() - date.getTime();

  if (diffDays === 0) {
    if (ageMs < 60 * 60 * 1000) {
      return { key: "now", label: "Just now" };
    }
    return { key: "today", label: "Earlier today" };
  }
  if (diffDays === 1) return { key: "yesterday", label: "Yesterday" };
  if (diffDays > 1 && diffDays < 7) {
    return {
      key: `day-${startDate.toISOString().slice(0, 10)}`,
      label: date.toLocaleDateString(undefined, { weekday: "long" }),
    };
  }
  return {
    key: `day-${startDate.toISOString().slice(0, 10)}`,
    label: date.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    }),
  };
}

function withCollapsedSummary(
  presentation: ActivityPresentation,
  count: number,
  kind: string,
): ActivityPresentation {
  if (count <= 1) return presentation;
  if (kind === "vault_note_updated" || kind === "vault_note_created") {
    const base = presentation.summary.replace(/\s·\s\d+\s+saves?$/i, "");
    return {
      ...presentation,
      summary: `${base} · ${count} saves`,
    };
  }
  return {
    ...presentation,
    summary: `${presentation.summary} · ×${count}`,
  };
}

/**
 * Newest-first workspace events → collapsed story beats grouped by time chapter.
 */
export function buildActivityStory(
  events: WorkspaceEvent[],
  cardsById: ReadonlyMap<string, WorkCard>,
  detailsById: ReadonlyMap<string, WorkCardDetail>,
): ActivityStoryChapter[] {
  const newestFirst = [...events].sort(
    (a, b) => Date.parse(b.timestamp_utc) - Date.parse(a.timestamp_utc),
  );

  const clusters: WorkspaceEvent[][] = [];
  for (const event of newestFirst) {
    const key = collapseKey(event);
    const latest = clusters[clusters.length - 1];
    if (!latest) {
      clusters.push([event]);
      continue;
    }
    const head = latest[0];
    const sameKey = collapseKey(head) === key && !key.startsWith("solo:");
    const withinWindow =
      Math.abs(Date.parse(head.timestamp_utc) - Date.parse(event.timestamp_utc)) <=
      COLLAPSE_WINDOW_MS;
    if (sameKey && withinWindow) {
      latest.push(event);
    } else {
      clusters.push([event]);
    }
  }

  const chapters: ActivityStoryChapter[] = [];
  const chapterIndex = new Map<string, number>();

  for (const cluster of clusters) {
    const event = cluster[0];
    const enrichment = resolveActivityEnrichment(event, cardsById, detailsById);
    const presentation = withCollapsedSummary(
      presentActivityEvent(event, enrichment),
      cluster.length,
      event.kind,
    );
    const beat: ActivityStoryBeat = {
      id: event.id,
      eventIds: cluster.map((entry) => entry.id),
      event,
      count: cluster.length,
      presentation,
      kicker: activityKicker(event.kind, presentation.tone),
    };

    const chapterMeta = chapterForTimestamp(event.timestamp_utc);
    const existing = chapterIndex.get(chapterMeta.key);
    if (existing == null) {
      chapterIndex.set(chapterMeta.key, chapters.length);
      chapters.push({
        key: chapterMeta.key,
        label: chapterMeta.label,
        beats: [beat],
      });
    } else {
      chapters[existing].beats.push(beat);
    }
  }

  return chapters;
}
