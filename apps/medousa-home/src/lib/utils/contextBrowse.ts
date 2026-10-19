import type { LocusNodeSummary } from "$lib/types/locus";
import type { VaultNoteSummary } from "$lib/types/vault";
import { sessionDisplayName } from "$lib/utils/contextHuman";
import { sessionIdForNoteChatTag } from "$lib/utils/contextMapNotes";

export interface ContextBrowseEntry {
  id: string;
  title: string;
  kind: string;
  excerpt: string;
  source: string;
  timestamp: string;
}

/** Presentation only: the original summary and record stay unchanged. */
export function contextMemoryPresentation(node: Pick<LocusNodeSummary, "context_summary" | "tier">) {
  const original = node.context_summary;
  const summary = original.trim();
  const prefix = summary.match(/^(verification|checkpoint|handoff|change|open_gap|summary|decision)\s*:\s*/i);
  const kinds: Record<string, string> = { verification: "Verification", checkpoint: "Checkpoint", handoff: "Handoff", change: "Change", open_gap: "Open gap", summary: "Summary", decision: "Decision" };
  let kind = prefix ? kinds[prefix[1].toLowerCase()] : "Memory";
  let content = prefix ? summary.slice(prefix[0].length) : summary;
  if (/^Verification failed:/i.test(content)) kind = "Verification failed";
  content = content.replace(/^(?:Verification (?:completed|failed)|Terminal checkpoint|Handoff(?: checkpoint)?|Applied change)\s*:\s*/i, "");
  const firstLine = content.split(/\r?\n/)[0].replace(/^#+\s*/, "").trim() || "Saved memory";
  const firstSentence = firstLine.match(/^.+?[.!?](?=\s|$)/)?.[0] || firstLine;
  const title = firstSentence.length <= 88 ? firstSentence : `${firstSentence.slice(0, 85).replace(/\s+\S*$/, "")}…`;
  const titlePrefix = title.replace(/…$/, "");
  const remainder = content.startsWith(titlePrefix) ? content.slice(titlePrefix.length).trim() : "";
  const excerpt = remainder.length > 160 ? `${remainder.slice(0, 157).replace(/\s+\S*$/, "")}…` : remainder;
  return { kind, title, excerpt, content, original };
}

export function contextBrowseEntries(
  moments: LocusNodeSummary[], labels: Record<string, string>, query = "",
): ContextBrowseEntry[] {
  const needle = query.trim().toLowerCase();
  return moments.filter((node) => !needle || `${node.context_summary} ${sessionDisplayName(node.session_id, labels)} ${node.session_id} ${node.sync_key}`.toLowerCase().includes(needle))
    .map((node) => {
      const { title, kind, excerpt } = contextMemoryPresentation(node);
      return { id: `thread:${node.sync_key}`, title, kind, excerpt,
        source: sessionDisplayName(node.session_id, labels), timestamp: node.timestamp };
    })
    .sort((a, b) => (Date.parse(b.timestamp) || 0) - (Date.parse(a.timestamp) || 0) || a.id.localeCompare(b.id));
}

/** Notes belong here only through an explicit link to a saved memory's source session. */
export function relatedContextNotes(notes: VaultNoteSummary[], sessionIds: Iterable<string>): VaultNoteSummary[] {
  const ids = [...new Set(sessionIds)];
  return notes.filter((note) => sessionIdForNoteChatTag(note.tags ?? [], ids) !== null);
}

export function contextNotesForSession(notes: VaultNoteSummary[], sessionId: string, sessionIds: Iterable<string>): VaultNoteSummary[] {
  const ids = [...new Set([...sessionIds, sessionId])];
  return notes.filter((note) => sessionIdForNoteChatTag(note.tags ?? [], ids) === sessionId);
}
