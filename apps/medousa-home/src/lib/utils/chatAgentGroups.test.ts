import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import type { SubagentRow } from "./subagentRows";
import { joinAgentGroups, projectChatAgentGroups } from "./chatAgentGroups";
import { presentChatMessages } from "./presentChatTurns";
import type { AgentPlacementGroup } from "$lib/types/generated/app_api";

function message(id: string, role: ChatMessage["role"], entrySeq: number): ChatMessage {
  return { id, role, content: id, transcript: { authorityId: "local", sessionId: "chat", entryId: id, entrySeq } };
}
function proposal(id: string, through: number, authority = "local"): PeerProposalReviewRecord {
  return { proposal: { proposal_id: id, request: { context: { sources: [{ selection: {
    session: { authority_id: authority, session_id: "chat" }, through_entry_seq: through,
  } }] } } } } as PeerProposalReviewRecord;
}
const messages = [message("request", "user", 1), message("spawn", "assistant", 2),
  message("later-request", "user", 3), message("later-reply", "assistant", 4)];

describe("joinAgentGroups", () => {
  it("joins thin Rust placement ids to full proposal and worker rows", () => {
    const projected: AgentPlacementGroup[] = [{
      anchorItemId: "request",
      entries: [
        { kind: "worker", workId: "work-z", nodeId: "spawn:segment:first" },
        { kind: "proposal", proposalId: "peer", originProposalId: "peer", nodeId: "spawn:segment:second" },
      ],
    }];
    const workers = new Map([["work-z", { workId: "work-z", parentMessageId: "spawn" } as SubagentRow]]);
    const groups = joinAgentGroups(projected, [proposal("peer", 1)], workers);
    expect([...groups.keys()]).toEqual(["request"]);
    expect(groups.get("request")?.entries?.map(entry => entry.kind)).toEqual(["worker", "proposal"]);
    expect(groups.get("request")?.entries?.[0]).toMatchObject({ kind: "worker", nodeId: "spawn:segment:first" });
  });

  it("skips entries whose full rows are not available", () => {
    const projected: AgentPlacementGroup[] = [{
      anchorItemId: null,
      entries: [{ kind: "worker", workId: "missing" }],
    }];
    expect(joinAgentGroups(projected, [], new Map()).get(null)?.entries).toEqual([]);
  });
});

describe("agent turn placement (Rust app core)", () => {
  it('keeps one durable conversation at its original spawn and retains the exact latest proposal', async () => {
    const first = proposal('first', 1), followup = proposal('followup', 3);
    for (const [index, row] of [first, followup].entries()) {
      row.proposal.request.context.created_at = `2026-10-30T14:0${index}:00Z`;
      row.proposal.request.target = { authority_id: 'local', execution_runtime_id: 'runtime' } as typeof row.proposal.request.target;
      row.binding = { execution_session: { authority_id: 'local', session_id: 'durable' } } as NonNullable<typeof row.binding>;
    }
    const groups = await projectChatAgentGroups(messages, messages, [followup, first], new Map());
    expect([...groups.keys()]).toEqual(['request']);
    expect(groups.get('request')?.proposals).toEqual([followup]);
    expect(groups.get('request')?.proposals[0]).toBe(followup);
    const foreign = structuredClone(followup);
    foreign.proposal.proposal_id = 'foreign';
    foreign.binding!.execution_session.authority_id = 'another-authority';
    expect([...(await projectChatAgentGroups(messages, messages, [first, foreign], new Map())).values()].flatMap(group => group.proposals)).toHaveLength(2);
  });
  it('orders mixed peers and workers by spawn segments, not kind, ID, or arrival', async () => {
    const history: ChatMessage[] = [messages[0], { ...messages[1], segments: [
      { kind: 'tool_group', groupId: 'first', toolRound: 1, runs: [{ runId: 'one', toolName: 'workshop.spawn', status: 'succeeded', round: 1, outputSummary: 'work-z' }] },
      { kind: 'tool_group', groupId: 'second', toolRound: 2, runs: [{ runId: 'two', toolName: 'peer_handoff', status: 'succeeded', round: 2, outputSummary: 'peer-middle' }] },
      { kind: 'handoff', handoffKind: 'worker', text: 'Delegated', workId: 'work-a' },
    ] }];
    const workers = new Map(['work-a', 'work-z'].map(workId => [workId, { workId, parentMessageId: 'spawn' } as SubagentRow]));
    const group = (await projectChatAgentGroups(history, history, [proposal('peer-middle', 1)], workers)).get('request')!;
    expect(group.entries?.map(entry => entry.kind === 'worker' ? entry.row.workId : entry.row.proposal.proposal_id)).toEqual(['work-z', 'peer-middle', 'work-a']);
    expect(group.entries?.map(entry => entry.nodeId)).toEqual(['spawn:segment:first', 'spawn:segment:second', 'spawn:segment:handoff:work-a:2']);
  });
  it('orders mixed delegations inside the same uninterrupted tool group by call order', async () => {
    const history: ChatMessage[] = [messages[0], { ...messages[1], segments: [{ kind: 'tool_group', groupId: 'batch', toolRound: 1, runs: [
      { runId: 'first', toolName: 'workshop.spawn', round: 1, status: 'succeeded', outputSummary: 'work-z' },
      { runId: 'second', toolName: 'peer_handoff', round: 2, status: 'succeeded', outputSummary: 'peer' },
    ] }] }];
    const group = (await projectChatAgentGroups(history, history, [proposal('peer', 1)], new Map([['work-z', { workId: 'work-z', parentMessageId: 'spawn' } as SubagentRow]]))).get('request');
    expect(group?.entries?.map(entry => entry.kind)).toEqual(['worker', 'proposal']);
    expect(group?.entries?.map(entry => entry.nodeId)).toEqual(['spawn:segment:batch', 'spawn:segment:batch']);
  });
  it('preserves admitted worker insertion order when legacy history has no segment coordinates', async () => {
    const workers = new Map(['work-z', 'work-a'].map(workId => [workId, { workId, parentMessageId: 'spawn' } as SubagentRow]));
    expect((await projectChatAgentGroups(messages, messages, [], workers)).get('request')?.workers.map(row => row.workId)).toEqual(['work-z', 'work-a']);
  });
  it('never matches a spawn ID from a later user turn', async () => {
    const history: ChatMessage[] = [...messages.slice(0, 3), { ...messages[3], segments: [{ kind: 'handoff', handoffKind: 'worker', text: 'Delegated', workId: 'work' }] }];
    const group = (await projectChatAgentGroups(history, history, [], new Map([['work', { workId: 'work', parentMessageId: 'spawn' } as SubagentRow]]))).get('request');
    expect(group?.entries?.[0].nodeId).toBeUndefined();
  });

  it("uses the parent turn when a refreshed link points at a later worker result", async () => {
    const history: ChatMessage[] = [messages[0], { ...messages[1], turnId: "parent" }, ...messages.slice(2),
      { id: "result", role: "assistant", content: "Done", lane: "worker", turnId: "parent", workId: "work" }];
    const workers = new Map([["work", { workId: "work", parentTurnId: "parent", parentMessageId: "result" } as SubagentRow]]);
    expect([...(await projectChatAgentGroups(history, history, [], workers)).keys()]).toEqual(["request"]);
  });
  it("keeps unknown spawn history in earlier work rather than attaching it to the result turn", async () => {
    const history: ChatMessage[] = [...messages.slice(2),
      { id: "result", role: "assistant", content: "Done", lane: "worker", workId: "work" }];
    for (const coordinates of [{}, { parentTurnId: "missing", parentMessageId: "missing" }]) {
      const workers = new Map([["work", { workId: "work", ...coordinates } as SubagentRow]]);
      expect([...(await projectChatAgentGroups(history, history, [], workers)).keys()]).toEqual([null]);
    }
  });
  it("groups nine agents at their initiating reply without moving them to later turns", async () => {
    const proposals = Array.from({ length: 9 }, (_, i) => proposal(`agent-${i}`, 1));
    const groups = await projectChatAgentGroups(messages, messages, proposals, new Map());
    expect([...groups.keys()]).toEqual(["request"]);
    expect(groups.get("request")?.proposals).toHaveLength(9);
    const completed = proposals.map(row => ({ ...row, receipt: { outcome: "completed", result: "Done" } } as PeerProposalReviewRecord));
    expect([...(await projectChatAgentGroups(messages, messages, completed, new Map())).keys()]).toEqual(["request"]);
  });
  it("keeps separate turns in separate groups after reloading history", async () => {
    const groups = await projectChatAgentGroups(messages, messages, [proposal("first", 1), proposal("second", 3)], new Map());
    expect(groups.get("request")?.proposals[0].proposal.proposal_id).toBe("first");
    expect(groups.get("later-request")?.proposals[0].proposal.proposal_id).toBe("second");
  });
  it("does not attach missing history or another authority's ranges to the latest turn", async () => {
    expect((await projectChatAgentGroups(messages, messages, [proposal("foreign", 1, "other")], new Map())).get(null)?.proposals).toHaveLength(1);
    const older = proposal("older", 0);
    expect((await projectChatAgentGroups(messages.slice(2), messages.slice(2), [older], new Map())).get(null)?.proposals).toHaveLength(1);
  });
  it("uses immutable source coordinates for imported conversations", async () => {
    const imported = messages.map(m => ({ ...m, transcript: { ...m.transcript!, authorityId: "derived", sessionId: "derived-chat", source: m.transcript } }));
    expect((await projectChatAgentGroups(imported, imported, [proposal("remote", 1)], new Map())).get("request")?.proposals).toHaveLength(1);
  });
  it("anchors newly delegated work during streaming before user history coordinates arrive", async () => {
    const live = [...messages.slice(0, 2), { id: "live-user", role: "user" as const, content: "Delegate again", createdAt: "2026-10-09T12:00:00Z" },
      { id: "live-reply", role: "assistant" as const, content: "", streaming: true }];
    const row = proposal("new-work", 3);
    row.proposal.request.context.created_at = "2026-10-09T12:00:01Z";
    const scope = { authority_id: "local", session_id: "chat" };
    expect((await projectChatAgentGroups(live, live, [row], new Map(), scope)).get("live-user")?.proposals).toHaveLength(1);
    const older = proposal("old-work", 1);
    older.proposal.request.context.created_at = row.proposal.request.context.created_at;
    expect((await projectChatAgentGroups(live, live, [older], new Map(), scope)).get("request")?.proposals).toHaveLength(1);
  });
  it("keeps a worker at its hidden spawn shell when synthesis arrives in a later turn", async () => {
    const shell: ChatMessage = { id: "shell", role: "assistant", content: "", turnId: "parent", stageWhisper: "Delegated" };
    const history = [messages[0], shell, ...messages.slice(2), { id: "result", role: "assistant" as const, content: "Result", lane: "worker" as const, workId: "work" }];
    const workers = new Map([["work", { workId: "work", parentTurnId: "parent", parentMessageId: "shell" } as SubagentRow]]);
    const groups = await projectChatAgentGroups(history, presentChatMessages(history), [], workers);
    expect(groups.get("request")?.workers).toHaveLength(1);
    expect(groups.has("later-reply")).toBe(false);
  });
});
