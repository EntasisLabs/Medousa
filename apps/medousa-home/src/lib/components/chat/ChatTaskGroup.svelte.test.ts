/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ChatTaskGroup from './ChatTaskGroup.svelte';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';
import type { PeerProposalControls } from './peerProposalControls';
import type { SubagentRow } from '$lib/utils/subagentRows';

const components: ReturnType<typeof mount>[] = [];
afterEach(async () => { for (const component of components.splice(0)) await unmount(component); document.body.replaceChildren(); vi.restoreAllMocks(); });
const request = (id: string) => ({ proposal: { proposal_id: id, expires_at: '2099-01-01T00:00:00Z', request: { instructions: `Task ${id}`, context: { sources: [] }, target: { runtime: 'medousa' } } } } as unknown as PeerProposalReviewRecord);
function render(rows = [request('first'), request('second')], workers: SubagentRow[] = []) {
  const controls: PeerProposalControls = { now: Date.now(), available: true, busy: false, busyId: null, feedback: null, hasMore: false, loadMore: vi.fn(), action: vi.fn(), openExecution: vi.fn() };
  const props = $state({ group: { proposals: rows, workers }, controls, onOpenSubagent: vi.fn(), onStopSubagent: vi.fn() });
  const target = document.createElement('div'); document.body.append(target);
  components.push(mount(ChatTaskGroup, { target, props })); flushSync();
  return props;
}
async function click(text: string) { [...document.querySelectorAll('button')].find(button => button.textContent?.includes(text))!.click(); await tick(); flushSync(); }

describe('turn task receipts', () => {
  it('uses the parent exchange route for the exact inline handoff without opening a modal', async () => {
    const modal = vi.spyOn(HTMLDialogElement.prototype, 'showModal');
    const props = render(); props.controls.inspect = vi.fn(); flushSync();
    await click('Task second');
    expect(props.controls.inspect).toHaveBeenCalledExactlyOnceWith('second');
    expect(modal).not.toHaveBeenCalled();
    expect(props.controls.action).not.toHaveBeenCalled();
    expect(props.controls.openExecution).not.toHaveBeenCalled();
  });
  it('shows compact decisions without legacy agent blocks or raw metadata', async () => {
    const props = render();
    expect(document.querySelectorAll('.task-receipt')).toHaveLength(2);
    expect(document.querySelector('.agent-group, .agent-work-line')).toBeNull();
    expect(document.body.textContent).not.toContain('Execution workshop');
    expect(document.body.textContent).not.toContain('Shared conversation ranges');
    expect(props.controls.action).not.toHaveBeenCalled();
  });
  it('opens only the selected decision and keeps approval explicit', async () => {
    vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function(this: HTMLDialogElement) { this.open = true; });
    const props = render(); await click('Task second');
    const dialog = document.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(dialog.textContent).toContain('Task second'); expect(dialog.textContent).not.toContain('Task first');
    expect(props.controls.action).not.toHaveBeenCalled();
    await click('Approve & start');
    expect(props.controls.action).toHaveBeenCalledWith(props.group.proposals[1], 'approve_and_dispatch');
  });
  it('does not turn a returned result into a new user approval', async () => {
    const row = request('returned'); row.receipt = { result: 'Findings', outcome: 'completed' } as NonNullable<typeof row.receipt>;
    render([row]); expect(document.querySelector('.decision')).toBeNull();
    expect(document.body.textContent).not.toContain('Decision needed');
  });
  it('collapses verbose worker updates behind one turn summary by default', () => {
    render([], Array.from({ length: 4 }, (_, i) => ({ workId: `worker-${i}`, title: 'Long progress update', streaming: false, terminal: false, statusLine: 'Result ready · Awaiting summary' } as SubagentRow)));
    const details = document.querySelector<HTMLDetailsElement>('details.worker-summary')!;
    expect(details.open).toBe(false);
    expect(details.querySelector('summary')?.textContent).toContain('4 agent tasks');
    expect(document.querySelector('.stop')).toBeNull();
    details.open = true;
    expect(details.querySelectorAll('.worker-receipt')).toHaveLength(4);
  });
  it('opens a worker result without showing its raw transcript and allows an explicit stop', async () => {
    const props = render([], [{ workId: 'worker', title: 'Review fixes', streaming: true, statusLine: 'Checking tests' } as SubagentRow]);
    await click('Review fixes'); expect(props.onOpenSubagent).toHaveBeenCalledWith('worker');
    await click('Stop'); expect(props.onStopSubagent).toHaveBeenCalledWith('worker');
    expect(document.querySelector('.agent-group')).toBeNull();
  });
});
