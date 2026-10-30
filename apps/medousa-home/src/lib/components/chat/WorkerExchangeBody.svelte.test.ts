/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import WorkerExchangeBody from './WorkerExchangeBody.svelte';
import type { WorkerTranscript } from '$lib/work/workerTranscripts.svelte';
const worker = vi.hoisted(() => ({ transcriptFor: vi.fn(), refresh: vi.fn() }));
vi.mock('$lib/work/workerTranscripts.svelte', () => ({ workerTranscripts: worker }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.resetAllMocks(); });
const task = (output: string, terminal = false): WorkerTranscript => ({ workId: 'worker', title: 'Check the UI', disposition: 'parallel', thinking: '', output, toolRuns: [], terminal, streaming: !terminal });
async function settle() { await tick(); flushSync(); await Promise.resolve(); await tick(); flushSync(); }
async function render() { const target = document.createElement('div'); document.body.append(target); component = mount(WorkerExchangeBody, { target, props: { workId: 'worker' } }); flushSync(); await settle(); }
describe('worker exchange body', () => {
  it('renders live output without refetching on every streamed update', async () => {
    const view = $state<{ task: WorkerTranscript | null }>({ task: task('First update') });
    worker.transcriptFor.mockImplementation(() => view.task);
    worker.refresh.mockImplementation(async () => view.task);
    await render(); expect(document.body.textContent).toContain('First update');
    view.task = task('Second update'); await settle();
    expect(document.body.textContent).toContain('Second update');
    expect(worker.refresh).toHaveBeenCalledExactlyOnceWith('worker');
    expect(document.body.textContent).toContain('does not expose a separate message history');
    expect(document.querySelector('textarea, form')).toBeNull();
  });
  it('prefers the recorded result and keeps technical metadata secondary', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Older stream', true), resultText: 'Final result', model: 'test-model' });
    worker.refresh.mockResolvedValue(task('Final result', true));
    await render(); expect(document.body.textContent).toContain('Final result'); expect(document.body.textContent).not.toContain('Older stream');
    expect(document.querySelector<HTMLDetailsElement>('details')?.open).toBe(false);
    expect(document.body.textContent).toContain('does not mean changes were approved');
  });
  it('shows an honest failure when the worker cannot be loaded', async () => {
    worker.transcriptFor.mockReturnValue(null); worker.refresh.mockResolvedValue(null);
    await render(); expect(document.querySelector('[role="status"]')?.textContent).toContain('could not be loaded');
  });
});
