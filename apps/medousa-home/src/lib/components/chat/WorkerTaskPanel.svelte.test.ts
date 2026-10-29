/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import WorkerTaskPanel from './WorkerTaskPanel.svelte';
import { workerTranscripts, type WorkerTranscript } from '$lib/work/workerTranscripts.svelte';
vi.mock('$lib/mobileNavigation', () => ({ registerMobileBackHandler: () => () => {} }));
vi.mock('$lib/daemon', () => ({ getWorkspaceCard: vi.fn() }));
let component: ReturnType<typeof mount>;
const task = (workId: string): WorkerTranscript => ({ workId, title: `Task ${workId}`, disposition: 'parallel', toolRuns: [], thinking: 'PRIVATE RAW TRACE', output: '', resultText: `Result ${workId}`, streaming: false, terminal: true });
beforeEach(() => {
  workerTranscripts.transcripts = new Map([['one', task('one')], ['two', task('two')]]);
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function(this: HTMLDialogElement) { this.open = true; });
});
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); workerTranscripts.transcripts = new Map(); vi.restoreAllMocks(); });
async function settle() { await tick(); flushSync(); await tick(); flushSync(); }
function render(workId: string | null = 'one') {
  const props = $state({ workId, onClose: vi.fn() });
  const target = document.createElement('div'); document.body.append(target);
  component = mount(WorkerTaskPanel, { target, props }); flushSync(); return props;
}
describe('focused worker task panel', () => {
  it('shows the result without old reasoning, raw facts or tool blocks', async () => {
    render(); await settle();
    expect(document.querySelector('dialog')?.open).toBe(true);
    expect(document.body.textContent).toContain('Result one');
    expect(document.body.textContent).not.toContain('PRIVATE RAW TRACE');
    expect(document.body.textContent).not.toContain('Run details');
    expect(document.querySelector('.worker-transcript-sheet')).toBeNull();
    expect(document.body.textContent).toContain('does not mean changes were approved');
  });
  it('switches exact result and removes the dialog on close', async () => {
    const props = render(); await settle(); props.workId = 'two'; await settle();
    expect(document.body.textContent).toContain('Result two'); expect(document.body.textContent).not.toContain('Result one');
    props.workId = null; await settle(); expect(document.querySelector('dialog')).toBeNull();
  });
  it('shows the returned-but-unsummarized lifecycle without claiming execution', async () => {
    workerTranscripts.transcripts = new Map([['one', { ...task('one'), terminal: false, streaming: false, statusLine: 'Result ready · Awaiting summary' }]]);
    vi.spyOn(workerTranscripts, 'refresh').mockResolvedValue(workerTranscripts.transcriptFor('one'));
    render(); await settle();
    expect(document.querySelector('.status')?.textContent).toBe('Result ready · Awaiting summary');
    expect(document.body.textContent).not.toContain('Status unavailable');
  });
  it('uses an honest unavailable state when cold loading fails', async () => {
    vi.spyOn(workerTranscripts, 'refresh').mockResolvedValue(null);
    render('missing'); await settle();
    expect(document.body.textContent).toContain('could not be loaded');
    expect(document.body.textContent).not.toContain('Session ended');
  });
});
