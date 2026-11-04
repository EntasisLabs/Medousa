/**
 * Commit peer proposal facts into ChatCore and paint the returned snapshot.
 * Daemon inbox remains poll/approve authority; core owns proposal anchors on
 * snapshot.agentGroups.
 */

import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import type { ChatSnapshot } from "$lib/types/generated/app_api";
import { applyChatProposals, openChatSession } from "$lib/appChat/client";
import { proposalFactsFromRows } from "$lib/utils/chatAgentGroups";

const lastFactsBySession = new Map<string, string>();

function factsKey(sessionId: string, facts: ReturnType<typeof proposalFactsFromRows>): string {
  return `${sessionId}:${JSON.stringify(facts)}`;
}

export type CommitChatProposalsPaint = (
  sessionId: string,
  snapshot: ChatSnapshot,
) => void;

/**
 * ApplyProposals then paint. Skips when the fact vector is unchanged.
 * Failures are non-fatal (pure-DOM vitest / session not open yet).
 */
export function commitChatProposals(
  sessionId: string | null | undefined,
  rows: PeerProposalReviewRecord[],
  options?: {
    authorityId?: string | null;
    paint?: CommitChatProposalsPaint;
  },
): void {
  const trimmed = sessionId?.trim();
  if (!trimmed) return;

  const facts = proposalFactsFromRows(rows);
  const key = factsKey(trimmed, facts);
  if (lastFactsBySession.get(trimmed) === key) return;
  lastFactsBySession.set(trimmed, key);

  const authority =
    options?.authorityId?.trim() ||
    `auth_${"0".repeat(64)}`;

  void openChatSession(trimmed, authority)
    .then(() => applyChatProposals(trimmed, facts))
    .then((snapshot) => {
      options?.paint?.(trimmed, snapshot);
    })
    .catch(() => {
      /* Core may be unavailable outside Tauri / without the stdio backend. */
    });
}

/** Test helper — drop memo so the next commit always dispatches. */
export function resetCommitChatProposalsMemo() {
  lastFactsBySession.clear();
}
