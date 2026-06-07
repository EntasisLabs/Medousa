import { listCapabilities, listManuscripts } from "$lib/daemon";
import type {
  CapabilityListEntry,
  ManuscriptCatalogEntry,
} from "$lib/types/catalog";

export class CatalogStore {
  manuscripts = $state<ManuscriptCatalogEntry[]>([]);
  capabilities = $state<CapabilityListEntry[]>([]);
  error = $state<string | null>(null);
  loading = $state(false);
  skillsOnly = $state(true);

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const [manuscripts, capabilities] = await Promise.all([
        listManuscripts({ skillsOnly: this.skillsOnly, limit: 100 }),
        listCapabilities(),
      ]);
      this.manuscripts = manuscripts.manuscripts;
      this.capabilities = capabilities.capabilities;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }
}

export const catalog = new CatalogStore();
