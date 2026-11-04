/**
 * Dual-write peer proposal facts into ChatCore so snapshot.agentGroups can
 * include proposal anchors. Daemon inbox remains poll/approve authority.
 */

import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import { applyChatProposals, openChatSession } from "$lib/appChat/client";
import { proposalFactsFromRows } from "$lib/utils/chatAgentGroups";

const lastFactsBySession = new Map<string, string>();

function factsKey(sessionId: string, facts: ReturnType<typeof proposalFactsFromRows>): string {
  return `${sessionId}:${JSON.stringify(facts)}`;
}

/**
 * Best-effort ApplyProposals. Skips when the fact vector is unchanged.
 * Failures are non-fatal (pure-DOM vitest / session not open yet).
 */
export function dualWriteChatProposals(
  sessionId: string | null | undefined,
  rows: PeerProposalReviewRecord[],
  options?: { authorityId?: string | null },
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
    .catch(() => {
      /* Core may be unavailable outside Tauri / without the stdio backend. */
    });
}

/** Test helper — drop memo so the next dual-write always dispatches. */
export function resetDualWriteChatProposalsMemo() {
  lastFactsBySession.clear();
}
