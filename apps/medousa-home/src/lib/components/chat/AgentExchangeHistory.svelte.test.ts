/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import AgentExchangeHistory from './AgentExchangeHistory.svelte';
const api = vi.hoisted(() => ({ getSessionHistory: vi.fn() }));
vi.mock('$lib/daemon', () => api);
vi.mock('$lib/haptics', () => ({ haptic: vi.fn() }));
vi.mock('$lib/mobileNavigation', () => ({ registerMobileBackHandler: () => () => {} }));
vi.mock('$lib/chat/sessionController', () => ({ TRANSCRIPT_PAGE_SIZE: 50, mapTurns: (turns: { entry_id: string; role: string; content: string }[]) => turns.map(turn => ({ ...turn, id: turn.entry_id })) }));
const page = (text = 'Latest reply', next: string | null = 'older') => ({ session_id: 'agent', authority_id: 'local', turns: [{ entry_id: text, role: 'assistant', content: text }], next_cursor: next });
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.resetAllMocks(); vi.useRealTimers(); });
async function settle() { await Promise.resolve(); await tick(); flushSync(); await Promise.resolve(); await tick(); flushSync(); }
async function render() {
  const target = document.createElement('div'); document.body.append(target);
  const props = $state({ sessionId: 'agent', authorityId: 'local', participant: 'Coder', fallbackResult: '' });
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
  it('retains older pages and refreshes new messages without discarding the loaded conversation', async () => {
    api.getSessionHistory.mockResolvedValueOnce(page()).mockResolvedValueOnce(page('Older reply', null)).mockResolvedValueOnce(page('Updated reply'));
    await render(); await click('Earlier messages');
    expect(api.getSessionHistory).toHaveBeenLastCalledWith('agent', { limit: 50, cursor: 'older' });
    expect(document.body.textContent).toContain('Older reply'); expect(document.body.textContent).toContain('Latest reply');
    expect(document.body.textContent!.indexOf('Older reply')).toBeLessThan(document.body.textContent!.indexOf('Latest reply'));
    await click('Refresh exchange');
    expect(document.body.textContent).toContain('Older reply'); expect(document.body.textContent).toContain('Updated reply');
    expect(document.body.textContent).not.toContain('Earlier messages');
  });
  it('offers an unseen intervening page after refreshing a long-inactive fully loaded thread', async () => {
    api.getSessionHistory.mockResolvedValueOnce(page()).mockResolvedValueOnce(page('Oldest', null))
      .mockResolvedValueOnce(page('Much newer', 'gap')).mockResolvedValueOnce(page('Intervening messages', 'older'));
    await render(); await click('Earlier messages');
    await click('Refresh exchange');
    await click('Earlier messages');
    expect(api.getSessionHistory).toHaveBeenLastCalledWith('agent', { limit: 50, cursor: 'gap' });
    expect(document.body.textContent).toContain('Oldest');
    expect(document.body.textContent).toContain('Intervening messages');
    expect(document.body.textContent).toContain('Much newer');
  });
  it('refreshes even with older pages loaded and replaces overlapping entries instead of duplicating them', async () => {
    vi.useFakeTimers();
    api.getSessionHistory.mockResolvedValueOnce(page()).mockResolvedValueOnce(page('Older reply', null)).mockResolvedValue({ ...page(), turns: [{ entry_id: 'Latest reply', role: 'assistant', content: 'Updated live reply' }] });
    await render(); await click('Earlier messages');
    await vi.advanceTimersByTimeAsync(5000); await settle();
    expect(document.body.textContent).toContain('Older reply'); expect(document.body.textContent).toContain('Updated live reply');
    expect(document.body.textContent).not.toContain('Latest reply');
    expect(api.getSessionHistory).toHaveBeenCalledTimes(3);
  });
  it('keeps the returned result fallback until the canonical assistant message contains it', async () => {
    api.getSessionHistory.mockResolvedValueOnce(page('Different response')).mockResolvedValueOnce(page('Returned evidence'));
    const props = await render(); props.fallbackResult = 'Returned evidence'; flushSync(); await settle();
    expect(document.body.textContent).toContain('Returned result');
    await click('Refresh exchange');
    expect(document.body.textContent).not.toContain('Returned result');
    expect(document.body.textContent!.split('Returned evidence')).toHaveLength(2);
  });
  it('preserves fallback on missing history and errors', async () => {
    api.getSessionHistory.mockRejectedValue(new Error('Offline'));
    const props = await render(); props.fallbackResult = 'Retained receipt'; flushSync(); await settle();
    expect(document.body.textContent).toContain('Retained receipt');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Offline');
  });
  it('renders tool-only entries and ordered text segments with expandable evidence', async () => {
    const run = { runId: 'read', toolName: 'code.read', round: 1, status: 'succeeded', inputSummary: 'Read source', outputSummary: 'Recorded evidence' };
    api.getSessionHistory.mockResolvedValue({ ...page('', null), turns: [{ entry_id: 'tools', role: 'assistant', content: '', segments: [{ kind: 'text', segmentId: 'before', markdown: 'Before tools' }, { kind: 'tool_group', groupId: 'group', toolRound: 1, runs: [run] }, { kind: 'text', segmentId: 'after', markdown: 'After tools' }] }] });
    await render();
    expect(document.body.textContent).toContain('Before tools'); expect(document.body.textContent).toContain('After tools');
    expect(document.querySelector('details, button[aria-expanded]')).not.toBeNull();
    const trigger = document.querySelector<HTMLButtonElement>('.tool-context-trigger')!;
    expect(trigger.textContent).toContain('1 tool');
    trigger.click(); await settle();
    const details = document.querySelector<HTMLDetailsElement>('details')!;
    details.open = true;
    expect(details.textContent).toContain('Recorded evidence');
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    expect(document.querySelector('textarea, input, form')).toBeNull();
  });
  it('groups adjacent rounds but preserves text boundaries and every recorded run', async () => {
    const run = (id: string) => ({ runId: id, toolName: 'code.read', round: 1, status: 'succeeded', outputSummary: `Evidence ${id}` });
    const groups = ['one', 'two', 'three', 'four'].map(id => ({ kind: 'tool_group', groupId: id, toolRound: 1, runs: [run(id)] }));
    api.getSessionHistory.mockResolvedValue({ ...page('', null), turns: [{ entry_id: 'grouped', role: 'assistant', content: '', segments: [...groups, { kind: 'text', segmentId: 'boundary', markdown: 'Between tools' }, { ...groups[0], groupId: 'last', runs: [run('last')] }] }] });
    await render();
    const triggers = document.querySelectorAll<HTMLButtonElement>('.tool-context-trigger');
    expect(triggers).toHaveLength(2);
    expect(triggers[0].textContent).toContain('4 tools');
    expect(document.body.textContent).not.toContain('Evidence two');
    triggers[0].click(); await settle();
    expect(document.querySelectorAll('[aria-label="Tools in execution order"] > li')).toHaveLength(4);
    expect(document.body.textContent).toContain('Evidence two');
    expect(document.body.textContent).toContain('Evidence three');
    const text = document.body.textContent!;
    expect(text.indexOf('Evidence four')).toBeLessThan(text.indexOf('Between tools'));
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });
  it('deduplicates final content already present in segments and matches a receipt across text segments', async () => {
    api.getSessionHistory.mockResolvedValue({ ...page('', null), turns: [{ entry_id: 'segmented', role: 'assistant', content: 'First part\n\nSecond part', segments: [{ kind: 'text', segmentId: 'one', markdown: 'First part' }, { kind: 'text', segmentId: 'two', markdown: 'Second part' }] }] });
    const props = await render(); props.fallbackResult = 'First part\n\nSecond part'; flushSync(); await settle();
    expect(document.body.textContent!.split('First part')).toHaveLength(2);
    expect(document.body.textContent!.split('Second part')).toHaveLength(2);
    expect(document.body.textContent).not.toContain('Returned result');
  });
  it('keeps legacy tool-only messages readable without text or segments', async () => {
    api.getSessionHistory.mockResolvedValue({ ...page('', null), turns: [{ entry_id: 'tools-only', role: 'assistant', content: '', toolRuns: [{ runId: 'legacy', toolName: 'code.read', round: 1, status: 'succeeded', outputSummary: 'Legacy evidence' }] }] });
    await render();
    document.querySelector<HTMLButtonElement>('.tool-context-trigger')!.click(); await settle();
    const details = document.querySelector<HTMLDetailsElement>('details')!;
    details.open = true;
    expect(details.textContent).toContain('Legacy evidence');
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
