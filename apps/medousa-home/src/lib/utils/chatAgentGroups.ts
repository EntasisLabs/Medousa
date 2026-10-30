import type { ChatMessage } from "$lib/types/chat";
import type { PeerProposalReviewRecord, SessionRef } from "$lib/types/generated/daemon_api";
import type { SubagentRow } from "./subagentRows";

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

/** Anchor work to its initiating turn, independent of the worker's live status. */
export function chatAgentGroups(
  messages: ChatMessage[],
  painted: ChatMessage[],
  proposals: PeerProposalReviewRecord[],
  workers: Map<string, SubagentRow>,
  scope?: SessionRef,
): Map<string | null, ChatAgentGroup> {
  const groups = new Map<string | null, ChatAgentGroup>();
  const visible = new Set(painted.map(message => message.id));
  function turnAnchor(index: number): string | null {
    if (index < 0) return null;
    let start = index;
    while (start > 0 && messages[start].role !== "user") start -= 1;
    // Use the stable user turn even when its transient handoff shell disappears.
    if (messages[start].role === "user" && visible.has(messages[start].id)) return messages[start].id;
    for (let i = start + 1; i < messages.length && messages[i].role !== "user"; i += 1) {
      const message = messages[i];
      if (message.role === "assistant" && message.lane !== "worker" && visible.has(message.id)) return message.id;
    }
    if (visible.has(messages[start].id)) return messages[start].id;
    return visible.has(messages[index].id) ? messages[index].id : null;
  }
  function group(anchor: string | null): ChatAgentGroup {
    let value = groups.get(anchor);
    if (!value) { value = { proposals: [], workers: [], entries: [] }; groups.set(anchor, value); }
    return value;
  }
  // A follow-up proposal may bind the same durable execution conversation.
  // Keep its original occurrence/anchor while updating the row's latest state.
  const threads = new Map<string, { row: PeerProposalReviewRecord; origin: PeerProposalReviewRecord }>();
  for (const proposal of [...proposals].sort((a, b) =>
    (Date.parse(a.proposal.request.context.created_at) || 0) - (Date.parse(b.proposal.request.context.created_at) || 0))) {
    const session = proposal.binding?.execution_session;
    const key = session
      ? JSON.stringify([session.authority_id, proposal.proposal.request.target.execution_runtime_id, session.session_id])
      : `proposal:${proposal.proposal.proposal_id}`;
    const first = threads.get(key);
    threads.set(key, { row: proposal, origin: first?.origin ?? proposal });
  }
  const origins = new Map([...threads.values()].map(({ row, origin }) => [row.proposal.proposal_id, origin]));
  for (const { row: proposal, origin } of threads.values()) {
    const sources = origin.proposal.request.context.sources;
    // A shared range is bounded by the admitted human request. Match full
    // authority/session coordinates, including imported remote provenance.
    let index = -1;
    for (let i = 0; i < messages.length; i += 1) {
      const message = messages[i];
      if (message.role !== "user" || !message.transcript) continue;
      const coordinates = [message.transcript, message.transcript.source].filter(Boolean);
      if (sources.some(({ selection }) => coordinates.some(coordinate => coordinate
        && coordinate.authorityId === selection.session.authority_id
        && coordinate.sessionId === selection.session.session_id
        && coordinate.entrySeq > (selection.after_entry_seq ?? 0)
        && coordinate.entrySeq <= selection.through_entry_seq))) index = i;
    }
    // Live user rows have an admitted timestamp before history has supplied
    // their entry coordinate. Only use it for a range beyond this daemon's
    // loaded history, never to reinterpret an older selected context range.
    if (scope) {
      const localSources = sources.filter(({ selection }) => selection.session.authority_id === scope.authority_id
        && selection.session.session_id === scope.session_id);
      const latestSeq = Math.max(0, ...messages.map(message => message.transcript?.authorityId === scope.authority_id
        && message.transcript.sessionId === scope.session_id ? message.transcript.entrySeq : 0));
      const created = Date.parse(origin.proposal.request.context.created_at);
      if (localSources.some(({ selection }) => selection.through_entry_seq > latestSeq)) {
        for (let i = index + 1; i < messages.length; i += 1) {
          const message = messages[i];
          if (message.role === "user" && !message.transcript && message.createdAt && Date.parse(message.createdAt) <= created) index = i;
        }
      }
    }
    const target = group(turnAnchor(index));
    target.proposals.push(proposal);
    target.entries!.push({ kind: "proposal", row: proposal });
  }
  function workerIndex(worker: SubagentRow): number {
    // A turn's active message may have advanced to its worker result. Never
    // reinterpret that arrival as the spawn location.
    let index = messages.findIndex(message => message.id === worker.parentMessageId && message.lane !== "worker");
    if (index < 0 && worker.parentTurnId) index = messages.findIndex(message => message.turnId === worker.parentTurnId && message.lane !== "worker");
    if (index < 0 && !worker.parentMessageId && !worker.parentTurnId) {
      index = messages.findIndex(message => message.workId === worker.workId && message.lane !== "worker");
    }
    return index;
  }
  for (const worker of workers.values()) {
    const target = group(turnAnchor(workerIndex(worker)));
    target.workers.push(worker);
    target.entries!.push({ kind: "worker", row: worker });
  }
  for (const [anchor, value] of groups) {
    const start = messages.findIndex(message => message.id === anchor);
    const nextUser = messages.findIndex((message, index) => index > start && message.role === "user");
    const turn = start < 0 ? [] : messages.slice(start, nextUser < 0 ? undefined : nextUser);
    const positions = new Map<ChatAgentEntry, number[]>();
    for (const entry of value.entries!) {
      const ids = entry.kind === "worker" ? [entry.row.workId] : [origins.get(entry.row.proposal.proposal_id)?.proposal.proposal_id, origins.get(entry.row.proposal.proposal_id)?.proposal.request.assignment_id, entry.row.proposal.proposal_id, entry.row.proposal.request.assignment_id].filter((id): id is string => Boolean(id));
      for (const [messageIndex, message] of turn.entries()) {
        if (!visible.has(message.id)) continue;
        for (const [segmentIndex, segment] of (message.segments ?? []).entries()) {
          const runIndex = segment.kind === "handoff" ? (ids.includes(segment.workId ?? "") ? 0 : -1)
            : segment.kind === "tool_group" ? segment.runs.findIndex(run => isDelegationTool(run.toolName) && ids.some(id =>
              (run.outputSummary ?? "").includes(id) || JSON.stringify(run.inputParams ?? []).includes(id))) : -1;
          if (runIndex < 0) continue;
          entry.nodeId = segment.kind === "tool_group" ? `${message.id}:segment:${segment.groupId}`
            : segment.kind === "handoff" && segment.text.trim() ? `${message.id}:segment:handoff:${segment.workId ?? segment.handoffKind}:${segmentIndex}` : undefined;
          positions.set(entry, [messageIndex, segmentIndex, runIndex]);
          break;
        }
        if (positions.has(entry)) break;
      }
    }
    // Segment coordinates beat unstable card/work IDs. Missing coordinates keep
    // supplied insertion order, never ID sort. Legacy rows cannot reconstruct
    // exact mixed spawn order without a persisted occurrence coordinate.
    value.entries!.sort((a, b) => {
      const first = positions.get(a); const second = positions.get(b);
      if (first && second) return first[0] - second[0] || first[1] - second[1] || first[2] - second[2];
      if (first !== undefined || second !== undefined) return first !== undefined ? -1 : 1;
      if (a.kind === "proposal" && b.kind === "proposal") {
        return (Date.parse(a.row.proposal.request.context.created_at) || 0) - (Date.parse(b.row.proposal.request.context.created_at) || 0);
      }
      return 0;
    });
    value.workers = value.entries!.flatMap(entry => entry.kind === "worker" ? [entry.row] : []);
    value.proposals = value.entries!.flatMap(entry => entry.kind === "proposal" ? [entry.row] : []);
  }
  return groups;
}
