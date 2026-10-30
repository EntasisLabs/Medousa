import { describe, expect, it } from 'vitest';
import { proposalRelations, sessionFamilies, type SessionRelation } from './sessionRelations';
import type { SessionSummary } from '$lib/types/session';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';
const session = (id: string, name = id): SessionSummary => ({ session_id: id, display_name: name, preview: '', turns: 1, verification_runs: 0 });
const link = (child: string, parent: string): SessionRelation => ({ child, parent, title: 'Polish sidebar' });

describe('recorded session families', () => {
  it('nests children once, flattens grandchildren and preserves chosen names', () => {
    const family = sessionFamilies([session('parent'), session('child', 'Medousa Coder'), session('grandchild', 'My review')], [link('child', 'parent'), link('grandchild', 'child')], new Set());
    expect(family.roots.map(row => row.session_id)).toEqual(['parent']);
    expect(family.children.get('parent')?.map(row => row.display_name)).toEqual(['Polish sidebar', 'My review']);
    expect(family.parents.get('grandchild')).toBe('parent');
  });
  it('keeps missing parents, cycles and conflicting parents standalone', () => {
    const rows = [session('a'), session('b'), session('c')];
    for (const links of [[link('a', 'missing')], [link('a', 'b'), link('b', 'a')], [link('a', 'b'), link('a', 'c')]]) {
      expect(sessionFamilies(rows, links, new Set()).roots).toHaveLength(3);
    }
  });
  it('leaves pinned children at top level without duplicating them under a parent', () => {
    const result = sessionFamilies([session('p'), session('c')], [link('c', 'p')], new Set(['c']));
    expect(result.roots).toHaveLength(2);
    expect(result.children.size).toBe(0);
    expect(result.parents.get('c')).toBe('p');
  });
  it('never infers provenance from identical generic names', () => {
    expect(sessionFamilies([session('a', 'Medousa Coder'), session('b', 'Medousa Coder')], [], new Set()).roots).toHaveLength(2);
  });
  it('requires confirmed execution binding and matching authority for both sessions', () => {
    const row = {
      proposal: { request: { owner_session: { session_id: 'p', authority_id: 'here' }, instructions: 'Polish sidebar\nDetails' } },
      binding: { execution_session: { session_id: 'c', authority_id: 'here' } },
    } as PeerProposalReviewRecord;
    expect(proposalRelations([row], 'here')).toEqual([link('c', 'p')]);
    expect(proposalRelations([row], 'elsewhere')).toEqual([]);
    expect(proposalRelations([{ ...row, binding: null }], 'here')).toEqual([]);
    expect(proposalRelations([{ ...row, binding: { ...row.binding!, execution_session: { session_id: 'c', authority_id: 'remote' } } }], 'here')).toEqual([]);
    expect(proposalRelations([{ ...row, binding: null, receipt: { binding: row.binding! } } as PeerProposalReviewRecord], 'here')).toEqual([link('c', 'p')]);
  });
});
