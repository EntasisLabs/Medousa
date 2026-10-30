/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ChatChangeReceipt from './ChatChangeReceipt.svelte';
import type { ReviewProjection } from '$lib/forge';

const api = vi.hoisted(() => ({ changes: vi.fn() }));
vi.mock('$lib/forge', () => ({ getForgeChanges: api.changes, addReviewComment: vi.fn(), deleteReviewComment: vi.fn(), resolveReviewComment: vi.fn() }));
vi.mock('$lib/code/codeProjectEvents', () => ({ subscribeCodeProjectEvents: () => () => {} }));
vi.mock('$lib/runtime/layout.svelte', () => ({ layout: { mobile: false } }));
vi.mock('./ReviewGitActions.svelte', () => ({ default: vi.fn() }));
vi.mock('./WorkingChangesBrowser.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/work/ForgeReviewSurface.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/work/ReviewCommentRail.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/mobile/MobileActionSheet.svelte', () => ({ default: vi.fn() }));
let component: ReturnType<typeof mount>;
beforeEach(() => vi.useFakeTimers());
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); vi.clearAllMocks(); vi.useRealTimers(); });
async function render(review: ReviewProjection | null = null) {
  const target = document.createElement('div'); document.body.append(target);
  const props = $state({ workId: 'work-a', projectTitle: 'Medousa', phase: 'working', compact: true, review,
    onOpenCode: vi.fn(), onRequestRevision: vi.fn() });
  api.changes.mockResolvedValue({ files: [{ path: 'a.ts', status: 'modified' }, { path: 'b.ts', status: 'added' }] });
  component = mount(ChatChangeReceipt, { target, props }); flushSync();
  await vi.advanceTimersByTimeAsync(250); await tick(); flushSync();
  return props;
}

describe('compact composer change receipt', () => {
  it('shows only the changed-file count, then opens the existing review without mutating work', async () => {
    const props = await render();
    const pill = document.querySelector('.change-pill') as HTMLButtonElement;
    expect(pill.textContent).toContain('2 changes');
    expect(pill.getAttribute('aria-label')).toContain('working copy');
    expect(pill.getAttribute('aria-haspopup')).toBe('dialog');
    expect(pill.getAttribute('aria-expanded')).toBe('false');
    expect(pill.querySelector('.change-action')?.textContent).toContain('View');
    expect(document.querySelector('.chat-change-receipt')).toBeNull();
    expect(document.querySelector('.chat-review-sheet')).toBeNull();
    pill.click(); flushSync(); await tick();
    expect(pill.getAttribute('aria-expanded')).toBe('true');
    expect(document.querySelector('.chat-review-sheet')).not.toBeNull();
    expect(props.onOpenCode).not.toHaveBeenCalled();
    expect(props.onRequestRevision).not.toHaveBeenCalled();
  });

  it('counts sealed review files only for the exact bound project', async () => {
    const review = { work_id: 'work-a', human_phase: 'review', changed_files: [{ path: 'sealed.ts', status: 'modified' }], synthesis: {}, comments: [] } as unknown as ReviewProjection;
    const props = await render(review);
    expect(document.querySelector('.change-pill')?.textContent).toContain('1 change');
    expect(document.querySelector('.change-pill')?.getAttribute('aria-label')).toContain('ready for review');
    expect(document.querySelector('.change-pill--ready')).not.toBeNull();
    expect(document.querySelector('.change-action')?.textContent).toContain('Review');
    props.review = { ...review, work_id: 'unrelated' }; flushSync(); await vi.advanceTimersByTimeAsync(250); await tick(); flushSync();
    expect(document.querySelector('.change-pill')?.textContent).toContain('2 changes');
    expect(document.querySelector('.change-pill')?.getAttribute('aria-label')).toContain('working copy');
  });

  it('closes an open review when its project changes', async () => {
    const props = await render();
    (document.querySelector('.change-pill') as HTMLButtonElement).click(); flushSync();
    expect(document.querySelector('.chat-review-sheet')).not.toBeNull();
    props.workId = 'work-b'; flushSync(); await tick();
    expect(document.querySelector('.chat-review-sheet')).toBeNull();
  });
});
