import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ChatStoreHost } from "./chatStoreHost";
import type { TurnTicketResponse } from "$lib/types/session";
const api = vi.hoisted(() => ({ start: vi.fn(), stop: vi.fn() }));
vi.mock("$lib/daemon", () => ({ startInteractiveStream: api.start, stopInteractiveStreamTurn: api.stop }));
vi.mock("$lib/stores/narration.svelte", () => ({ narration: { stop: vi.fn() } }));
vi.mock("$lib/chat/sessionController", () => ({ mapTurns: () => [], TRANSCRIPT_PAGE_SIZE: 32 }));
vi.mock("$lib/chat/workerLaneController", () => ({ workerLinkForTurn: () => undefined }));
import { registerTurn, settleTurn, startTurnStream } from "./streamLifecycleController";
import { applyPumpedStreamEvent } from "./streamApplyController";
import { applyStreamSeq } from "$lib/stream/reconnect";

beforeEach(() => { vi.clearAllMocks(); vi.useFakeTimers(); });
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
function host(): ChatStoreHost {
  return {
    workshopScopeId: "studio", workshopEpoch: 1, sessionId: "chat", messages: [], turns: new Map(), workers: new Map(),
    streamOwners: new Map(), lastSeqByTurn: new Map(), recentlySettledTurns: new Map(), terminalReconcileTimers: new Map(),
    activeTurnId: null, assistantId: null, backgroundActivity: 0,
    withSessionFields: (_: string, run: () => void) => run(),
  } as unknown as ChatStoreHost;
}
function ticket(id: string, streamUrl: string): TurnTicketResponse {
  return { turn_id: id, session_id: "chat", mode: "interactive", phase: "accepted", accepted_at_utc: "2026-10-23T00:00:00Z", stream_url: streamUrl, stream_ready: true };
}
it("continues an agent session after the previous prompt without replaying its terminal event", async () => {
  const state = host(); const url = "/v1/agents/sessions/provider-session/stream";
  const accepted = ticket("provider-session", url);
  registerTurn(state, accepted, "answer-one");
  await startTurnStream(state, accepted.turn_id, accepted.session_id, url);
  expect(api.start).toHaveBeenLastCalledWith(url);
  applyStreamSeq(state.lastSeqByTurn, { turn_id: accepted.turn_id, seq: 7 });
  settleTurn(state, accepted.turn_id);
  expect(state.lastSeqByTurn.get(accepted.turn_id)).toBe(7);

  registerTurn(state, accepted, "answer-two");
  await startTurnStream(state, accepted.turn_id, accepted.session_id, url);
  expect(api.start).toHaveBeenLastCalledWith(`${url}?since=7`);
  // Inclusive/redundant delivery of the old final must not settle the new prompt.
  applyPumpedStreamEvent(state, { sessionId: "chat", event: {
    schema_version: 3, turn_id: accepted.turn_id, seq: 7, emitted_at_utc: "2026-10-23T00:00:00Z",
    event: { type: "turn_completed", outcome: "completed", aggregate_text: "Previous answer" },
  } });
  expect(state.turns.get(accepted.turn_id)?.messageId).toBe("answer-two");
  expect(state.activeTurnId).toBe(accepted.turn_id);
  expect(applyStreamSeq(state.lastSeqByTurn, { turn_id: accepted.turn_id, seq: 8 })).toBe(true);
});
it("keeps native turn cursor cleanup unchanged", async () => {
  const state = host(); const url = "/v1/interactive/turn/native-turn/stream";
  const accepted = ticket("native-turn", url);
  registerTurn(state, accepted, "native-answer");
  await startTurnStream(state, accepted.turn_id, accepted.session_id, url);
  state.lastSeqByTurn.set(accepted.turn_id, 7);
  settleTurn(state, accepted.turn_id);
  expect(state.lastSeqByTurn.has(accepted.turn_id)).toBe(false);
});
it("retains an external cursor even when a recoverable disconnect evicted its stream owner", async () => {
  const state = host(); const url = "http://workshop:7419/v1/agents/sessions/provider-session/stream";
  registerTurn(state, ticket("provider-session", url), "answer");
  state.lastSeqByTurn.set("provider-session", 4);
  settleTurn(state, "provider-session");
  expect(state.lastSeqByTurn.get("provider-session")).toBe(4);
});
