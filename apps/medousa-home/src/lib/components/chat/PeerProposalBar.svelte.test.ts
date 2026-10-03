/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import PeerProposalBar from "./PeerProposalBar.svelte";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";

const api = vi.hoisted(() => ({ list: vi.fn(), act: vi.fn() }));
const workshopState = vi.hoisted(() => ({ activeWorkshopId: "local", activeLabel: "Mac mini", switching: false, workshops: [] as { kind: string; pairing?: { workshopDeviceId: string } }[] }));
vi.mock("$lib/daemon/coordination", () => ({
  listPeerProposals: api.list, actOnPeerProposal: api.act,
  proposalExecutionTransport: (kind: string, runtime: string) => kind === "portal" ? runtime : null,
}));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: { online: true, health: { active_profile_id: "owner", runtime: { advertised_capabilities: ["coordination.operator_proposals.v1"] } } } }));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: workshopState }));
vi.mock("$lib/platform", () => ({ isTauri: () => true }));
vi.mock("$lib/remotePeerCompletionSync", () => ({ requestRemotePeerCompletionSync: vi.fn() }));

let component: ReturnType<typeof mount> | undefined;
const now = Date.parse("2026-10-03T12:00:00Z");
function record(id: string, runtime = "local"): PeerProposalReviewRecord {
  const authority = `auth_${"a".repeat(64)}`;
  const target = { runtime: "medousa" as const, execution_runtime_id: runtime, authority_id: authority };
  const channel = { authority_id: authority, channel_id: "channel" };
  const executionSession = { authority_id: authority, session_id: `ses_executor_${id}` };
  return {
    proposal: { proposal_id: id, expires_at: new Date(now + 3_600_000).toISOString(), continue_owner: false, request: {
      assignment_id: id, idempotency_key: id, owner_principal_id: "owner",
      owner_session: { authority_id: authority, session_id: "ses_owner" }, execution_session: executionSession,
      execution_grant_id: "grant", instructions: `Assignment ${id}`, target,
      context: { manifest_id: "context", created_by: "owner", created_at: new Date(now).toISOString(), sources: [] }, channel, forge_work_id: "work",
    } },
    decision: null, binding: { agent_session_id: `turn-${id}`, assignment_id: id, channel, execution_session: executionSession, owner_principal_id: "owner", target },
    progress: { state: "running", observed_at: new Date(now).toISOString(), last_activity_at: new Date(now).toISOString(), current_activity: "Run verification" },
  };
}
async function settle() { await tick(); await vi.advanceTimersByTimeAsync(0); flushSync(); }
async function render() {
  const props = $state({ sessionId: "ses_owner" });
  const target = document.createElement("div"); document.body.append(target);
  component = mount(PeerProposalBar, { target, props });
  await settle();
  return props;
}
beforeEach(() => {
  vi.useFakeTimers(); vi.setSystemTime(now); vi.clearAllMocks();
  workshopState.workshops = [];
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  api.list.mockResolvedValue({ proposals: [], next_cursor: null });
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe("assignment card lifecycle", () => {
  it("shows live work and retains the selected terminal result after it leaves the inbox", async () => {
    const work = record("proposal-one");
    api.list.mockResolvedValueOnce({ proposals: [work], next_cursor: null });
    await render();
    expect(document.body.textContent).toContain("Agent is working");
    expect(document.body.textContent).toContain("Run verification");
    api.list.mockResolvedValue({ proposals: [], next_cursor: null, tracked_proposal: { ...work, receipt: { outcome: "completed", result: "Verified result" } } });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(api.list).toHaveBeenLastCalledWith("ses_owner", null, undefined, "proposal-one");
    expect(document.body.textContent).toContain("Work completed");
    expect(document.body.textContent).toContain("Verified result");
    expect(document.body.textContent).not.toContain("Run verification");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("keeps the last observed assignment when its workshop cannot be refreshed", async () => {
    api.list.mockResolvedValueOnce({ proposals: [record("proposal-one")], next_cursor: null });
    await render(); api.list.mockRejectedValue(new Error("Workshop unavailable"));
    await vi.advanceTimersByTimeAsync(60_000); await settle();
    expect(document.body.textContent).toContain("Assignment proposal-one");
    expect(document.body.textContent).toContain("Progress updates are delayed");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("keeps remote pagination and selected-assignment polling pinned to the serving workshop", async () => {
    workshopState.workshops = [{ kind: "portal", pairing: { workshopDeviceId: "remote" } }];
    api.list.mockImplementation(async (_session, runtime, after) => runtime === "remote"
      ? { proposals: [record(after ? "second" : "first", "remote")], next_cursor: after ? null : "remote-cursor" }
      : { proposals: [], next_cursor: null });
    await render();
    const more = [...document.querySelectorAll("button")].find(button => button.textContent === "More requests")!;
    more.click(); await settle();
    expect(api.list).toHaveBeenCalledWith("ses_owner", "remote", "remote-cursor");
    expect(document.body.textContent).toContain("Assignment second");
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(api.list).toHaveBeenCalledWith("ses_owner", "remote", "remote-cursor", "second");
    expect(api.list).not.toHaveBeenCalledWith("ses_owner", null, expect.anything(), "second");
  });

  it("ignores an older session's response after the chat changes", async () => {
    let resolve!: (response: unknown) => void;
    api.list.mockImplementation((session: string) => session === "ses_owner"
      ? new Promise(done => { resolve = done; })
      : Promise.resolve({ proposals: [record("new-session")], next_cursor: null }));
    const props = await render();
    props.sessionId = "ses_other"; await settle();
    resolve({ proposals: [record("old-session")], next_cursor: null }); await settle();
    expect(document.body.textContent).toContain("Assignment new-session");
    expect(document.body.textContent).not.toContain("Assignment old-session");
  });
});
