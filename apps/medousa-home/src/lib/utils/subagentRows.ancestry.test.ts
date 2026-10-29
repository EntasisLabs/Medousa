import { afterEach, describe, expect, it, vi } from "vitest";
const state = vi.hoisted(() => ({
  chat: { workers: new Map() },
  workspace: { cards: [{ id: "work", column: "in_flight", status_label: "Working" }], cardDetailsCache: new Map([["work", { kind: "turn_worker", work_id: "work", session_id: "chat", correlation_id: "parent", card: { title: "Task" } }]]) },
}));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: state.chat }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: state.workspace }));
vi.mock("$lib/work/workerTranscripts.svelte", () => ({ workerTranscripts: { transcriptFor: () => null } }));
import { subagentRowsForSession } from "./subagentRows";

afterEach(() => { state.workspace.cards[0].column = 'in_flight'; state.workspace.cards[0].status_label = 'Working'; });

describe("worker card ancestry", () => {
  it.each(['wrapping_up', 'backlog', 'blocked', 'done'])("does not mark %s workers as actively streaming", column => {
    state.workspace.cards[0].column = column;
    state.workspace.cards[0].status_label = column === 'wrapping_up' ? 'synthesis pending' : column;
    const row = subagentRowsForSession('chat')[0];
    expect(row.streaming).toBe(false);
    if (column === 'wrapping_up') {
      expect(row.terminal).toBe(false);
      expect(row.statusLine).toBe('Result ready · Awaiting summary');
    }
  });
  it('marks running workers as streaming', () => {
    expect(subagentRowsForSession('chat')[0].streaming).toBe(true);
  });
  it("uses the persisted parent turn before a live worker link arrives", () => {
    expect(subagentRowsForSession("chat")[0]).toMatchObject({ workId: "work", parentTurnId: "parent" });
  });
});
