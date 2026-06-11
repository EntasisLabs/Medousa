import type { LocusAvecSnapshot, LocusNodeSummary } from "$lib/types/locus";

export interface ContextPostureEntry {
  id: string;
  sessionId: string;
  title: string;
  subtitle: string;
  searchText: string;
  userAvec: LocusAvecSnapshot;
  modelAvec: LocusAvecSnapshot | null;
  threadCount: number;
  latestTimestamp: string;
  latestSyncKey: string;
  latestSummary: string;
}

export const AVEC_DIMENSIONS: {
  key: keyof Omit<LocusAvecSnapshot, "psi">;
  label: string;
}[] = [
  { key: "stability", label: "Stability" },
  { key: "friction", label: "Friction" },
  { key: "logic", label: "Logic" },
  { key: "autonomy", label: "Autonomy" },
];

function parseTimestamp(value: string): number {
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? 0 : ms;
}

function formatPostureTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return timestamp;
  return date.toLocaleString();
}

export function buildContextPostureEntries(
  nodes: LocusNodeSummary[],
  sessionLabels: Record<string, string> = {},
): ContextPostureEntry[] {
  const bySession = new Map<string, LocusNodeSummary[]>();

  for (const node of nodes) {
    const bucket = bySession.get(node.session_id) ?? [];
    bucket.push(node);
    bySession.set(node.session_id, bucket);
  }

  const entries: ContextPostureEntry[] = [];

  for (const [sessionId, sessionNodes] of bySession) {
    const sorted = [...sessionNodes].sort(
      (left, right) => parseTimestamp(right.timestamp) - parseTimestamp(left.timestamp),
    );
    const latestWithAvec = sorted.find((node) => node.user_avec);
    if (!latestWithAvec?.user_avec) continue;

    const label = sessionLabels[sessionId]?.trim() || sessionId;
    const latestSummary = latestWithAvec.context_summary.trim();
    const title = label === sessionId ? sessionId : label;

    entries.push({
      id: sessionId,
      sessionId,
      title,
      subtitle: `ψ ${latestWithAvec.user_avec.psi.toFixed(2)} · ${sorted.length} thread${sorted.length === 1 ? "" : "s"} · ${formatPostureTime(latestWithAvec.timestamp)}`,
      searchText: [
        sessionId,
        title,
        label,
        latestSummary,
        latestWithAvec.sync_key,
        ...AVEC_DIMENSIONS.map((dim) => String(latestWithAvec.user_avec?.[dim.key] ?? "")),
      ].join(" "),
      userAvec: latestWithAvec.user_avec,
      modelAvec: latestWithAvec.model_avec ?? null,
      threadCount: sorted.length,
      latestTimestamp: latestWithAvec.timestamp,
      latestSyncKey: latestWithAvec.sync_key,
      latestSummary,
    });
  }

  return entries.sort(
    (left, right) => parseTimestamp(right.latestTimestamp) - parseTimestamp(left.latestTimestamp),
  );
}

export function filterContextPostureEntries(
  entries: ContextPostureEntry[],
  query: string,
): ContextPostureEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) => entry.searchText.toLowerCase().includes(needle));
}

export function postureWhisper(avec: LocusAvecSnapshot): string {
  return AVEC_DIMENSIONS.map(
    (dim) => `${dim.label.toLowerCase()} ${avec[dim.key].toFixed(2)}`,
  )
    .concat(`ψ ${avec.psi.toFixed(2)}`)
    .join(" · ");
}
