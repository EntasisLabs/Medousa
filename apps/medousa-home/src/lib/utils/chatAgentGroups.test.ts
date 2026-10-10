import { describe, expect, it } from "vitest";
import type { ChatMessage } from "$lib/types/chat";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import type { SubagentRow } from "./subagentRows";
import { chatAgentGroups } from "./chatAgentGroups";
import { presentChatMessages } from "./presentChatTurns";

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

describe("agent turn placement", () => {
  it("groups nine agents at their initiating reply without moving them to later turns", () => {
    const proposals = Array.from({ length: 9 }, (_, i) => proposal(`agent-${i}`, 1));
    const groups = chatAgentGroups(messages, messages, proposals, new Map());
    expect([...groups.keys()]).toEqual(["request"]);
    expect(groups.get("request")?.proposals).toHaveLength(9);
    const completed = proposals.map(row => ({ ...row, receipt: { outcome: "completed", result: "Done" } } as PeerProposalReviewRecord));
    expect([...chatAgentGroups(messages, messages, completed, new Map()).keys()]).toEqual(["request"]);
  });
  it("keeps separate turns in separate groups after reloading history", () => {
    const groups = chatAgentGroups(messages, messages, [proposal("first", 1), proposal("second", 3)], new Map());
    expect(groups.get("request")?.proposals[0].proposal.proposal_id).toBe("first");
    expect(groups.get("later-request")?.proposals[0].proposal.proposal_id).toBe("second");
  });
  it("does not attach missing history or another authority's ranges to the latest turn", () => {
    expect(chatAgentGroups(messages, messages, [proposal("foreign", 1, "other")], new Map()).get(null)?.proposals).toHaveLength(1);
    const older = proposal("older", 0);
    expect(chatAgentGroups(messages.slice(2), messages.slice(2), [older], new Map()).get(null)?.proposals).toHaveLength(1);
  });
  it("uses immutable source coordinates for imported conversations", () => {
    const imported = messages.map(m => ({ ...m, transcript: { ...m.transcript!, authorityId: "derived", sessionId: "derived-chat", source: m.transcript } }));
    expect(chatAgentGroups(imported, imported, [proposal("remote", 1)], new Map()).get("request")?.proposals).toHaveLength(1);
  });
  it("anchors newly delegated work during streaming before user history coordinates arrive", () => {
    const live = [...messages.slice(0, 2), { id: "live-user", role: "user" as const, content: "Delegate again", createdAt: "2026-10-09T12:00:00Z" },
      { id: "live-reply", role: "assistant" as const, content: "", streaming: true }];
    const row = proposal("new-work", 3);
    row.proposal.request.context.created_at = "2026-10-09T12:00:01Z";
    const scope = { authority_id: "local", session_id: "chat" };
    expect(chatAgentGroups(live, live, [row], new Map(), scope).get("live-user")?.proposals).toHaveLength(1);
    // A proposal sharing an older range must stay with the older request.
    const older = proposal("old-work", 1);
    older.proposal.request.context.created_at = row.proposal.request.context.created_at;
    expect(chatAgentGroups(live, live, [older], new Map(), scope).get("request")?.proposals).toHaveLength(1);
  });
  it("keeps a worker at its hidden spawn shell when synthesis arrives in a later turn", () => {
    const shell: ChatMessage = { id: "shell", role: "assistant", content: "", turnId: "parent", stageWhisper: "Delegated" };
    const history = [messages[0], shell, ...messages.slice(2), { id: "result", role: "assistant" as const, content: "Result", lane: "worker" as const, workId: "work" }];
    const workers = new Map([["work", { workId: "work", parentTurnId: "parent", parentMessageId: "shell" } as SubagentRow]]);
    const groups = chatAgentGroups(history, presentChatMessages(history), [], workers);
    expect(groups.get("request")?.workers).toHaveLength(1);
    expect(groups.has("later-reply")).toBe(false);
  });
});
