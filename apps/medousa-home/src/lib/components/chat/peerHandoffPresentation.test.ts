import { describe, expect, it } from 'vitest';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';
import { peerHandoffPresentation, peerWorkTitle } from './peerHandoffPresentation';

function work(): PeerProposalReviewRecord {
  const sender = { authority_id: 'sender-authority', session_id: 'sender' };
  return {
    proposal: { request: { owner_session: sender } },
    handoff: { admission: 'delegate', policy: {}, responsible_session: sender, state: 'working' },
  } as PeerProposalReviewRecord;
}

describe('sender-controlled handoff presentation', () => {
  it('does not equate a worker result with review when completion policy skips review', () => {
    const row = work();
    row.handoff!.policy = { completion: 'worker_result', responsibility: 'retain' };
    row.handoff!.state = 'accepted';
    row.receipt = { outcome: 'completed' } as NonNullable<typeof row.receipt>;
    const result = peerHandoffPresentation(row, Date.now());
    expect(result.status).toBe('Result returned');
    expect(result.senderResponsible).toBe(true);
    expect(result.senderStatus).not.toContain('Reviewed');
  });

  it('keeps a returned result above stale running progress without a handoff record', () => {
    const row = work(); row.handoff = null;
    row.progress = { state: 'running', current_activity: 'Old action' } as NonNullable<typeof row.progress>;
    row.receipt = { outcome: 'completed' } as NonNullable<typeof row.receipt>;
    expect(peerHandoffPresentation(row, Date.now()).status).toBe('Result returned');
  });

  it('keeps legacy past-deadline proposals actionable until an explicit decision', () => {
    const row = work(); row.handoff = null;
    row.proposal.expires_at = '2020-01-01T00:00:00Z';
    expect(peerHandoffPresentation(row, Date.now())).toMatchObject({ needsApproval: true, status: 'Needs your approval', nextActor: 'You' });
    row.decision = { approved: true } as NonNullable<typeof row.decision>;
    expect(peerHandoffPresentation(row, Date.now())).toMatchObject({ needsApproval: true, status: 'Approved · ready to start', nextActor: 'You' });
  });

  it('keeps the sender next on changes requested even when an old receipt is present', () => {
    const row = work(); row.handoff!.state = 'changes_requested';
    row.receipt = { outcome: 'completed' } as NonNullable<typeof row.receipt>;
    expect(peerHandoffPresentation(row, Date.now()).nextActor).toBe('Medousa');
    row.handoff!.responsible_session = { authority_id: 'other', session_id: 'other' };
    expect(peerHandoffPresentation(row, Date.now()).nextActor).toBe('Responsible agent');
  });

  it('does not advertise failed handoffs as awaiting the agent or trust stale blocked progress after a result', () => {
    const row = work(); row.handoff!.state = 'failed';
    expect(peerHandoffPresentation(row, Date.now())).toMatchObject({ attention: true, nextActor: 'Medousa' });
    row.handoff!.state = 'accepted';
    row.receipt = { outcome: 'completed' } as NonNullable<typeof row.receipt>;
    row.progress = { state: 'blocked' } as NonNullable<typeof row.progress>;
    expect(peerHandoffPresentation(row, Date.now())).toMatchObject({ attention: false, nextActor: null });
  });

  it('uses the actual responsible session rather than treating transfer intent as a completed transfer', () => {
    const row = work();
    row.handoff!.policy.responsibility = 'transfer';
    expect(peerHandoffPresentation(row, Date.now()).senderResponsible).toBe(true);
    row.handoff!.responsible_session = { authority_id: 'worker-authority', session_id: 'worker' };
    expect(peerHandoffPresentation(row, Date.now()).senderResponsible).toBe(false);
  });

  it('gives an unsuccessful terminal result priority over a review state and stale progress', () => {
    const row = work();
    row.handoff!.state = 'awaiting_sender_review';
    row.receipt = { outcome: 'failed' } as NonNullable<typeof row.receipt>;
    const result = peerHandoffPresentation(row, Date.now());
    expect(result.status).toBe('Work failed');
    expect(result.attention).toBe(true);
  });
});

describe('agent task titles', () => {
  it('keeps distinct task titles on the same project', () => {
    expect(peerWorkTitle('Validate rollback\nFull instructions', 'Hashmap')).toBe('Validate rollback');
    expect(peerWorkTitle('Update operator guide', 'Hashmap')).toBe('Update operator guide');
  });
  it('extracts the named undertaking from a scoped assignment', () => {
    expect(peerWorkTitle('Work only in the isolated Forge undertaking "Prepare Hashmap deployment". Full instructions')).toBe('Prepare Hashmap deployment');
  });
});
