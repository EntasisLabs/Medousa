import type { ChatMessage } from "$lib/types/chat";
import type {
  AgentPlacementGroup,
  ProposalFact,
  WorkerPlacementFact,
  WorkerReturnFact,
} from "$lib/types/generated/app_api";
import type { PeerProposalReviewRecord, SessionRef } from "$lib/types/generated/daemon_api";
import type { SubagentRow } from "./subagentRows";
import { chatMessageToItem } from "$lib/appChat/fromChatMessage";
import { projectAgentPlacement, projectVisibleItems } from "$lib/appChat/client";

export type ChatAgentEntry =
  | { kind: "proposal"; row: PeerProposalReviewRecord; nodeId?: string }
  | { kind: "worker"; row: SubagentRow; nodeId?: string };

export interface ChatAgentGroup {
  proposals: PeerProposalReviewRecord[];
  workers: SubagentRow[];
  /** Mixed spawn order; optional for standalone proposal/worker lists. */
  entries?: ChatAgentEntry[];
}

export function agentEntries(group: ChatAgentGroup): ChatAgentEntry[] {
  return group.entries ?? [
    ...group.proposals.map(row => ({ kind: "proposal" as const, row })),
    ...group.workers.map(row => ({ kind: "worker" as const, row })),
  ];
}

function isDelegationTool(name: string): boolean {
  return /peer[_\.]?(propose|handoff)|workshop[_.]spawn|cognition_workshop_mutate/.test(name);
}

/** Tool event projection is also an invalidation signal for proposal discovery. */
export function delegationRevision(messages: ChatMessage[]): string {
  return JSON.stringify(messages.flatMap(message => {
    const runs = new Map([
      ...(message.toolRuns ?? []),
      ...(message.segments ?? []).flatMap(segment => segment.kind === "tool_group" ? segment.runs : []),
    ].map(run => [run.runId, run]));
    return [...runs.values()].filter(run => isDelegationTool(run.toolName))
      .map(run => [message.id, run.runId, run.status, run.outputSummary]);
  }));
}

/** Extract thin proposal facts for the Rust placement core. */
export function proposalFactsFromRows(proposals: PeerProposalReviewRecord[]): ProposalFact[] {
  return proposals.map(row => ({
    proposal_id: row.proposal.proposal_id,
    assignment_id: row.proposal.request.assignment_id ?? null,
    created_at: row.proposal.request.context.created_at ?? null,
    execution_runtime_id: row.proposal.request.target?.execution_runtime_id ?? null,
    binding_authority_id: row.binding?.execution_session?.authority_id ?? null,
    binding_session_id: row.binding?.execution_session?.session_id ?? null,
    sources: (row.proposal.request.context.sources ?? []).map(({ selection }) => ({
      authority_id: selection.session.authority_id,
      session_id: selection.session.session_id,
      after_entry_seq: selection.after_entry_seq ?? null,
      through_entry_seq: selection.through_entry_seq,
    })),
  }));
}

/** Extract thin worker facts; Map/array order is spawn insertion order. */
export function workerFactsFromRows(
  workers: Map<string, SubagentRow> | Iterable<SubagentRow>,
): WorkerPlacementFact[] {
  const rows = workers instanceof Map ? workers.values() : workers;
  return [...rows].map(row => ({
    work_id: row.workId,
    parent_turn_id: row.parentTurnId ?? null,
    parent_item_id: row.parentMessageId ?? null,
  }));
}

/** Host-supplied raw return texts for painted-id visibility projection. */
export function workerReturnFactsFromRows(
  workers: ReadonlyMap<string, { returnedTexts?: string[] }>,
): WorkerReturnFact[] {
  return [...workers.entries()].map(([workId, row]) => ({
    work_id: workId,
    returned_texts: row.returnedTexts ?? [],
  }));
}

/**
 * Join Rust placement groups to full UI rows. Missing rows are skipped so the
 * renderer never invents placement.
 */
export function joinAgentGroups(
  groups: AgentPlacementGroup[],
  proposals: PeerProposalReviewRecord[],
  workers: Map<string, SubagentRow>,
): Map<string | null, ChatAgentGroup> {
  const proposalsById = new Map(proposals.map(row => [row.proposal.proposal_id, row]));
  const result = new Map<string | null, ChatAgentGroup>();
  for (const group of groups) {
    const anchor = group.anchorItemId ?? null;
    const entries: ChatAgentEntry[] = [];
    for (const entry of group.entries) {
      if (entry.kind === "proposal") {
        const row = proposalsById.get(entry.proposalId);
        if (!row) continue;
        entries.push({ kind: "proposal", row, nodeId: entry.nodeId ?? undefined });
      } else {
        const row = workers.get(entry.workId);
        if (!row) continue;
        entries.push({ kind: "worker", row, nodeId: entry.nodeId ?? undefined });
      }
    }
    result.set(anchor, {
      entries,
      proposals: entries.flatMap(entry => entry.kind === "proposal" ? [entry.row] : []),
      workers: entries.flatMap(entry => entry.kind === "worker" ? [entry.row] : []),
    });
  }
  return result;
}

/**
 * Project agent/task placement through the Rust application core, then join
 * full proposal/worker rows for painting. Visible ids come from core too;
 * `painted` remains a sync paint fallback when projection is unavailable.
 */
export async function projectChatAgentGroups(
  messages: ChatMessage[],
  painted: ChatMessage[],
  proposals: PeerProposalReviewRecord[],
  workers: Map<string, SubagentRow>,
  scope?: SessionRef,
): Promise<Map<string | null, ChatAgentGroup>> {
  const items = messages.map(chatMessageToItem);
  const sessionId = scope?.session_id ?? "";
  let visibleItemIds = painted.map((message) => message.id);
  try {
    visibleItemIds = await projectVisibleItems({
      sessionId,
      items,
      workerReturns: workerReturnFactsFromRows(workers),
    });
  } catch {
    // Keep host-painted ids when the core path is unavailable.
  }
  const groups = await projectAgentPlacement({
    sessionId,
    authorityId: scope?.authority_id ?? "",
    items,
    proposals: proposalFactsFromRows(proposals),
    workers: workerFactsFromRows(workers),
    visibleItemIds,
  });
  return joinAgentGroups(groups, proposals, workers);
}
