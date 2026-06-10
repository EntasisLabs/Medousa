import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard, WorkspaceEvent } from "$lib/types/workspace";
import { formatCardTitle } from "$lib/utils/formatWork";
import {
  buildProvenanceChips,
  formatManifestStatusChip,
  type ProvenanceChip,
  type WorkHubLayer,
  workHubLayer,
} from "$lib/utils/workHub";
import { filterCardTimeline, sortTimeline } from "$lib/utils/cardTimeline";

export interface WorkManifestation {
  id: string;
  title: string;
  layer: WorkHubLayer;
  statusChip: ReturnType<typeof formatManifestStatusChip>;
  updatedAt: string;
  provenance: ProvenanceChip[];
  timeline: WorkspaceEvent[];
  detail: WorkCardDetail | null;
  resultPreview: string | null;
  error: string | null;
}

export function buildWorkManifestation(
  card: WorkCard,
  detail: WorkCardDetail | null | undefined,
  feed: WorkspaceEvent[],
): WorkManifestation {
  const timeline = sortTimeline(filterCardTimeline(feed, card.id));
  const resultPreview =
    detail?.result_excerpt?.trim() ||
    detail?.task_line?.trim() ||
    detail?.user_ack?.trim() ||
    null;

  return {
    id: card.id,
    title: formatCardTitle(card),
    layer: workHubLayer(card),
    statusChip: formatManifestStatusChip(card),
    updatedAt: card.updated_at_utc,
    provenance: buildProvenanceChips(card, detail),
    timeline,
    detail: detail ?? null,
    resultPreview,
    error: detail?.error?.trim() || null,
  };
}
