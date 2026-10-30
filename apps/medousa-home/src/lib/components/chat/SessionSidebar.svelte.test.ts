/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
const mocks = vi.hoisted(() => ({
  chat: { sessions: [] as any[], workshopScopeId: 'w', workshopEpoch: 0, sessionId: 'p', focusedSessionId: 'p', pinnedIds: [] as string[], sessionListQuery: '', isPinned: (id: string): boolean => false, togglePin: (id: string) => {}, refreshSessions: vi.fn(), switchSession: vi.fn(), sessionsCursor: null as string | null, sessionsRefreshing: false, sessionsLoadingOlder: false },
  list: vi.fn(),
  open: vi.fn(),
}));
vi.mock('$lib/stores/chat.svelte', () => ({ chat: mocks.chat }));
vi.mock('$lib/daemon/session', async original => ({ ...await original<typeof import('$lib/daemon/session')>(), listSessions: mocks.list }));
vi.mock('$lib/stores/connection.svelte', () => ({ connection: { offline: true, online: false, health: { active_profile_id: 'profile', runtime: { authority_id: 'here', advertised_capabilities: [] } } } }));
vi.mock('$lib/stores/bots.svelte', () => ({ bots: { bots: [], refresh: vi.fn(async () => {}), forSession: () => undefined } }));
vi.mock('$lib/stores/catalog.svelte', () => ({ catalog: { manuscripts: [], refresh: vi.fn() } }));
vi.mock('$lib/stores/sharedMode.svelte', () => ({ sharedMode: { load: vi.fn(), isShared: false } }));
vi.mock('$lib/stores/userProfiles.svelte', () => ({ userProfiles: { activeDisplayName: 'You' } }));
vi.mock('$lib/stores/shellTabs.svelte', () => ({ shellTabs: { openChat: vi.fn() } }));
vi.mock('$lib/stores/connectedAgents.svelte', () => ({ connectedAgents: { workshopScopeId: 'w', conversations: [], refresh: vi.fn() } }));
vi.mock('$lib/runtime/layout.svelte', () => ({ layout: { setSessionDrawerOpen: vi.fn() } }));
import SessionSidebar from './SessionSidebar.svelte';
import { sidebarRelations } from './sidebarRelations.svelte';
import { sidebarKey } from '$lib/utils/sidebarState';
let component: ReturnType<typeof mount> | undefined;
beforeEach(() => {
  localStorage.clear();
  mocks.chat.pinnedIds = [];
  mocks.chat.sessionsCursor = null;
  mocks.chat.togglePin = (id: string) => { mocks.chat.pinnedIds = mocks.chat.pinnedIds.includes(id) ? mocks.chat.pinnedIds.filter(pin => pin !== id) : [...mocks.chat.pinnedIds, id]; };
  mocks.list.mockReset().mockResolvedValue({ sessions: [], next_cursor: null });
  mocks.chat.isPinned = (id: string) => mocks.chat.pinnedIds.includes(id);
  mocks.chat.sessionListQuery = '';
  mocks.chat.focusedSessionId = 'p';
  mocks.chat.sessions = [
    { session_id: 'p', display_name: 'Sidebar polish', preview: '', turns: 2, verification_runs: 0, last_timestamp: new Date().toISOString() },
    { session_id: 'c', display_name: 'Medousa Coder', preview: '', turns: 1, verification_runs: 0, last_timestamp: new Date().toISOString() },
  ];
  sidebarRelations.scope = sidebarKey('w', 'here', 'profile', 'families');
  sidebarRelations.links = [{ child: 'c', parent: 'p', title: 'Composer spacing' }];
  mocks.open.mockClear();
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.useRealTimers(); });
async function render() {
  component = mount(SessionSidebar, { target: document.body, props: { open: true, variant: 'inline', onOpenSession: mocks.open } });
  flushSync(); await tick();
}
it('searches beyond the recent cache, opens the old result, and returns to browsing when cleared', async () => {
  vi.useFakeTimers();
  mocks.list.mockResolvedValue({ sessions: [{ session_id: 'old-lab', display_name: 'labbing', preview: '', turns: 313, verification_runs: 0 }], next_cursor: null });
  await render();
  const search = document.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = 'labbing'; search.dispatchEvent(new Event('input', { bubbles: true }));
  flushSync(); await tick();
  expect(document.body.textContent).toContain('Searching all chats');
  await vi.advanceTimersByTimeAsync(180); flushSync(); await tick();
  expect(mocks.list).toHaveBeenCalledWith({ q: 'labbing', limit: 50, includeVerification: false, cursor: undefined });
  const row = [...document.querySelectorAll<HTMLButtonElement>('.session-row-main')].find(button => button.textContent?.includes('labbing'))!;
  expect(row).toBeTruthy(); row.click(); await tick();
  expect(mocks.open).toHaveBeenCalledWith('old-lab', 'labbing');
  expect(mocks.chat.sessions.some(session => session.session_id === 'old-lab')).toBe(false);
  search.value = ''; search.dispatchEvent(new Event('input', { bubbles: true })); flushSync(); await tick();
  expect(document.body.textContent).toContain('Sidebar polish');
  expect(document.body.textContent).not.toContain('labbing');
});
it('keeps an old search result pinned when the query is cleared', async () => {
  vi.useFakeTimers();
  mocks.list.mockResolvedValue({ sessions: [{ session_id: 'old-lab', display_name: 'labbing', preview: '', turns: 313, verification_runs: 0 }], next_cursor: null });
  await render();
  const search = document.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = 'labbing'; search.dispatchEvent(new Event('input', { bubbles: true })); flushSync(); await tick();
  await vi.advanceTimersByTimeAsync(180); flushSync(); await tick();
  const row = [...document.querySelectorAll('.session-row')].find(row => row.textContent?.includes('labbing'))!;
  row.querySelector<HTMLButtonElement>('[aria-label="Pin session"]')!.click();
  search.value = ''; search.dispatchEvent(new Event('input', { bubbles: true })); flushSync(); await tick();
  const pinned = document.querySelector('[aria-label="Pinned"]')!.closest('li')!;
  expect(pinned.textContent).toContain('labbing');
  expect(mocks.chat.pinnedIds).toContain('old-lab');
});
it('exposes paginated browsing when older pages exist', async () => {
  mocks.chat.sessionsCursor = 'page-two';
  await render();
  expect([...document.querySelectorAll('button')].some(button => button.textContent?.includes('Load older chats'))).toBe(true);
});
it('keeps the Pinned section between connected agents and Today even when empty', async () => {
  await render();
  const titles = [...document.querySelectorAll('.session-sidebar-section-title')].map(title => title.textContent?.trim());
  expect(titles.indexOf('Pinned')).toBe(titles.indexOf('Connected agents') + 1);
  expect(titles.indexOf('Pinned')).toBeLessThan(titles.indexOf('Today'));
  expect(document.querySelector('[aria-label="Pinned"]')?.getAttribute('aria-expanded')).toBe('true');
});
it.each(['p', 'c'])('keeps pinned chat %s in Pinned once, outside recent and collapsed families', async (id) => {
  mocks.chat.pinnedIds = [id];
  await render();
  const section = document.querySelector('[aria-label="Pinned"]')!.closest('li')!;
  const title = id === 'p' ? 'Sidebar polish' : 'Composer spacing';
  expect(section.textContent).toContain(title);
  const rows = [...document.querySelectorAll<HTMLButtonElement>('.session-row-main')].filter(row => row.textContent?.includes(title));
  expect(rows).toHaveLength(1);
  expect(section.contains(rows[0])).toBe(true);
  expect(section.querySelector('[aria-label="Unpin session"]')).not.toBeNull();
  rows[0].click(); await tick();
  expect(mocks.open).toHaveBeenCalledWith(id, title);
});
it('restores the saved Pinned collapse preference', async () => {
  mocks.chat.pinnedIds = ['p'];
  localStorage.setItem(sidebarKey('w', 'here', 'profile', 'pinned'), 'true');
  await render();
  const heading = document.querySelector<HTMLButtonElement>('[aria-label="Pinned"]')!;
  expect(heading.getAttribute('aria-expanded')).toBe('false');
  expect(heading.closest('li')!.querySelector('.session-row-main')).toBeNull();
  heading.click(); flushSync(); await tick();
  expect(heading.closest('li')!.textContent).toContain('Sidebar polish');
});
it('shows children once beneath a collapsible parent and opens the correct child', async () => {
  await render();
  expect(document.querySelectorAll('.session-row-main')).toHaveLength(2); // create bot + parent
  expect(document.body.textContent).not.toContain('Medousa Coder');
  const related = document.querySelector<HTMLButtonElement>('[aria-label="1 related chat for Sidebar polish"]')!;
  expect(related.closest('.session-row')?.querySelector('.session-row-main')?.textContent).toContain('Sidebar polish');
  expect(related.getAttribute('aria-expanded')).toBe('false');
  expect([...document.querySelectorAll('.session-sidebar-section-title')].some(title => title.textContent?.includes('related'))).toBe(false);
  related.click(); flushSync(); await tick();
  expect(mocks.open).not.toHaveBeenCalled();
  expect(related.getAttribute('aria-expanded')).toBe('true');
  expect(localStorage.getItem(sidebarKey('w', 'here', 'profile', 'related:p'))).toBe('false');
  expect(document.querySelectorAll('.session-row-main')).toHaveLength(3);
  const child = [...document.querySelectorAll<HTMLButtonElement>('.session-row-main')].find(button => button.textContent?.includes('Composer spacing'))!;
  child.click(); await tick();
  expect(mocks.open).toHaveBeenCalledWith('c', 'Composer spacing');
});
it('restores the existing locally saved family state', async () => {
  localStorage.setItem(sidebarKey('w', 'here', 'profile', 'related:p'), 'false');
  await render();
  const related = document.querySelector<HTMLButtonElement>('.session-row-disclosure')!;
  expect(related.getAttribute('aria-expanded')).toBe('true');
  expect(document.querySelector('.session-family-children')?.textContent).toContain('Composer spacing');
  related.click(); flushSync(); await tick();
  expect(document.querySelector('.session-family-children')).toBeNull();
  expect(localStorage.getItem(sidebarKey('w', 'here', 'profile', 'related:p'))).toBe('true');
});
it('opens the selected child family by default without sharing state across profiles', async () => {
  localStorage.setItem(sidebarKey('w', 'here', 'another-profile', 'related:p'), 'true');
  mocks.chat.focusedSessionId = 'c';
  await render();
  expect(document.querySelector('.session-row-disclosure')?.getAttribute('aria-expanded')).toBe('true');
  expect(document.querySelector('[aria-current="page"]')?.textContent).toContain('Composer spacing');
});
it('search finds a task-named child directly and retains a parent navigation link', async () => {
  await render();
  const search = document.querySelector<HTMLInputElement>('input[type="search"]')!;
  search.value = 'Composer'; search.dispatchEvent(new Event('input', { bubbles: true }));
  flushSync(); await tick();
  expect(document.body.textContent).toContain('Composer spacing');
  expect(document.body.textContent).toContain('From: Sidebar polish');
  expect(document.querySelector('.session-row-disclosure')).toBeNull();
  const parent = [...document.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.includes('From:'))!;
  parent.click(); await tick();
  expect(mocks.open).toHaveBeenCalledWith('p', undefined);
});
