import {
  getCapability,
  listCapabilities,
  listManuscripts,
} from "$lib/daemon";
import type {
  CapabilityListEntry,
  CapabilityResolveResponse,
  ManuscriptCatalogEntry,
} from "$lib/types/catalog";

export class CatalogStore {
  manuscripts = $state<ManuscriptCatalogEntry[]>([]);
  capabilities = $state<CapabilityListEntry[]>([]);
  capabilityDetail = $state<CapabilityResolveResponse | null>(null);
  capabilityDetailId = $state<string | null>(null);
  capabilityDetailLoading = $state(false);
  capabilityDetailError = $state<string | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const [manuscripts, capabilities] = await Promise.all([
        listManuscripts({ skillsOnly: false, limit: 200 }),
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

  async loadCapabilityDetail(capabilityId: string) {
    if (this.capabilityDetailId === capabilityId && this.capabilityDetail) {
      return;
    }
    this.capabilityDetailId = capabilityId;
    this.capabilityDetailLoading = true;
    this.capabilityDetailError = null;
    try {
      this.capabilityDetail = await getCapability(capabilityId);
    } catch (err) {
      this.capabilityDetail = null;
      this.capabilityDetailError =
        err instanceof Error ? err.message : String(err);
    } finally {
      this.capabilityDetailLoading = false;
    }
  }

  clearCapabilityDetail() {
    this.capabilityDetailId = null;
    this.capabilityDetail = null;
    this.capabilityDetailError = null;
  }
}

export const catalog = new CatalogStore();
