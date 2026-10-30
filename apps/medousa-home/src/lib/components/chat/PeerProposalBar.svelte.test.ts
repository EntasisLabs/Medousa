/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import PeerProposalBar from "./PeerProposalBar.svelte";
import { connection } from "$lib/stores/connection.svelte";
import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";
import type { PeerProposalControls } from './peerProposalControls';
import type { ChatMessage } from '$lib/types/chat';
const stream = $state({ messages: [] as ChatMessage[] });

const api = vi.hoisted(() => ({ list: vi.fn(), act: vi.fn() }));
const navigation = vi.hoisted(() => ({ openChat: vi.fn(() => 'tab'), openProject: vi.fn() }));
const workshopState = vi.hoisted(() => ({ activeWorkshopId: "local", activeLabel: "Mac mini", switching: false, workshops: [] as { id?: string; label?: string; kind: string; pairing?: { workshopDeviceId: string } }[] }));
vi.mock("$lib/daemon/coordination", () => ({
  listPeerProposals: api.list, actOnPeerProposal: api.act,
  proposalExecutionTransport: (kind: string, runtime: string) => kind === "portal" ? runtime : null,
}));
vi.mock("$lib/stores/connection.svelte", () => {
  const state = $state({ online: true, health: { active_profile_id: "owner", runtime: { authority_id: `auth_${"a".repeat(64)}`, advertised_capabilities: ["coordination.operator_proposals.v1"] } } });
  return { connection: state };
});
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: workshopState }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessions: [{ session_id: "ses_owner", display_name: "Taco" }], messagesFor: () => stream.messages } }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: { active: null, items: [] } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openChat: navigation.openChat } }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { openCodeWorkspace: navigation.openProject } }));
vi.mock("$lib/platform", () => ({ isTauri: () => true, isTauriMobilePlatform: () => false }));
vi.mock("$lib/components/chat/ToolActivitySheet.svelte", () => ({ default: vi.fn() }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { runtimeLabel: () => "Mac mini" } }));
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
function task(title: string) {
  const button = [...document.querySelectorAll<HTMLButtonElement>('button.task-receipt')].find(button => button.querySelector('strong')?.textContent === title);
  expect(button, `Task receipt for ${title}`).toBeDefined();
  return button!;
}
async function inspect(title: string) { task(title).click(); await settle(); return document.querySelector('dialog[open]')!; }
async function render(onSnapshot?: (session: string | null, rows: PeerProposalReviewRecord[], controls: PeerProposalControls) => void) {
  const props = $state({ sessionId: "ses_owner", onSnapshot });
  const target = document.createElement("div"); document.body.append(target);
  component = mount(PeerProposalBar, { target, props });
  await settle();
  return props;
}
beforeEach(() => {
  vi.useFakeTimers(); vi.setSystemTime(now); vi.clearAllMocks(); stream.messages = [];
  workshopState.workshops = [];
  Object.assign(connection, { online: true, health: { active_profile_id: "owner", runtime: { authority_id: `auth_${"a".repeat(64)}`, advertised_capabilities: ["coordination.operator_proposals.v1"] } } });
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.open = false; this.dispatchEvent(new Event('close')); });
  api.list.mockResolvedValue({ proposals: [], next_cursor: null });
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.restoreAllMocks(); vi.useRealTimers(); });

describe("assignment card lifecycle", () => {
  it('discovers a new remote delegation on its tool event without waiting for the polling cooldown', async () => {
    workshopState.workshops = [{ kind: 'portal', pairing: { workshopDeviceId: 'remote' } }];
    await render();
    api.list.mockClear();
    api.list.mockImplementation(async (_session, runtime) => ({ proposals: runtime === 'remote' ? [record('fresh', 'remote')] : [] }));
    stream.messages = [{ id: 'host', role: 'assistant', content: '', toolRuns: [{ runId: 'spawn', toolName: 'peer_handoff', round: 1, status: 'succeeded' }] }];
    await settle();
    expect(api.list).toHaveBeenCalledWith('ses_owner', 'remote', undefined, undefined);
    expect(document.body.textContent).toContain('Assignment fresh');
  });

  it('replays a delegation invalidation arriving during remote discovery instead of losing it to the cooldown', async () => {
    workshopState.workshops = [{ kind: 'portal', pairing: { workshopDeviceId: 'remote' } }];
    let finish!: (value: { proposals: PeerProposalReviewRecord[] }) => void;
    let remoteCalls = 0;
    api.list.mockImplementation(async (_session, runtime) => {
      if (runtime !== 'remote') return { proposals: [] };
      if (++remoteCalls === 1) return new Promise(resolve => { finish = resolve; });
      return { proposals: [record('queued', 'remote')] };
    });
    await render();
    stream.messages = [{ id: 'host', role: 'assistant', content: '', segments: [{ kind: 'tool_group', groupId: 'spawn', toolRound: 1,
      runs: [{ runId: 'spawn', toolName: 'peer_handoff', round: 1, status: 'succeeded' }] }] }];
    await settle();
    finish({ proposals: [] }); await settle();
    expect(remoteCalls).toBe(2);
    expect(document.body.textContent).toContain('Assignment queued');
  });
  it('does not poll again for ordinary text or unrelated tool updates', async () => {
    await render(); api.list.mockClear();
    stream.messages = [{ id: 'host', role: 'assistant', content: 'Working', toolRuns: [{ runId: 'read', toolName: 'code.read', round: 1, status: 'succeeded' }] }];
    await settle();
    expect(api.list).not.toHaveBeenCalled();
  });
  it('shares the same polled snapshot and controls with the composer dock', async () => {
    const snapshot = vi.fn();
    api.list.mockResolvedValue({ proposals: [record('one')] });
    const props = await render(snapshot);
    expect(snapshot.mock.lastCall?.[0]).toBe('ses_owner');
    expect(snapshot.mock.lastCall?.[1][0].proposal.proposal_id).toBe('one');
    expect(snapshot.mock.lastCall?.[2].action).toBeTypeOf('function');
    expect(api.list).toHaveBeenCalledTimes(1);
    expect(api.act).not.toHaveBeenCalled();
    api.list.mockResolvedValue({ proposals: [] });
    props.sessionId = 'other'; await settle();
    expect(snapshot.mock.lastCall?.[0]).toBe('other');
    expect(snapshot.mock.lastCall?.[1]).toEqual([]);
  });
  it.each(["completed", "failed", "cancelled"] as const)("restores %s work after reopening without an in-memory selected id", async outcome => {
    const work = record("saved-work");
    work.receipt = { receipt_id: "receipt", binding: work.binding!, outcome, result: "Saved agent result" };
    api.list.mockResolvedValue({ proposals: [work], next_cursor: null });
    await render();
    await unmount(component!); component = undefined; document.body.replaceChildren();
    api.list.mockClear();
    await render();
    expect(api.list).toHaveBeenCalledWith("ses_owner", null, undefined, undefined);
    expect(document.body.textContent).toContain("Assignment saved-work");
    expect(document.body.textContent).not.toContain("Saved agent result");
    const detail = await inspect('Assignment saved-work');
    expect(detail.textContent).toContain("Saved agent result");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("keeps loaded work readable through missing health and reconnect, but clears another profile's work", async () => {
    const work = record("saved-work");
    api.list.mockResolvedValue({ proposals: [work], next_cursor: null });
    await render();
    const health = connection.health!;
    Object.assign(connection, { online: false, health: null }); await settle();
    expect(document.body.textContent).toContain("Assignment saved-work");
    const calls = api.list.mock.calls.length;
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(api.list).toHaveBeenCalledTimes(calls);
    Object.assign(connection, { online: true, health }); await settle();
    expect(api.list).toHaveBeenLastCalledWith("ses_owner", null, undefined, "saved-work");
    api.list.mockResolvedValue({ proposals: [], next_cursor: null });
    connection.health = { ...health, active_profile_id: "another-owner" }; await settle();
    expect(document.body.textContent).not.toContain("Assignment saved-work");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("retains every observed request on a workshop whose refresh fails", async () => {
    workshopState.workshops = [{ kind: "portal", pairing: { workshopDeviceId: "remote" } }];
    api.list.mockImplementation(async (_session, runtime) => ({ proposals: runtime === "remote" ? [record("one", "remote"), record("two", "remote")] : [] }));
    await render();
    api.list.mockImplementation(async (_session, runtime) => {
      if (runtime === "remote") throw new Error("Workshop unavailable");
      return { proposals: [] };
    });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(document.querySelectorAll('button.task-receipt')).toHaveLength(2);
    expect(document.body.textContent).toContain("Assignment one");
    expect(document.body.textContent).toContain("Assignment two");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("shows live work and retains the selected terminal result after it leaves the inbox", async () => {
    const work = record("proposal-one");
    api.list.mockResolvedValueOnce({ proposals: [work], next_cursor: null });
    await render();
    expect(task('Assignment proposal-one').textContent).toContain("Agent working");
    const detail = await inspect('Assignment proposal-one');
    expect(detail.textContent).toContain("Run verification");
    api.list.mockResolvedValue({ proposals: [], next_cursor: null, tracked_proposal: { ...work, receipt: { outcome: "completed", result: "Verified result" } } });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(api.list).toHaveBeenLastCalledWith("ses_owner", null, undefined, "proposal-one");
    expect(document.body.textContent).toContain("Result returned");
    expect(task('Assignment proposal-one').textContent).not.toContain('Agent working');
    expect(document.body.textContent).toContain("Verified result");
    expect(document.body.textContent).not.toContain("Run verification");
    expect(api.act).not.toHaveBeenCalled();
  });

  it("keeps the last observed assignment when its workshop cannot be refreshed", async () => {
    api.list.mockResolvedValueOnce({ proposals: [record("proposal-one")], next_cursor: null });
    await render(); api.list.mockRejectedValue(new Error("Workshop unavailable"));
    await vi.advanceTimersByTimeAsync(60_000); await settle();
    expect(document.body.textContent).toContain("Assignment proposal-one");
    const detail = await inspect('Assignment proposal-one');
    expect(detail.textContent).toContain("Progress updates are delayed");
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
    expect(api.list).toHaveBeenCalledWith("ses_owner", "remote", "remote-cursor", undefined);
    expect(document.body.textContent).toContain("Assignment first");
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

describe("delegation context line", () => {
  it("opens the execution chat rather than the provider agent id, only after a user click", async () => {
    api.list.mockResolvedValue({ proposals: [record('native')] });
    await render();
    expect(navigation.openChat).not.toHaveBeenCalled();
    await inspect('Assignment native');
    [...document.querySelectorAll('button')].find(button => button.textContent?.includes('Open agent conversation'))!.click();
    await settle();
    expect(navigation.openChat).toHaveBeenCalledWith('ses_executor_native', { title: 'Medousa Coder', activate: true });
    expect(api.act).not.toHaveBeenCalled();
  });

  it("offers the execution workshop instead of opening remote ids on the active daemon", async () => {
    workshopState.workshops = [{ id: 'remote-workshop', label: 'Studio', kind: 'portal', pairing: { workshopDeviceId: 'remote' } }];
    const remote = record('remote-native', 'remote');
    remote.proposal.request.target.authority_id = 'remote-authority';
    api.list.mockImplementation(async (_session, runtime) => ({ proposals: runtime === 'remote' ? [remote] : [] }));
    await render();
    await inspect('Assignment remote-native');
    const labels = [...document.querySelectorAll('button')].map(button => button.textContent);
    expect(labels.some(label => label?.includes('Open Studio'))).toBe(true);
    expect(labels.some(label => label?.includes('Open project') || label?.includes('Open agent conversation'))).toBe(false);
    expect(navigation.openChat).not.toHaveBeenCalled();
    expect(navigation.openProject).not.toHaveBeenCalled();
  });

  it("keeps assignment details behind a task receipt and preserves the open dialog through progress updates", async () => {
    const work = record("compact-work");
    work.proposal.request.instructions = 'Scaffold Penjamin\nLong assignment details stay behind the disclosure.';
    api.list.mockResolvedValue({ proposals: [work] });
    await render();
    const summary = task('Scaffold Penjamin');
    expect(document.querySelector('dialog')?.open).toBe(false);
    expect(summary.textContent).not.toContain('Long assignment');
    expect(document.body.textContent).not.toContain('Long assignment');
    const details = await inspect('Scaffold Penjamin');
    expect(details.textContent).toContain('Long assignment details stay behind the disclosure.');
    expect(details.textContent).toContain('Run verification');
    api.list.mockResolvedValue({ proposals: [], tracked_proposal: { ...work, progress: { ...work.progress, current_activity: 'Build application' } } });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(document.querySelector('dialog[open]')).toBe(details);
    expect(document.body.textContent).toContain('Build application');
    expect(api.act).not.toHaveBeenCalled();
  });

  it("keeps a completed worker awaiting sender review until the sender accepts", async () => {
    const work = record('review-work');
    work.receipt = { ...work.receipt, outcome: 'completed', result: 'Worker finished' } as NonNullable<typeof work.receipt>;
    work.handoff = { admission: 'delegate', policy: { completion: 'sender_review', responsibility: 'retain' }, responsible_session: work.proposal.request.owner_session, state: 'awaiting_sender_review' };
    api.list.mockResolvedValue({ proposals: [work] });
    await render();
    expect(task('Assignment review-work').textContent).toContain('Awaiting sender review');
    const detail = await inspect('Assignment review-work');
    expect(detail.textContent).toContain('Awaiting sender review');
    expect(detail.textContent).toContain('Worker finished');
    expect(document.body.textContent).not.toContain('Result accepted');
    expect([...document.querySelectorAll('button')].some(button => button.textContent?.includes('Approve'))).toBe(false);
    api.list.mockResolvedValue({ proposals: [], tracked_proposal: { ...work, handoff: { ...work.handoff, state: 'accepted', review: { verdict: 'accept', reason: 'Checks and scope verified', receipt_id: 'receipt', sender_session: work.proposal.request.owner_session, turn_id: 'review-turn' } } } });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(task('Assignment review-work').textContent).toContain('Result accepted');
    expect(detail.textContent).toContain('Checks and scope verified');
    expect(api.act).not.toHaveBeenCalled();
  });

  it("shows approval actions for proposals while a direct handoff waits for admission without another approval", async () => {
    const proposal = record('suggested-work');
    proposal.binding = null;
    proposal.progress = null;
    api.list.mockResolvedValue({ proposals: [proposal] });
    await render();
    expect([...document.querySelectorAll('button')].some(button => button.textContent === 'Approve & start')).toBe(false);
    await inspect('Assignment suggested-work');
    expect([...document.querySelectorAll('button')].some(button => button.textContent === 'Approve & start')).toBe(true);
    const direct = { ...proposal, handoff: { admission: 'delegate' as const, policy: {}, responsible_session: proposal.proposal.request.owner_session, state: 'awaiting_acceptance' as const } };
    api.list.mockResolvedValue({ proposals: [], tracked_proposal: direct });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(task('Assignment suggested-work').textContent).toContain('Waiting for agent');
    expect([...document.querySelectorAll('button')].some(button => button.textContent?.includes('Approve'))).toBe(false);
    expect(api.act).not.toHaveBeenCalled();
  });
});


describe("grouped agent activity", () => {
  it("shows nine compact task receipts and preserves their order and selected detail as results arrive", async () => {
    const rows = Array.from({ length: 9 }, (_, i) => record(`task-${i}`));
    api.list.mockResolvedValue({ proposals: rows });
    await render();
    expect(document.querySelectorAll('button.task-receipt')).toHaveLength(9);
    expect(document.querySelector('details.agent-work-line')).toBeNull();
    const row = task('Assignment task-5');
    const detail = await inspect('Assignment task-5');
    const finished = rows.map(work => ({ ...work, receipt: { receipt_id: `result-${work.proposal.proposal_id}`, binding: work.binding!, outcome: 'completed' as const, result: 'Checks passed' } }));
    api.list.mockResolvedValue({ proposals: [...finished].reverse() });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect([...document.querySelectorAll('button.task-receipt strong')].map(el => el.textContent)).toEqual(rows.map(work => work.proposal.request.instructions));
    expect(task('Assignment task-5')).toBe(row);
    expect(document.querySelector('dialog[open]')).toBe(detail);
    expect(detail.textContent).toContain('Assignment task-5');
    expect(detail.textContent).toContain('Checks passed');
    expect([...document.querySelectorAll('button.task-receipt')].every(button => button.textContent?.includes('Result returned'))).toBe(true);
  });

  it("tracks all results when multiple assignments leave the inbox", async () => {
    const first = record('one'), second = record('two');
    api.list.mockResolvedValue({ proposals: [first, second] });
    await render();
    api.list.mockImplementation(async (_session, _runtime, _after, selected) => ({ proposals: [], tracked_proposal: selected ? { ...(selected === 'one' ? first : second), receipt: { outcome: 'completed', result: `Result ${selected}` } } : null }));
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect((await inspect('Assignment one')).textContent).toContain('Result one');
    expect((await inspect('Assignment two')).textContent).toContain('Result two');
    expect(api.list).toHaveBeenCalledWith('ses_owner', null, undefined, 'two');
  });

  it("approves the clicked task and preserves approval when its startup fails", async () => {
    const first = record('one'), second = record('two');
    first.binding = null; second.binding = null;
    api.list.mockResolvedValue({ proposals: [first, second] });
    api.act.mockImplementation(async (_proposal, action) => { if (action === 'dispatch') throw new Error('Startup unavailable'); return {}; });
    await render();
    const affected = await inspect('Assignment two');
    [...affected.querySelectorAll('button')].find(button => button.textContent === 'Approve & start')!.click();
    await settle();
    expect(api.act).toHaveBeenCalledWith(second.proposal, 'approve', null);
    expect(api.act).toHaveBeenCalledWith(second.proposal, 'dispatch', null);
    expect(api.act).not.toHaveBeenCalledWith(first.proposal, expect.anything(), expect.anything());
    expect(document.body.textContent).toContain('Start approved work');
    expect(affected.textContent).toContain('Startup unavailable');
    expect(task('Assignment two').textContent).toContain('Decision needed');
    expect([...affected.querySelectorAll('button')].find(button => button.textContent === 'Start approved work')?.disabled).toBe(false);
    expect(api.act).toHaveBeenCalledTimes(2);
    expect((await inspect('Assignment one')).textContent).not.toContain('Startup unavailable');
    await inspect('Assignment two');
    api.list.mockResolvedValue({ proposals: [first, { ...second, decision: { approved: true } }] });
    await vi.advanceTimersByTimeAsync(15_000); await settle();
    expect(affected.textContent).toContain('Startup unavailable');
  });
});
