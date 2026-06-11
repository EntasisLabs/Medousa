import type { ContextRecallEntry } from "$lib/types/context";
import type { LocusNodeSummary } from "$lib/types/locus";

const STOP_WORDS = new Set([
  "the",
  "and",
  "for",
  "with",
  "that",
  "this",
  "from",
  "your",
  "about",
  "into",
  "when",
  "what",
  "they",
  "have",
  "been",
  "will",
  "would",
  "could",
  "should",
  "their",
  "there",
  "where",
  "which",
  "while",
  "than",
  "then",
  "also",
  "just",
  "like",
  "some",
  "more",
  "very",
  "does",
  "dont",
  "doesnt",
  "user",
  "operator",
]);

function claimKeywords(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((word) => word.length > 3 && !STOP_WORDS.has(word));
}

function scoreThreadMatch(entry: ContextRecallEntry, node: LocusNodeSummary): number {
  let score = 0;
  const claimId = entry.meta?.claim_id?.toLowerCase() ?? "";
  const syncKey = node.sync_key.toLowerCase();
  const summary = node.context_summary.toLowerCase();

  if (claimId && (syncKey.includes(claimId) || summary.includes(claimId))) {
    score += 12;
  }

  if (entry.title.toLowerCase().includes(syncKey) || syncKey.includes(entry.id.toLowerCase())) {
    score += 8;
  }

  for (const word of claimKeywords(entry.title)) {
    if (summary.includes(word)) score += 2;
    if (syncKey.includes(word)) score += 1;
  }

  return score;
}

export function findRelatedThreadsForClaim(
  entry: ContextRecallEntry,
  nodes: LocusNodeSummary[],
  limit = 3,
): LocusNodeSummary[] {
  if (entry.kind !== "claim" || nodes.length === 0) return [];

  return [...nodes]
    .map((node) => ({ node, score: scoreThreadMatch(entry, node) }))
    .filter(({ score }) => score >= 4)
    .sort((left, right) => {
      if (right.score !== left.score) return right.score - left.score;
      return Date.parse(right.node.timestamp) - Date.parse(left.node.timestamp);
    })
    .slice(0, limit)
    .map(({ node }) => node);
}

export function hasKnownChatSession(
  sessionId: string,
  sessionIds: Set<string> | string[],
): boolean {
  const trimmed = sessionId.trim();
  if (!trimmed) return false;
  if (sessionIds instanceof Set) return sessionIds.has(trimmed);
  return sessionIds.includes(trimmed);
}

export function threadSearchQueryForClaim(entry: ContextRecallEntry): string {
  const words = claimKeywords(entry.title);
  return words.slice(0, 4).join(" ");
}
