import { proposalRelations, type SessionRelation } from '$lib/utils/sessionRelations';
import type { PeerProposalReviewRecord } from '$lib/types/generated/daemon_api';

/** Home-only provenance index. No daemon names, work state or transcripts are changed. */
class SidebarRelations {
  scope = $state('');
  links = $state<SessionRelation[]>([]);
  load(scope: string) {
    if (scope === this.scope) return;
    this.scope = scope;
    try {
      const value: unknown = JSON.parse(localStorage.getItem(scope + ':relations') ?? '[]');
      this.links = Array.isArray(value) ? value.filter((item): item is SessionRelation => item && typeof item.child === 'string' && typeof item.parent === 'string' && typeof item.title === 'string').slice(-2000) : [];
    } catch { this.links = []; }
  }
  observe(scope: string, rows: PeerProposalReviewRecord[], authority: string) {
    if (scope !== this.scope) return;
    const updates = proposalRelations(rows, authority);
    if (!updates.length) return;
    const links = new Map(this.links.map(link => [JSON.stringify([link.child, link.parent]), link]));
    for (const link of updates) links.set(JSON.stringify([link.child, link.parent]), link);
    this.links = [...links.values()].slice(-2000);
    try { localStorage.setItem(scope + ':relations', JSON.stringify(this.links)); } catch { /* Ephemeral fallback. */ }
  }
}
export const sidebarRelations = new SidebarRelations();
