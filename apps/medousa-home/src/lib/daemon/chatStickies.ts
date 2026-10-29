import { daemonUnary } from './contractClient';

/** Chat-local working state, never a permanent-memory projection. */
export interface StickyNote {
  id: string;
  text: string;
  kind: string;
  pinned: boolean;
  exact: boolean;
  source_refs: string[];
  status: 'active' | 'resolved';
}
export interface StickyBoard { revision: number; notes: StickyNote[] }
export type StickyOperation =
  | { action: 'put'; note: StickyNote }
  | { action: 'pin'; id: string; pinned: boolean }
  | { action: 'resolve' | 'reopen' | 'remove'; id: string }
  | { action: 'rollup'; source_ids: string[]; note: StickyNote };
export interface StickyMutationRequest {
  expected_revision: number;
  request_id: string;
  operation: StickyOperation;
}

export function readChatStickies(sessionId: string, workshopScopeId?: string): Promise<StickyBoard> {
  return daemonUnary('sessions.by_session_id.stickies.get', { session_id: sessionId }, undefined, undefined, undefined, workshopScopeId);
}
export function mutateChatStickies(sessionId: string, request: StickyMutationRequest, workshopScopeId?: string): Promise<StickyBoard> {
  const { action, ...fields } = request.operation;
  const body = { action: `sticky.${action}`, ...fields, expected_revision: request.expected_revision, request_key: request.request_id };
  return daemonUnary('sessions.by_session_id.stickies.post', { session_id: sessionId }, body, undefined, undefined, workshopScopeId);
}
