import { listVaultRoots, searchVaultNotes } from "$lib/daemon";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import type { VaultSearchHit } from "$lib/types/vault";

export type RootNoteHit = VaultSearchHit & { rootId: string; rootLabel: string };
export class VaultSearchCatalog {
  hits = $state<RootNoteHit[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  query = $state("");
  scope = $state("");
  #sequence = 0;
  #pending: { key: string; work: Promise<void> } | null = null;
  #loadedAt = 0;
  resultsFor(query: string): RootNoteHit[] {
    return this.scope === activeWorkshopId() && this.query === query.trim() ? this.hits : [];
  }
  search(query: string): Promise<void> {
    query = query.trim();
    const scope = activeWorkshopId();
    const key = JSON.stringify([scope, query]);
    if (this.#pending?.key === key) return this.#pending.work;
    if (this.scope === scope && this.query === query && Date.now() - this.#loadedAt < 3000) return Promise.resolve();
    const sequence = ++this.#sequence;
    this.loading = true;
    this.error = null;
    const current = () => sequence === this.#sequence && scope === activeWorkshopId();
    const work = (async () => {
      try {
        await new Promise(resolve => setTimeout(resolve, query ? 80 : 0));
        if (!current()) return;
        const roots = await listVaultRoots();
        if (!current()) return;
        const results = await Promise.allSettled(roots.roots.map(async root => {
          const result = await searchVaultNotes(query, 24, undefined, root.id);
          if (result.root_id !== root.id) {
            throw new Error("Update the workshop engine to search linked vaults safely.");
          }
          return result.hits.map(hit => ({ ...hit, rootId: root.id, rootLabel: root.label }));
        }));
        if (!current()) return;
        this.scope = scope;
        this.query = query;
        this.hits = results.flatMap(result => result.status === "fulfilled" ? result.value : []);
        const unavailable = roots.roots.filter((_, index) => results[index].status === "rejected").map(root => root.label);
        const firstFailure = results.find(result => result.status === "rejected");
        this.error = unavailable.length
          ? `Couldn’t search ${unavailable.join(", ")}. ${results.some(result => result.status === "fulfilled")
            ? "Other vault results remain available."
            : String(firstFailure?.reason)}`
          : null;
        this.#loadedAt = Date.now();
      } catch (cause) {
        if (current()) {
          this.scope = scope;
          this.query = query;
          this.hits = [];
          this.error = cause instanceof Error ? cause.message : String(cause);
        }
      } finally {
        if (sequence === this.#sequence) this.loading = false;
      }
    })();
    this.#pending = { key, work };
    void work.finally(() => { if (this.#pending?.work === work) this.#pending = null; });
    return work;
  }
}
export const vaultSearchCatalog = new VaultSearchCatalog();
