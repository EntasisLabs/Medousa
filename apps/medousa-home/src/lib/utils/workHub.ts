import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";
import { formatCardTitle } from "$lib/utils/formatWork";
import { vaultDisplayTitle } from "$lib/utils/formatVault";

export type WorkHubLayer = "living" | "settled" | "failed" | "stopped" | "stuck";

export type StatusChipTone = "muted" | "primary" | "warning" | "danger" | "success";

export interface WorkHubStatusChip {
  label: string;
  tone: StatusChipTone;
}

export interface WorkHubPartition {
  living: WorkCard[];
  settled: WorkCard[];
  failed: WorkCard[];
  stopped: WorkCard[];
  stuck: WorkCard[];
}

export interface ProvenanceChip {
  id: string;
  label: string;
  kind: "chat" | "vault" | "tools" | "session" | "manuscript";
  href?: string | null;
}

const LIVING_COLUMN_ORDER: Record<string, number> = {
  in_flight: 0,
  wrapping_up: 1,
  backlog: 2,
  blocked: 3,
};

export function partitionWorkHub(cards: WorkCard[]): WorkHubPartition {
  const living: WorkCard[] = [];
  const settled: WorkCard[] = [];
  const failed: WorkCard[] = [];
  const stopped: WorkCard[] = [];
  const stuck: WorkCard[] = [];

  for (const card of cards) {
    const layer = workHubLayer(card);
    switch (layer) {
      case "living":
        living.push(card);
        break;
      case "settled":
        settled.push(card);
        break;
      case "failed":
        failed.push(card);
        break;
      case "stopped":
        stopped.push(card);
        break;
      case "stuck":
        stuck.push(card);
        break;
    }
  }

  return {
    living: sortLivingCards(living),
    settled: sortByUpdated(settled),
    failed: sortByUpdated(failed),
    stopped: sortByUpdated(stopped),
    stuck: sortByUpdated(stuck),
  };
}

export function workHubLayer(card: WorkCard): WorkHubLayer {
  if (card.column === "done") return "settled";

  if (card.column === "blocked") {
    if (card.status_label === "needs approval") return "living";
    if (card.status_label === "canceled") return "stopped";
    if (card.status_label === "failed" || card.status_label === "dead_letter") {
      return "failed";
    }
    return "stuck";
  }

  if (card.column === "backlog" || card.column === "in_flight" || card.column === "wrapping_up") {
    return "living";
  }

  return "stuck";
}

export function formatManifestStatusChip(card: WorkCard): WorkHubStatusChip {
  if (card.column === "done") {
    return { label: "Settled", tone: "success" };
  }

  if (card.column === "blocked") {
    if (card.status_label === "needs approval") {
      return { label: "Needs you", tone: "warning" };
    }
    if (card.status_label === "failed") {
      return { label: "Failed", tone: "danger" };
    }
    if (card.status_label === "dead_letter") {
      return { label: "Stuck", tone: "danger" };
    }
    if (card.status_label === "canceled") {
      return { label: "Stopped", tone: "muted" };
    }
    return { label: "Stuck", tone: "danger" };
  }

  switch (card.column) {
    case "backlog":
      return { label: "Queued", tone: "muted" };
    case "in_flight":
      return { label: "Running", tone: "primary" };
    case "wrapping_up":
      return { label: "Finishing", tone: "warning" };
    default:
      return { label: card.column.replaceAll("_", " "), tone: "muted" };
  }
}

export function buildProvenanceChips(
  card: WorkCard,
  detail?: WorkCardDetail | null,
): ProvenanceChip[] {
  const chips: ProvenanceChip[] = [];

  if (detail?.session_id?.trim()) {
    chips.push({
      id: "chat",
      label: "Chat",
      kind: "chat",
    });
  }

  for (const path of detail?.associations.vault_paths ?? []) {
    const trimmed = path.trim();
    if (!trimmed) continue;
    chips.push({
      id: `vault:${trimmed}`,
      label: vaultDisplayTitle(trimmed, trimmed),
      kind: "vault",
      href: trimmed,
    });
  }

  const toolCount = detail?.tool_names?.length ?? 0;
  if (toolCount > 0) {
    chips.push({
      id: "tools",
      label: `${toolCount} tool${toolCount === 1 ? "" : "s"}`,
      kind: "tools",
    });
  }

  if (detail?.manuscript_id?.trim()) {
    chips.push({
      id: `manuscript:${detail.manuscript_id}`,
      label: detail.manuscript_id,
      kind: "manuscript",
    });
  }

  if (chips.length === 0) {
    chips.push({
      id: "card",
      label: formatCardTitle(card).slice(0, 32),
      kind: "session",
    });
  }

  return chips.slice(0, 4);
}

function sortLivingCards(cards: WorkCard[]): WorkCard[] {
  return [...cards].sort((a, b) => {
    const columnDelta =
      (LIVING_COLUMN_ORDER[a.column] ?? 9) - (LIVING_COLUMN_ORDER[b.column] ?? 9);
    if (columnDelta !== 0) return columnDelta;
    return Date.parse(b.updated_at_utc) - Date.parse(a.updated_at_utc);
  });
}

function sortByUpdated(cards: WorkCard[]): WorkCard[] {
  return [...cards].sort(
    (a, b) => Date.parse(b.updated_at_utc) - Date.parse(a.updated_at_utc),
  );
}

export function hubCardsForPrefetch(cards: WorkCard[]): WorkCard[] {
  const { living, settled, failed, stopped, stuck } = partitionWorkHub(cards);
  return [...living, ...settled, ...failed.slice(0, 8), ...stopped.slice(0, 8), ...stuck.slice(0, 8)];
}
