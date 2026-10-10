import type { ChatMessage } from "$lib/types/chat";
import type { PeerProposalReviewRecord, SessionRef } from "$lib/types/generated/daemon_api";
import type { SubagentRow } from "./subagentRows";

export interface ChatAgentGroup {
  proposals: PeerProposalReviewRecord[];
  workers: SubagentRow[];
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
    if (!value) { value = { proposals: [], workers: [] }; groups.set(anchor, value); }
    return value;
  }
  for (const proposal of proposals) {
    const sources = proposal.proposal.request.context.sources;
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
      const created = Date.parse(proposal.proposal.request.context.created_at);
      if (localSources.some(({ selection }) => selection.through_entry_seq > latestSeq)) {
        for (let i = index + 1; i < messages.length; i += 1) {
          const message = messages[i];
          if (message.role === "user" && !message.transcript && message.createdAt && Date.parse(message.createdAt) <= created) index = i;
        }
      }
    }
    group(turnAnchor(index)).proposals.push(proposal);
  }
  for (const worker of workers.values()) {
    let index = messages.findIndex(message => message.id === worker.parentMessageId);
    if (index < 0 && worker.parentTurnId) index = messages.findIndex(message => message.turnId === worker.parentTurnId && message.lane !== "worker");
    if (index < 0) index = messages.findIndex(message => message.workId === worker.workId);
    group(turnAnchor(index)).workers.push(worker);
  }
  for (const value of groups.values()) {
    value.workers.sort((a, b) => {
      const first = messages.findIndex(message => a.parentMessageId ? message.id === a.parentMessageId
        : a.parentTurnId ? message.turnId === a.parentTurnId && message.lane !== "worker" : message.workId === a.workId);
      const second = messages.findIndex(message => b.parentMessageId ? message.id === b.parentMessageId
        : b.parentTurnId ? message.turnId === b.parentTurnId && message.lane !== "worker" : message.workId === b.workId);
      return (first < 0 ? Infinity : first) - (second < 0 ? Infinity : second) || a.workId.localeCompare(b.workId);
    });
  }
  return groups;
}
