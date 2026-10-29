import { beforeEach, describe, expect, it, vi } from 'vitest';
import { readChatStickies, mutateChatStickies } from './chatStickies';
import { daemonUnary } from './contractClient';
vi.mock('./contractClient', () => ({ daemonUnary: vi.fn() }));
beforeEach(() => vi.mocked(daemonUnary).mockReset());
describe('chat sticky client', () => {
  it('uses the workshop-scoped operation transport for reads', async () => {
    vi.mocked(daemonUnary).mockResolvedValue({ revision: 0, notes: [] });
    await expect(readChatStickies('chat/with spaces', 'workshop-a')).resolves.toEqual({ revision: 0, notes: [] });
    expect(daemonUnary).toHaveBeenCalledWith('sessions.by_session_id.stickies.get', { session_id: 'chat/with spaces' }, undefined, undefined, undefined, 'workshop-a');
  });
  it('maps mutations to the same typed actions used by tools without changing retry identity', async () => {
    const request = { expected_revision: 2, request_id: 'retry-key', operation: { action: 'resolve' as const, id: 'detail' } };
    await mutateChatStickies('chat-a', request, 'workshop-a');
    await mutateChatStickies('chat-a', request, 'workshop-a');
    expect(daemonUnary).toHaveBeenCalledWith('sessions.by_session_id.stickies.post', { session_id: 'chat-a' }, { action: 'sticky.resolve', id: 'detail', expected_revision: 2, request_key: 'retry-key' }, undefined, undefined, 'workshop-a');
    expect(vi.mocked(daemonUnary).mock.calls[0]).toEqual(vi.mocked(daemonUnary).mock.calls[1]);
  });
});
