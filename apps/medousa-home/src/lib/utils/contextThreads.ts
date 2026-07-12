import type { ContextThreadEntry, LocusNodeSummary } from "$lib/types/locus";
import {
  formatContextWhen,
  humanMomentTitle,
  sessionDisplayName,
  tierHumanLabel,
} from "$lib/utils/contextHuman";

export function threadTitle(node: LocusNodeSummary): string {
  return humanMomentTitle(node);
}

export function threadSubtitle(
  node: LocusNodeSummary,
  sessionLabels: Record<string, string> = {},
): string {
  const session = sessionDisplayName(node.session_id, sessionLabels);
  const when = formatContextWhen(node.timestamp);
  return `${when} · ${session}`;
}

export function formatThreadTime(timestamp: string): string {
  return formatContextWhen(timestamp);
}

export function buildContextThreadEntries(
  nodes: LocusNodeSummary[],
  sessionLabels: Record<string, string> = {},
): ContextThreadEntry[] {
  return nodes.map((node) => ({
    id: node.sync_key,
    title: threadTitle(node),
    subtitle: threadSubtitle(node, sessionLabels),
    searchText: [
      node.sync_key,
      node.session_id,
      sessionDisplayName(node.session_id, sessionLabels),
      node.tier,
      tierHumanLabel(node.tier),
      node.context_summary,
      node.timestamp,
    ].join(" "),
    sessionId: node.session_id,
    tier: node.tier,
    timestamp: node.timestamp,
    syncKey: node.sync_key,
  }));
}

export function filterContextThreadEntries(
  entries: ContextThreadEntry[],
  query: string,
): ContextThreadEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter((entry) => entry.searchText.toLowerCase().includes(needle));
}

export function avecWhisper(
  avec: { stability: number; friction: number; logic: number; autonomy: number; psi: number } | null | undefined,
): string | null {
  if (!avec) return null;
  return `stability ${avec.stability.toFixed(2)} · friction ${avec.friction.toFixed(2)} · logic ${avec.logic.toFixed(2)} · autonomy ${avec.autonomy.toFixed(2)} · ψ ${avec.psi.toFixed(2)}`;
}
