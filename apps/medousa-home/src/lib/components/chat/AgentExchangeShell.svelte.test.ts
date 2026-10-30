/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createRawSnippet, flushSync, mount, unmount } from 'svelte';
import AgentExchangeShell from './AgentExchangeShell.svelte';
const back = vi.hoisted(() => ({ handler: null as null | (() => boolean), dispose: vi.fn() }));
vi.mock('$lib/mobileNavigation', () => ({ registerMobileBackHandler: (handler: () => boolean) => { back.handler = handler; return back.dispose; } }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.clearAllMocks(); });
function render() {
  const origin = document.createElement('button'); origin.textContent = 'Open work'; document.body.append(origin); origin.focus();
  const target = document.createElement('div'); document.body.append(target);
  const onClose = vi.fn();
  component = mount(AgentExchangeShell, { target, props: { title: 'Agent exchange', participant: 'Coder', returnFocus: origin, onClose,
    children: createRawSnippet(() => ({ render: () => '<p>Recorded exchange</p>' })) } }); flushSync();
  return { origin, onClose, root: document.querySelector<HTMLElement>('dialog.agent-exchange')! };
}
describe('agent exchange shell', () => {
  it('focuses the reading surface, identifies participants, and provides no accidental composer', () => {
    const { root } = render();
    expect(document.activeElement).toBe(root);
    expect(root.textContent).toContain('Medousa'); expect(root.textContent).toContain('Coder');
    expect(root.querySelector('textarea, input, form')).toBeNull();
    expect(root.textContent).toContain('Decisions require explicit action');
  });
  it('closes on Escape, explicit return and mobile Back', () => {
    const { root, onClose } = render();
    root.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    expect(onClose).toHaveBeenCalledTimes(1);
    root.querySelector<HTMLButtonElement>('footer button')!.click();
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(back.handler?.()).toBe(true); expect(onClose).toHaveBeenCalledTimes(3);
  });
  it('restores the exact connected origin and unregisters Back on teardown', async () => {
    const { origin } = render();
    await unmount(component!); component = undefined;
    expect(document.activeElement).toBe(origin); expect(back.dispose).toHaveBeenCalledOnce();
  });
});
