/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import WorkerExchangeBody from './WorkerExchangeBody.svelte';
import type { WorkerTranscript } from '$lib/work/workerTranscripts.svelte';
const worker = vi.hoisted(() => ({ transcriptFor: vi.fn(), refresh: vi.fn() }));
vi.mock('$lib/work/workerTranscripts.svelte', () => ({ workerTranscripts: worker }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.resetAllMocks(); });
const task = (output: string, terminal = false): WorkerTranscript => ({ workId: 'worker', title: 'Check the UI', disposition: 'parallel', thinking: '', output, toolRuns: [], historyLoaded: true, taskPrompt: 'Full assignment instructions', steerMessages: [{ text: 'Check follow-up too', at: '2026-10-30T14:00:00Z' }], terminal, streaming: !terminal });
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
    expect(document.body.textContent).toContain('Full assignment instructions');
    expect(document.body.textContent).toContain('Check follow-up too');
    expect(document.querySelector('textarea, form')).toBeNull();
  });
  it('prefers the recorded result and keeps technical metadata secondary', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Older stream', true), resultText: 'Final result', model: 'test-model' });
    worker.refresh.mockResolvedValue(task('Final result', true));
    await render(); expect(document.body.textContent).toContain('Final result'); expect(document.body.textContent).not.toContain('Older stream');
    expect(document.querySelector<HTMLDetailsElement>('details')?.open).toBe(false);
    expect(document.body.textContent).toContain('does not mean changes were approved');
  });
  it('does not let a short card preview eclipse a longer live result', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Complete long output'.repeat(100), true), historyLoaded: false, resultText: 'Short preview' });
    worker.refresh.mockResolvedValue(null);
    await render();
    expect(document.body.textContent).toContain('Complete long output'.repeat(100));
    expect(document.body.textContent).not.toContain('Short preview');
    expect(document.body.textContent).toContain('Only the card preview');
  });
  it('shows recorded tools as an expandable transcript instead of only a count', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Final result', true), toolRuns: [{ run_id: 'one', name: 'read_file', status: 'succeeded', round: 1, output_summary: 'Recorded evidence' }] });
    worker.refresh.mockResolvedValue(null);
    await render();
    expect(document.querySelector('[aria-label="Recorded worker tools"]')).not.toBeNull();
    const trigger = document.querySelector<HTMLButtonElement>('[aria-label="Recorded worker tools"] .tool-context-trigger')!;
    expect(trigger.textContent).toContain('1 tool');
    expect(document.querySelector('[aria-label="Recorded worker tools"] details')).toBeNull();
    trigger.click(); await settle();
    const details = document.querySelector<HTMLDetailsElement>('[aria-label="Recorded worker tools"] details')!;
    expect(details).not.toBeNull();
    details.open = true;
    expect(details.textContent).toContain('Recorded evidence');
    expect(document.querySelector('[aria-label="Tool activity"]')).toBeNull();
  });
  it('places recorded follow-ups between their surrounding tool calls', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Finished', true), toolRuns: [
      { run_id: 'early', name: 'read_file', status: 'succeeded', round: 1, started_at: '2026-10-30T13:59:00Z', output_summary: 'Earlier tool' },
      { run_id: 'late', name: 'read_file', status: 'succeeded', round: 2, started_at: '2026-10-30T14:01:00Z', output_summary: 'Later tool' },
    ] });
    worker.refresh.mockResolvedValue(null);
    await render();
    expect(document.querySelectorAll('.tool-context-trigger')).toHaveLength(2);
    for (const trigger of document.querySelectorAll<HTMLButtonElement>('.tool-context-trigger')) trigger.click();
    await settle();
    const text = document.body.textContent!;
    expect(text.indexOf('Earlier tool')).toBeLessThan(text.indexOf('Check follow-up too'));
    expect(text.indexOf('Check follow-up too')).toBeLessThan(text.indexOf('Later tool'));
  });
  it('collapses a long sequence into one row and retains every tool on demand', async () => {
    worker.transcriptFor.mockReturnValue({ ...task('Finished', true), steerMessages: [], toolRuns: Array.from({ length: 8 }, (_, index) => ({ run_id: `run-${index}`, name: 'coder_shell_run', status: index === 3 ? 'failed' : 'succeeded', round: index + 1, output_summary: `Evidence ${index}` })) });
    worker.refresh.mockResolvedValue(null);
    await render();
    const trigger = document.querySelector<HTMLButtonElement>('.tool-context-trigger')!;
    expect(document.querySelectorAll('.tool-context-trigger')).toHaveLength(1);
    expect(trigger.textContent).toContain('8 tools');
    expect(trigger.textContent).toContain('1 failed');
    expect(document.body.textContent).not.toContain('Evidence 4');
    trigger.click(); await settle();
    expect(document.querySelectorAll('[aria-label="Tools in execution order"] > li')).toHaveLength(8);
    for (let index = 0; index < 8; index++) expect(document.body.textContent).toContain(`Evidence ${index}`);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
  it('shows an honest failure when the worker cannot be loaded', async () => {
    worker.transcriptFor.mockReturnValue(null); worker.refresh.mockResolvedValue(null);
    await render(); expect(document.querySelector('[role="status"]')?.textContent).toContain('could not be loaded');
  });
});
