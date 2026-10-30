/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ChatWorkDock from './ChatWorkDock.svelte';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';
import type { PeerProposalControls } from './peerProposalControls';
import type { SubagentRow } from '$lib/utils/subagentRows';

vi.mock('$lib/stores/chat.svelte', () => ({ chat: { sessions: [] } }));
vi.mock('$lib/stores/undertakings.svelte', () => ({ undertakings: { active: null, items: [{ id: 'work-a', title: 'Medousa' }, { id: 'work-b', title: 'rndr' }] } }));
vi.mock('$lib/stores/connection.svelte', () => ({ connection: { health: { runtime: { authority_id: 'local' } } } }));
vi.mock('$lib/stores/workshops.svelte', () => ({ workshops: { workshops: [], activeWorkshopId: 'local', activeLabel: 'Mac mini' } }));
vi.mock('$lib/stores/executionTargets.svelte', () => ({ executionTargets: { runtimeLabel: () => 'Mac mini' } }));
vi.mock('./ToolActivitySheet.svelte', () => ({ default: vi.fn() }));

function request(id: string, work = 'work-a'): PeerProposalReviewRecord {
  return {
    proposal: { proposal_id: id, expires_at: '2099-01-01T00:00:00Z', request: {
      instructions: `Task ${id}`, forge_work_id: work,
      owner_session: { authority_id: 'local', session_id: 'chat' },
      target: { authority_id: 'local', execution_runtime_id: 'local', runtime: 'medousa' },
      context: { sources: [] }, channel: { channel_id: 'chat' },
    } },
  } as unknown as PeerProposalReviewRecord;
}
function controls(): PeerProposalControls {
  return { now: Date.now(), available: true, busy: false, busyId: null, feedback: null,
    hasMore: false, loadMore: vi.fn(), action: vi.fn(), openExecution: vi.fn() };
}
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); localStorage.clear(); vi.restoreAllMocks(); });
function render(proposals: PeerProposalReviewRecord[], workers: SubagentRow[] = []) {
  const props = $state({ sessionId: 'chat', proposals, workers, controls: controls(), onOpenWorker: vi.fn() });
  const target = document.createElement('div'); document.body.append(target);
  component = mount(ChatWorkDock, { target, props }); flushSync();
  return props;
}
function button(text: string) {
  const result = [...document.querySelectorAll('button')].find(item => item.textContent?.includes(text));
  if (!result) throw new Error(`Missing button: ${text}`);
  return result;
}
async function click(text: string) { button(text).click(); await tick(); flushSync(); }

describe('composer work dock', () => {
  it('uses a decorative agent icon and a stateful chevron without adding glyphs to the label', async () => {
    render([request('one')]);
    const trigger = document.querySelector<HTMLButtonElement>('.work-pill')!;
    expect(trigger.querySelectorAll('svg[aria-hidden="true"]')).toHaveLength(2);
    expect(trigger.textContent?.trim()).toBe('1 task');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    await click('1 task');
    expect(trigger.querySelector('.chevron.expanded')).not.toBeNull();
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
  });
  it('opens the exact exchange through the shared controller without a task sheet or action', async () => {
    const showModal = vi.spyOn(HTMLDialogElement.prototype, 'showModal');
    const props = render([request('one'), request('two')]);
    props.controls.inspect = vi.fn(); flushSync();
    await click('2 tasks');
    expect(document.querySelector<HTMLDialogElement>('.work-tray')?.open).toBe(true);
    await click('Task two');
    expect(props.controls.inspect).toHaveBeenCalledExactlyOnceWith('two');
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(showModal).not.toHaveBeenCalled();
    expect(props.controls.action).not.toHaveBeenCalled();
    expect(props.controls.openExecution).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(document.querySelector('.work-pill'));
  });
  it('dismisses the lightweight picker on Escape and outside pointer without dispatch', async () => {
    const props = render([request('one')]); await click('1 task');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await tick(); flushSync();
    expect(document.querySelector('.work-tray')).toBeNull();
    await click('1 task');
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await tick(); flushSync();
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(props.controls.action).not.toHaveBeenCalled();
  });
  it('lists approvals from multiple projects without approving, starting or opening the first agent', async () => {
    const props = render([request('one'), request('two', 'work-b')]);
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(button('2 need your decision').textContent).toContain('2 need your decision');
    await click('2 need your decision');
    expect(document.querySelector('.work-tray')?.textContent).toContain('This chat');
    expect(document.querySelector<HTMLDialogElement>('dialog.work-tray')?.open).toBe(true);
    expect(document.querySelector('.work-tray')?.textContent).toContain('Mac mini / Medousa');
    expect(document.querySelector('.work-tray')?.textContent).toContain('Mac mini / rndr');
    expect(props.controls.action).not.toHaveBeenCalled();
    expect(props.controls.openExecution).not.toHaveBeenCalled();
    expect(props.onOpenWorker).not.toHaveBeenCalled();
  });

  it('opens the selected exact request with explicit approval controls and no automatic action', async () => {
    const show = vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
    const props = render([request('one'), request('two')]);
    await click('2 tasks'); await click('Task two');
    expect(show).toHaveBeenCalledTimes(1); // The picker is nonmodal; only standalone fallback details use a sheet.
    expect(document.querySelector('.work-tray')).toBeNull();
    const dialog = document.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(dialog.textContent).toContain('Task two');
    expect(dialog.textContent).not.toContain('Task one');
    expect(dialog.querySelector('.agent-work-line')).toBeNull();
    expect(dialog.textContent).toContain('It does not approve, apply or publish');
    expect(props.controls.action).not.toHaveBeenCalled();
    await click('Approve & start');
    expect(props.controls.action).toHaveBeenCalledWith(props.proposals[1], 'approve_and_dispatch');
  });

  it('prioritizes an approved failed start above returned results and keeps its exact error in the sheet', async () => {
    vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
    const returned = request('returned');
    returned.receipt = { outcome: 'completed', result: 'No commands were run' } as NonNullable<typeof returned.receipt>;
    const retry = request('retry'); retry.decision = { approved: true } as NonNullable<typeof retry.decision>;
    const props = render([returned, retry]);
    props.controls.actionErrors = { retry: 'Provider unavailable' }; flushSync();
    await click('1 task');
    expect(document.querySelector('.task')?.textContent).toContain('Task retry');
    expect(document.querySelector('.task')?.textContent).toContain('Start not confirmed');
    await click('Task retry');
    expect(document.querySelector('dialog')?.textContent).toContain('Provider unavailable');
    await click('Start approved work');
    expect(props.controls.action).toHaveBeenCalledWith(props.proposals[1], 'dispatch');
  });

  it('shows review as the host responsibility, not a new approval', async () => {
    const row = request('review');
    row.handoff = { admission: 'delegate', state: 'awaiting_sender_review', policy: {}, responsible_session: row.proposal.request.owner_session } as NonNullable<typeof row.handoff>;
    row.receipt = { outcome: 'completed', result: 'Findings returned' } as NonNullable<typeof row.receipt>;
    render([row]); await click('1 task');
    expect(document.querySelector('.decision-link')).toBeNull();
    expect(document.querySelector('.work-tray')?.textContent).toContain('1 review pending');
    expect(document.querySelector('.task')?.textContent).toContain('Next: Medousa');
  });

  it('lets the user choose any worker, including a terminal worker', async () => {
    const workers = ['first', 'second'].map(workId => ({ workId, title: `Worker ${workId}`, terminal: true, streaming: false } as SubagentRow));
    const props = render([], workers); await click('Task history');
    document.querySelector('details')!.open = true;
    await click('Worker second');
    expect(props.onOpenWorker).toHaveBeenCalledWith('second');
    expect(props.controls.action).not.toHaveBeenCalled();
  });

  it('keeps finished work collapsed and dismisses/restores it locally across remounts', async () => {
    const worker = { workId: 'done', title: 'Finished worker', terminal: true, streaming: false } as SubagentRow;
    const props = render([], [worker]); await click('Task history');
    expect(document.querySelector('details')!.open).toBe(false);
    expect(document.querySelector('.work-tray')?.textContent).toContain('No active work');
    document.querySelector('details')!.open = true;
    await click('Dismiss');
    expect(document.querySelector('.finished-row')).toBeNull();
    expect(props.onOpenWorker).not.toHaveBeenCalled();
    expect(props.controls.action).not.toHaveBeenCalled();
    await unmount(component);
    render([], [worker]); await click('Task history');
    document.querySelector('details')!.open = true;
    expect(document.querySelector('.finished-row')).toBeNull();
    await click('Show dismissed'); await click('Restore');
    expect(document.querySelector('.finished-row')?.textContent).toContain('Finished worker');
  });

  it('never hides attention or restarted work, and clears its stale dismissal', async () => {
    const worker = { workId: 'done', title: 'Worker', terminal: true, streaming: false } as SubagentRow;
    const props = render([], [worker]); await click('Task history');
    document.querySelector('details')!.open = true; await click('Dismiss');
    props.workers = [{ ...worker, attention: true }]; flushSync(); await tick(); flushSync();
    expect(document.querySelector('.tasks h3')?.textContent).toContain('Needs attention');
    expect(document.querySelector('.task')?.textContent).toContain('Worker');
    expect(document.querySelector('.history-action')).toBeNull();
    props.workers = [worker]; flushSync(); await tick(); flushSync();
    expect(document.querySelector('.finished-row')).not.toBeNull();
  });

  it('isolates dismissals by chat and keeps pending sender review out of history', async () => {
    const worker = { workId: 'same', title: 'Worker', terminal: true } as SubagentRow;
    const props = render([], [worker]); await click('Task history');
    document.querySelector('details')!.open = true; await click('Dismiss');
    props.sessionId = 'other'; flushSync(); await tick(); flushSync();
    await click('Task history');
    expect(document.querySelector('.finished-row')).not.toBeNull();
    const row = request('review');
    row.handoff = { admission: 'delegate', state: 'awaiting_sender_review', policy: {}, responsible_session: row.proposal.request.owner_session } as NonNullable<typeof row.handoff>;
    row.receipt = { outcome: 'completed' } as NonNullable<typeof row.receipt>;
    props.proposals = [row]; flushSync(); await tick();
    expect(document.querySelector('.tasks h3')?.textContent).toContain('Needs attention');
    expect(document.querySelector('.tasks > .task')?.textContent).toContain('Task review');
  });

  it('preserves the full working status in the accessible trigger name', () => {
    render([], [{ workId: 'active', title: 'Active', streaming: true } as SubagentRow]);
    const trigger = document.querySelector<HTMLButtonElement>('.work-pill')!;
    expect(trigger.textContent?.trim()).toBe('1 task working');
    expect(trigger.getAttribute('aria-label')).toBe('1 task working');
  });

  it('does not count returned workers waiting for synthesis as working', async () => {
    render([], [{ workId: 'result', title: 'Result', terminal: false, streaming: false, statusLine: 'Result ready · Awaiting summary' } as SubagentRow]);
    expect(button('1 task').textContent).not.toContain('working');
    await click('1 task');
    expect(document.querySelector('dialog.work-tray')?.textContent).toContain('0 working');
    document.querySelector<HTMLButtonElement>('[aria-label="Collapse work list"]')!.click(); await tick(); flushSync();
    expect(document.querySelector('.work-tray')).toBeNull();
  });

  it('counts an actual project review and opens it without starting or applying work', async () => {
    const onOpenProjectReview = vi.fn();
    const target = document.createElement('div'); document.body.append(target);
    component = mount(ChatWorkDock, { target, props: { sessionId: 'chat', projectNeedsReview: true, onOpenProjectReview, onOpenWorker: vi.fn() } }); flushSync();
    expect(document.querySelector('.work-pill')).toBeNull();
    await click('1 needs your decision');
    expect(onOpenProjectReview).toHaveBeenCalledExactlyOnceWith();
    expect(document.querySelector('dialog')?.open).toBe(false);
  });
  it('clears the picker and selected task when the session changes', async () => {
    vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
    const props = render([request('old')]); await click('1 task'); await click('Task old');
    props.sessionId = 'other'; props.proposals = []; flushSync(); await tick();
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(document.querySelector('dialog')?.open).toBe(false);
    expect(document.querySelector('dialog')?.textContent).not.toContain('Task old');
    expect(props.controls.action).not.toHaveBeenCalled();
  });
});
