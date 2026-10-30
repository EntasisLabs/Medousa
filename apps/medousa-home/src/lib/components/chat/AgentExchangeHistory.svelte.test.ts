/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import AgentExchangeHistory from './AgentExchangeHistory.svelte';
const api = vi.hoisted(() => ({ getSessionHistory: vi.fn() }));
vi.mock('$lib/daemon', () => api);
vi.mock('$lib/chat/sessionController', () => ({ TRANSCRIPT_PAGE_SIZE: 50, mapTurns: (turns: { entry_id: string; role: string; content: string }[]) => turns.map(turn => ({ id: turn.entry_id, role: turn.role, content: turn.content })) }));
const page = (text = 'Latest reply', next: string | null = 'older') => ({ session_id: 'agent', authority_id: 'local', turns: [{ entry_id: text, role: 'assistant', content: text }], next_cursor: next });
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.resetAllMocks(); vi.useRealTimers(); });
async function settle() { await Promise.resolve(); await tick(); flushSync(); await Promise.resolve(); await tick(); flushSync(); }
async function render() {
  const target = document.createElement('div'); document.body.append(target);
  const props = $state({ sessionId: 'agent', authorityId: 'local', participant: 'Coder' });
  component = mount(AgentExchangeHistory, { target, props }); flushSync(); await settle();
  return props;
}
async function click(text: string) { const button = [...document.querySelectorAll('button')].find(button => button.textContent?.includes(text)); expect(button).toBeTruthy(); button!.click(); await settle(); }
describe('read-only agent exchange history', () => {
  it('loads bounded canonical history without a composer and does not spin a reactive request loop', async () => {
    api.getSessionHistory.mockResolvedValue(page()); await render();
    expect(api.getSessionHistory).toHaveBeenCalledExactlyOnceWith('agent', { limit: 50 });
    expect(document.body.textContent).toContain('Latest reply');
    expect(document.querySelector('textarea, input, form')).toBeNull();
  });
  it('pages backward and forward without changing the parent session, and can refresh the latest page', async () => {
    api.getSessionHistory.mockResolvedValueOnce(page()).mockResolvedValueOnce(page('Older reply', null)).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('Updated reply'));
    await render(); await click('Earlier messages');
    expect(api.getSessionHistory).toHaveBeenLastCalledWith('agent', { limit: 50, cursor: 'older' });
    expect(document.body.textContent).toContain('Older reply'); expect(document.body.textContent).not.toContain('Latest reply');
    await click('Later messages'); expect(document.body.textContent).toContain('Latest reply');
    await click('Refresh exchange'); expect(document.body.textContent).toContain('Updated reply');
  });
  it('rejects a response from another authority and lets the user retry', async () => {
    api.getSessionHistory.mockResolvedValueOnce({ ...page('Foreign text'), authority_id: 'foreign' }).mockResolvedValueOnce(page('Recovered'));
    await render(); expect(document.body.textContent).not.toContain('Foreign text');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('authority changed');
    await click('Refresh exchange'); expect(document.body.textContent).toContain('Recovered'); expect(document.querySelector('[role="alert"]')).toBeNull();
  });
  it('ignores late results from a previously viewed session', async () => {
    let resolve!: (value: ReturnType<typeof page>) => void;
    api.getSessionHistory.mockReturnValueOnce(new Promise(done => resolve = done)).mockResolvedValueOnce({ ...page('New session'), session_id: 'new-agent' });
    const props = await render(); props.sessionId = 'new-agent'; flushSync(); await settle();
    expect(document.body.textContent).toContain('New session');
    resolve(page('Stale reply')); await settle();
    expect(document.body.textContent).toContain('New session'); expect(document.body.textContent).not.toContain('Stale reply');
  });
  it('shows load errors and refreshes only while mounted', async () => {
    vi.useFakeTimers(); api.getSessionHistory.mockRejectedValueOnce(new Error('Connection unavailable')).mockResolvedValue(page('Recovered'));
    await render(); expect(document.querySelector('[role="alert"]')?.textContent).toContain('Connection unavailable');
    await click('Refresh exchange'); expect(document.body.textContent).toContain('Recovered');
    await unmount(component!); component = undefined;
    await vi.advanceTimersByTimeAsync(10000); expect(api.getSessionHistory).toHaveBeenCalledTimes(2);
  });
});
