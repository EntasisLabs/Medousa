/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from 'vitest';
import { createRawSnippet, flushSync, mount, tick, unmount } from 'svelte';
import SidebarSectionHeading from './SidebarSectionHeading.svelte';
import { sidebarKey } from '$lib/utils/sidebarState';
vi.mock('$lib/stores/chat.svelte', () => ({ chat: { workshopScopeId: 'workshop' } }));
vi.mock('$lib/stores/connection.svelte', () => ({ connection: { health: { runtime: { authority_id: 'authority' }, active_profile_id: 'profile' } } }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); localStorage.clear(); });
const content = createRawSnippet(() => ({ render: () => '<div data-content>Conversations</div>' }));
const actions = createRawSnippet(() => ({ render: () => '<button data-action>New</button>' }));
async function render(extra = {}) {
  component = mount(SidebarSectionHeading, { target: document.body, props: { id: 'today', label: 'Today', count: 3, content, children: actions, ...extra } });
  flushSync(); await tick();
}
it('collapses content, retains count/actions, and restores a saved preference', async () => {
  await render();
  const toggle = document.querySelector<HTMLButtonElement>('[aria-label="Today"]')!;
  expect(toggle.getAttribute('aria-expanded')).toBe('true');
  toggle.click(); flushSync(); await tick();
  expect(toggle.getAttribute('aria-expanded')).toBe('false');
  expect(document.querySelector('[data-content]')).toBeNull();
  expect(document.querySelector('[data-action]')).not.toBeNull();
  expect(toggle.textContent).toContain('3');
  expect(localStorage.getItem(sidebarKey('workshop', 'authority', 'profile', 'today'))).toBe('true');
  await unmount(component!); document.body.replaceChildren(); await render();
  expect(document.querySelector('[aria-label="Today"]')?.getAttribute('aria-expanded')).toBe('false');
});
it('search reveals results without overwriting collapse preference', async () => {
  const key = sidebarKey('workshop', 'authority', 'profile', 'today');
  localStorage.setItem(key, 'true');
  await render({ searching: true });
  expect(document.querySelector('[data-content]')).not.toBeNull();
  expect(localStorage.getItem(key)).toBe('true');
});
it('other profiles do not inherit collapsed preferences', async () => {
  localStorage.setItem(sidebarKey('workshop', 'authority', 'other', 'today'), 'true');
  await render();
  expect(document.querySelector('[data-content]')).not.toBeNull();
});
