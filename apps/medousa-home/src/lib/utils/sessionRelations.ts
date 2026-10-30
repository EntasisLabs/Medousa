import type { SessionSummary } from '$lib/types/session';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';

export interface SessionRelation { child: string; parent: string; title: string }

/** A request alone is not proof that an execution chat was created. */
export function proposalRelations(rows: PeerProposalReviewRecord[], authority: string): SessionRelation[] {
  return rows.flatMap(row => {
    const request = row.proposal.request;
    const execution = (row.binding ?? row.receipt?.binding)?.execution_session;
    if (!execution || !authority || request.owner_session.authority_id !== authority || execution.authority_id !== authority
      || execution.session_id === request.owner_session.session_id) return [];
    const title = request.instructions.trim().split('\n').find(line => line.trim())?.trim().slice(0, 80) || 'Related work';
    return [{ child: execution.session_id, parent: request.owner_session.session_id, title }];
  });
}

/** Resolve only acyclic, unambiguous recorded links; flatten grandchildren. */
export function sessionFamilies(sessions: SessionSummary[], relations: SessionRelation[], pinned: Set<string>) {
  const byId = new Map(sessions.map(session => [session.session_id, session]));
  const links = new Map<string, SessionRelation>();
  const conflicts = new Set<string>();
  for (const relation of relations) {
    if (links.has(relation.child) && links.get(relation.child)!.parent !== relation.parent) conflicts.add(relation.child);
    links.set(relation.child, relation);
  }
  for (const id of conflicts) links.delete(id);
  const rootFor = (id: string): string | null => {
    const seen = new Set([id]);
    let parent = links.get(id)?.parent;
    if (!parent || !byId.has(parent)) return null;
    while (parent) {
      if (seen.has(parent)) return null;
      seen.add(parent);
      const next: string | undefined = links.get(parent)?.parent;
      if (!next || !byId.has(next)) return parent;
      parent = next;
    }
    return null;
  };
  const children = new Map<string, SessionSummary[]>();
  const parents = new Map<string, string>();
  const display = (session: SessionSummary): SessionSummary => {
    const title = links.get(session.session_id)?.title;
    // Preserve user names, use the task only for generic agent labels.
    return title && (!session.display_name?.trim() || /^medousa coder$/i.test(session.display_name.trim()))
      ? { ...session, display_name: title } : session;
  };
  for (const session of sessions) {
    const root = rootFor(session.session_id);
    if (root) parents.set(session.session_id, root);
    if (!root || pinned.has(session.session_id)) continue;
    children.set(root, [...(children.get(root) ?? []), display(session)]);
  }
  return {
    roots: sessions.filter(session => !parents.has(session.session_id) || pinned.has(session.session_id)).map(display),
    children, parents, display,
  };
}
