/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ChatStickyBoard from './ChatStickyBoard.svelte';
import { readChatStickies, mutateChatStickies, type StickyBoard } from '$lib/daemon/chatStickies';
vi.mock('$lib/daemon/chatStickies', () => ({ readChatStickies: vi.fn(), mutateChatStickies: vi.fn() }));
const note = { id: 'correction', text: 'Preserve this exact exception', kind: 'constraint', pinned: true, exact: true, status: 'active' as const, source_refs: ['turn:4'] };
const board: StickyBoard = { revision: 3, notes: [note] };
let component: ReturnType<typeof mount>;
async function settle() { await Promise.resolve(); await tick(); flushSync(); await Promise.resolve(); await tick(); flushSync(); }
function render(sessionId = 'chat-a') {
  const props = $state({ sessionId, workshopScopeId: 'workshop-a', profileId: 'profile-a', enabled: true });
  const target = document.createElement('div'); document.body.append(target);
  component = mount(ChatStickyBoard, { target, props }); flushSync(); return props;
}
function button(text: string) {
  const found = [...document.querySelectorAll('button')].find(item => item.textContent?.trim() === text || item.textContent?.startsWith(text));
  if (!found) throw new Error(`Missing button ${text}`); return found;
}
async function click(text: string) { button(text).click(); await settle(); }
beforeEach(() => {
  vi.mocked(readChatStickies).mockReset().mockResolvedValue(structuredClone(board));
  vi.mocked(mutateChatStickies).mockReset().mockResolvedValue({ revision: 4, notes: [] });
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.open = false; this.dispatchEvent(new Event('close')); });
});
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); vi.restoreAllMocks(); });

describe('chat sticky inspector', () => {
  it('inspects scoped notes without mutating and protects exact/source text in presentation', async () => {
    render(); await settle(); await click('Sticky notes');
    expect(readChatStickies).toHaveBeenCalledWith('chat-a', 'workshop-a');
    expect(document.querySelector('dialog')?.open).toBe(true);
    expect(document.querySelector('dialog')?.textContent).toContain(note.text);
    expect(document.querySelector('dialog')?.textContent).toContain('Pinned · Exact');
    expect(document.querySelector('dialog')?.textContent).toContain('turn:4');
    expect(mutateChatStickies).not.toHaveBeenCalled();
  });
  it('saves explicit edits at the observed revision while preserving source metadata', async () => {
    render(); await settle(); await click('Sticky notes'); await click('Edit');
    const input = document.querySelector('textarea')!;
    input.value = 'Corrected exact detail'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    await click('Save changes');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3, request_id: expect.any(String), operation: { action: 'put', note: { ...note, text: 'Corrected exact detail' } } }), 'workshop-a');
  });
  it('does not silently rebase an unsaved edit after reloading a newer board', async () => {
    render(); await settle(); await click('Sticky notes'); await click('Edit');
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 4, notes: [{ ...note, text: 'Concurrent correction' }] });
    await click('Reload latest');
    expect(document.querySelector('dialog')?.textContent).toContain('The board changed since this edit began');
    vi.mocked(mutateChatStickies).mockRejectedValueOnce(new Error('sticky revision conflict'));
    await click('Save changes');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3 }), 'workshop-a');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('revision conflict');
  });
  it('requires explicit confirmation before removal', async () => {
    render(); await settle(); await click('Sticky notes'); await click('Remove');
    expect(mutateChatStickies).not.toHaveBeenCalled();
    await click('Confirm removal');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ operation: { action: 'remove', id: note.id } }), 'workshop-a');
  });
  it('retries an unknown outcome using the identical idempotent mutation', async () => {
    vi.mocked(mutateChatStickies).mockRejectedValueOnce(new Error('transport lost'));
    render(); await settle(); await click('Sticky notes'); await click('Resolve');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('not confirmed');
    const first = vi.mocked(mutateChatStickies).mock.calls[0];
    await click('Retry same change');
    expect(vi.mocked(mutateChatStickies).mock.calls[1]).toEqual(first);
  });
  it('ignores old-chat reads and clears edits when the chat or workshop changes', async () => {
    let finish!: (value: StickyBoard) => void;
    vi.mocked(readChatStickies).mockReturnValueOnce(new Promise(resolve => finish = resolve));
    const props = render(); await settle();
    props.sessionId = 'chat-b'; props.workshopScopeId = 'workshop-b'; flushSync(); await settle();
    finish({ revision: 100, notes: [{ ...note, text: 'Old chat secret' }] }); await settle();
    await click('Sticky notes');
    expect(document.querySelector('dialog')?.textContent).not.toContain('Old chat secret');
    expect(readChatStickies).toHaveBeenCalledWith('chat-b', 'workshop-b');
    expect(mutateChatStickies).not.toHaveBeenCalled();
  });
  it('clears the board when profiles change without changing chat identity', async () => {
    const props = render(); await settle(); await click('Sticky notes');
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 0, notes: [] });
    props.profileId = 'profile-b'; flushSync(); await settle();
    expect(document.querySelector('dialog')?.open).toBe(false);
    await click('Sticky notes');
    expect(document.querySelector('dialog')?.textContent).not.toContain(note.text);
    expect(mutateChatStickies).not.toHaveBeenCalled();
  });
  it('does not fetch while disabled', async () => {
    const props = $state({ sessionId: 'chat-a', enabled: false });
    const target = document.createElement('div'); document.body.append(target);
    component = mount(ChatStickyBoard, { target, props }); flushSync(); await settle();
    expect(readChatStickies).not.toHaveBeenCalled();
    expect(document.querySelector('button')).toBeNull();
  });
});
