import type { ContextThreadEntry, LocusNodeSummary } from "$lib/types/locus";
import {
  formatContextWhen,
  humanMomentTitle,
  momentHeadline,
  momentKeptProse,
  sessionDisplayName,
  tierHumanLabel,
} from "$lib/utils/contextHuman";

function shortSessionLabel(label: string, maxLen = 32): string {
  const value = label.trim();
  if (value.length <= maxLen) return value;
  const cut = value.slice(0, maxLen);
  const at = cut.lastIndexOf(" ");
  return `${(at > 16 ? cut.slice(0, at) : cut).trimEnd()}…`;
}

/** Scannable list title — feel first, never a tech dump. */
export function threadTitle(node: LocusNodeSummary): string {
  const title = humanMomentTitle(node);
  const kept = momentKeptProse("", node.context_summary, title, 96);
  return momentHeadline(node.user_avec, kept, title);
}

export function threadSubtitle(
  node: LocusNodeSummary,
  sessionLabels: Record<string, string> = {},
): string {
  const session = shortSessionLabel(sessionDisplayName(node.session_id, sessionLabels));
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
  return nodes.map((node) => {
    const title = threadTitle(node);
    const session = sessionDisplayName(node.session_id, sessionLabels);
    return {
      id: node.sync_key,
      title,
      subtitle: threadSubtitle(node, sessionLabels),
      searchText: [
        node.sync_key,
        node.session_id,
        session,
        node.tier,
        tierHumanLabel(node.tier),
        node.context_summary,
        node.timestamp,
        title,
      ].join(" "),
      sessionId: node.session_id,
      tier: node.tier,
      timestamp: node.timestamp,
      syncKey: node.sync_key,
    };
  });
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
