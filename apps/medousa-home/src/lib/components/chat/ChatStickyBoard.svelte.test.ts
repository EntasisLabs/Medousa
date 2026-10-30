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
  vi.spyOn(HTMLDialogElement.prototype, 'show').mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.open = false; this.dispatchEvent(new Event('close')); });
});
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); vi.restoreAllMocks(); });

describe('chat sticky inspector', () => {
  it('uses vector icons and anchors the notebook to the right edge of the notes trigger', async () => {
    render(); await settle();
    const trigger = document.querySelector<HTMLButtonElement>('.sticky-trigger')!;
    expect(trigger.querySelectorAll('svg[aria-hidden="true"]')).toHaveLength(2);
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue({ left: 700, right: 820, top: 600, bottom: 632, width: 120, height: 32, x: 700, y: 600, toJSON: () => ({}) });
    await click('Sticky notes');
    const notebook = document.querySelector<HTMLDialogElement>('dialog')!;
    expect(notebook.style.left).toBe('420px');
    expect(trigger.querySelector('.chevron.expanded')).not.toBeNull();
    expect(document.querySelector('button[aria-label="Close sticky notes"] svg')).not.toBeNull();
    expect(mutateChatStickies).not.toHaveBeenCalled();
  });
  it('inspects scoped notes without mutating and protects exact/source text in presentation', async () => {
    render(); await settle(); await click('Sticky notes');
    expect(readChatStickies).toHaveBeenCalledWith('chat-a', 'workshop-a');
    expect(document.querySelector('dialog')?.open).toBe(true);
    expect(document.querySelector('dialog')?.textContent).toContain(note.text);
    expect(document.querySelector('dialog')?.textContent).toContain('Pinned · Exact');
    expect(document.querySelector('textarea')).toBeNull();
    expect(document.querySelector('dialog')?.textContent).not.toContain('Resolve');
    await click('⋯');
    expect(document.querySelector('dialog')?.textContent).toContain('turn:4');
    expect(mutateChatStickies).not.toHaveBeenCalled();
  });
  it('saves explicit edits at the observed revision while preserving source metadata', async () => {
    render(); await settle(); await click('Sticky notes'); await click(note.text);
    const input = document.querySelector('textarea')!;
    input.value = 'Corrected exact detail'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    await click('Save changes');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3, request_id: expect.any(String), operation: { action: 'put', note: { ...note, text: 'Corrected exact detail' } } }), 'workshop-a');
  });
  it('does not silently rebase an unsaved edit after reloading a newer board', async () => {
    render(); await settle(); await click('Sticky notes'); await click(note.text);
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 4, notes: [{ ...note, text: 'Concurrent correction' }] });
    await click('Reload latest');
    expect(document.querySelector('dialog')?.textContent).toContain('The board changed since this edit began');
    vi.mocked(mutateChatStickies).mockRejectedValueOnce(new Error('sticky revision conflict'));
    await click('Save changes');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3 }), 'workshop-a');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('revision conflict');
  });
  it('requires explicit confirmation before removal', async () => {
    render(); await settle(); await click('Sticky notes'); await click('⋯'); await click('Remove');
    expect(mutateChatStickies).not.toHaveBeenCalled();
    await click('Confirm removal');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ operation: { action: 'remove', id: note.id } }), 'workshop-a');
  });
  it('retries an unknown outcome using the identical idempotent mutation', async () => {
    vi.mocked(mutateChatStickies).mockRejectedValueOnce(new Error('transport lost'));
    render(); await settle(); await click('Sticky notes'); await click('⋯'); await click('Resolve');
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
  it('is nonmodal, returns focus on Escape, and dismisses on outside pointer interaction', async () => {
    render(); await settle(); await click('Sticky notes');
    expect(HTMLDialogElement.prototype.show).toHaveBeenCalledOnce();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await settle();
    expect(document.querySelector('dialog')?.open).toBe(false);
    expect(document.activeElement).toBe(button('Sticky notes'));
    await click('Sticky notes');
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await settle();
    expect(document.querySelector('dialog')?.open).toBe(false);
  });
  it('adds inline only on request and preserves the draft when dismissed', async () => {
    render(); await settle(); await click('Sticky notes');
    expect(document.querySelector('textarea')).toBeNull();
    await click('+ Add a note');
    const input = document.querySelector('textarea')!;
    expect(document.activeElement).toBe(input);
    input.value = 'Keep this new detail'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); await settle();
    await click('Sticky notes');
    expect(document.querySelector('textarea')?.value).toBe('Keep this new detail');
    await click('Add sticky note');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3, operation: { action: 'put', note: expect.objectContaining({ text: 'Keep this new detail', pinned: true, exact: false, status: 'active' }) } }), 'workshop-a');
  });
  it('unpins through the overflow menu at the observed revision', async () => {
    render(); await settle(); await click('Sticky notes'); await click('⋯'); await click('Unpin');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3, operation: { action: 'pin', id: note.id, pinned: false } }), 'workshop-a');
  });
  it('toggles exact preservation without losing note metadata', async () => {
    render(); await settle(); await click('Sticky notes'); await click('⋯'); await click('Allow summarizing');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ expected_revision: 3, operation: { action: 'put', note: { ...note, exact: false } } }), 'workshop-a');
  });
  it('reveals resolved notes and allows reopening them', async () => {
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 3, notes: [{ ...note, status: 'resolved' }] });
    render(); await settle(); await click('Sticky notes');
    expect(document.querySelector('.note-text')).toBeNull();
    await click('Show resolved'); await click('⋯'); await click('Reopen');
    expect(mutateChatStickies).toHaveBeenCalledWith('chat-a', expect.objectContaining({ operation: { action: 'reopen', id: note.id } }), 'workshop-a');
  });
  it('refreshes automatically without overwriting an inline edit', async () => {
    vi.useFakeTimers();
    try {
      render(); await settle(); await click('Sticky notes');
      const reads = vi.mocked(readChatStickies).mock.calls.length;
      await vi.advanceTimersByTimeAsync(5000); await settle();
      expect(readChatStickies).toHaveBeenCalledTimes(reads + 1);
      await click(note.text);
      const input = document.querySelector('textarea')!;
      input.value = 'Unsaved change'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
      await vi.advanceTimersByTimeAsync(10000); await settle();
      expect(readChatStickies).toHaveBeenCalledTimes(reads + 1);
      expect(input.value).toBe('Unsaved change');
    } finally { vi.useRealTimers(); }
  });

  it('keeps an edit visible if a manual reload no longer includes its note', async () => {
    render(); await settle(); await click('Sticky notes'); await click(note.text);
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 4, notes: [] });
    await click('Reload latest');
    expect(document.querySelector('textarea')?.value).toBe(note.text);
    expect(document.querySelector('dialog')?.textContent).toContain('The board changed since this edit began');
    await click('Cancel');
    expect(document.querySelector('textarea')).toBeNull();
  });
  it('ignores an old-profile mutation result after switching profiles', async () => {
    let finish!: (value: StickyBoard) => void;
    vi.mocked(mutateChatStickies).mockReturnValueOnce(new Promise(resolve => finish = resolve));
    const props = render(); await settle(); await click('Sticky notes'); await click('⋯');
    button('Resolve').click(); await settle();
    vi.mocked(readChatStickies).mockResolvedValue({ revision: 0, notes: [] });
    props.profileId = 'profile-b'; flushSync(); await settle();
    finish({ revision: 4, notes: [{ ...note, text: 'Old profile secret' }] }); await settle();
    await click('Sticky notes');
    expect(document.querySelector('dialog')?.textContent).not.toContain('Old profile secret');
    expect(document.querySelector('dialog')?.textContent).not.toContain(note.text);
  });

});
