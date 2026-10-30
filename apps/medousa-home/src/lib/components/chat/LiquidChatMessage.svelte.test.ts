/** @vitest-environment happy-dom */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createRawSnippet, flushSync, mount, unmount } from 'svelte';
import LiquidChatMessage from './LiquidChatMessage.svelte';
import type { ChatMessage } from '$lib/types/chat';

vi.mock('$lib/liquid/render', async () => ({ SceneRenderer: (await import('./LiquidDelegationScene.testfixture.svelte')).default }));
vi.mock('$lib/liquid/archetypes/registerUi', () => ({ installLiquidUi: vi.fn() }));
vi.mock('$lib/components/ui/MarkdownContent.svelte', () => ({ default: vi.fn() }));
vi.mock('$lib/components/ui/bindMarkdownView', () => ({}));
vi.mock('$lib/stores/settings.svelte', () => ({ settings: { showEngineDetailsInChat: false } }));
const scenes = vi.hoisted(() => ({ get: vi.fn(() => null) }));
vi.mock('$lib/liquid/surfaces/chat/chatScenes.svelte', () => ({ chatScenes: scenes }));
vi.mock('$lib/liquid/surfaces/chat/chatEventSink', () => ({ createChatEventSink: vi.fn() }));
vi.mock('$lib/liquid/surfaces/chat/chatInteractions', () => ({ chatInteractions: { record: vi.fn() } }));

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.clearAllMocks(); });
const message: ChatMessage = { id: 'reply', role: 'assistant', content: 'Before After', segments: [
  { kind: 'text', segmentId: 'before', modelRound: 1, markdown: 'Before', committed: true },
  { kind: 'handoff', handoffKind: 'worker', workId: 'work', text: 'Spawned' },
  { kind: 'text', segmentId: 'after', modelRound: 2, markdown: 'After', committed: true },
] };
function render(ids: string[]) {
  const target = document.createElement('div'); document.body.append(target);
  const afterNode = createRawSnippet<[string]>(id => ({ render: () => `<aside data-receipt="${id()}">Task receipt</aside>` }));
  component = mount(LiquidChatMessage, { target, props: { message, sessionId: 'chat', delegationNodeIds: ids, afterNode } });
  flushSync(); return target;
}
describe('delegation scene insertion', () => {
  it('inserts a receipt after the initiating segment, before subsequent answer text', () => {
    const target = render(['reply:segment:handoff:work:1']);
    expect([...target.querySelectorAll('[data-scene-id], [data-receipt]')].map(node => node.getAttribute('data-scene-id') ?? node.getAttribute('data-receipt'))).toEqual([
      'reply:segment:text:before', 'reply:segment:handoff:work:1', 'reply:segment:handoff:work:1', 'reply:segment:text:after',
    ]);
    expect(target.querySelectorAll('[data-receipt]')).toHaveLength(1);
  });
  it('keeps receipts visible before a replacement daemon scene that omits their segment', () => {
    scenes.get.mockReturnValueOnce({ root: { id: 'custom', type: 'stack', props: {}, slots: {}, fillState: 'ready' } } as never);
    const target = render(['reply:segment:handoff:work:1', 'reply:segment:handoff:work:1']);
    expect(target.firstElementChild?.tagName).toBe('ASIDE');
    expect(target.querySelector('[data-scene-id="custom"]')).not.toBeNull();
    expect(target.querySelectorAll('[data-receipt]')).toHaveLength(1);
  });
});
