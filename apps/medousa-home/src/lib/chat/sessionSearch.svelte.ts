import type { SessionSummary } from "$lib/types/session";
import { mergeSessionSummaries, readSessionPages } from "./sessionDiscovery";

/** One instance per search surface, independent of browsing and other queries. */
export class SessionSearch {
  scope = $state("");
  query = $state("");
  sessions = $state<SessionSummary[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  private revision = 0;

  cancel() {
    this.revision += 1;
    this.sessions = [];
    this.loading = false;
    this.error = null;
  }

  async search(scope: string, query: string) {
    const revision = ++this.revision;
    this.scope = scope;
    this.query = query.trim();
    this.sessions = [];
    this.error = null;
    this.loading = Boolean(scope && this.query);
    if (!this.loading) return;
    const current = () => revision === this.revision;
    const needle = this.query;
    await new Promise(resolve => setTimeout(resolve, 180));
    if (!current()) return;
    try {
      const response = await readSessionPages({ q: needle, current });
      if (response && current()) this.sessions = response.sessions;
    } catch (error) {
      if (current()) this.error = error instanceof Error ? error.message : String(error);
    } finally {
      if (current()) this.loading = false;
    }
  }

  resultsFor(scope: string, query: string, cached: SessionSummary[]): SessionSummary[] {
    const needle = query.trim().toLowerCase();
    const remote = this.scope === scope && this.query.toLowerCase() === needle ? this.sessions : [];
    // Consumers keep their own title/fuzzy/family-label matching semantics.
    return mergeSessionSummaries(cached, remote);
  }
}

export const spotlightSessionSearch = new SessionSearch();
