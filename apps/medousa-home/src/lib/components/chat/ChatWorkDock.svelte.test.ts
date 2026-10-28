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
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); vi.restoreAllMocks(); });
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
  it('lists approvals from multiple projects without approving, starting or opening the first agent', async () => {
    const props = render([request('one'), request('two', 'work-b')]);
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(button('2 agents').textContent).toContain('2 needs you');
    await click('2 agents');
    expect(document.querySelector('.work-tray')?.textContent).toContain('This chat');
    expect(document.querySelector('.work-tray')?.textContent).toContain('Mac mini / Medousa');
    expect(document.querySelector('.work-tray')?.textContent).toContain('Mac mini / rndr');
    expect(props.controls.action).not.toHaveBeenCalled();
    expect(props.controls.openExecution).not.toHaveBeenCalled();
    expect(props.onOpenWorker).not.toHaveBeenCalled();
  });

  it('opens the selected exact request with explicit approval controls and no automatic action', async () => {
    const show = vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
    const props = render([request('one'), request('two')]);
    await click('2 agents'); await click('Task two');
    expect(show).toHaveBeenCalledOnce();
    const dialog = document.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(dialog.textContent).toContain('Task two');
    expect(dialog.textContent).not.toContain('Task one');
    expect((dialog.querySelector('.agent-work-line') as HTMLDetailsElement).open).toBe(true);
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
    await click('2 agents');
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
    render([row]); await click('1 agent');
    expect(button('1 agent').textContent).not.toContain('needs you');
    expect(document.querySelector('.work-tray')?.textContent).toContain('1 review pending');
    expect(document.querySelector('.task')?.textContent).toContain('Next: Medousa');
  });

  it('lets the user choose any worker, including a terminal worker', async () => {
    const workers = ['first', 'second'].map(workId => ({ workId, title: `Worker ${workId}`, terminal: true, streaming: false } as SubagentRow));
    const props = render([], workers); await click('2 agents'); await click('Worker second');
    expect(props.onOpenWorker).toHaveBeenCalledWith('second');
    expect(props.controls.action).not.toHaveBeenCalled();
  });

  it('clears the picker and selected task when the session changes', async () => {
    vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
    const props = render([request('old')]); await click('1 agent'); await click('Task old');
    props.sessionId = 'other'; props.proposals = []; flushSync(); await tick();
    expect(document.querySelector('.work-tray')).toBeNull();
    expect(document.querySelector('dialog')?.open).toBe(false);
    expect(document.querySelector('dialog')?.textContent).not.toContain('Task old');
    expect(props.controls.action).not.toHaveBeenCalled();
  });
});
