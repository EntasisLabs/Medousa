import {
  getCapability,
  getManuscript,
  importManuscripts,
  listCapabilities,
  listManuscripts,
  updateManuscript,
} from "$lib/daemon";
import type {
  CapabilityListEntry,
  CapabilityResolveResponse,
  ManuscriptCatalogEntry,
} from "$lib/types/catalog";
import type {
  ManuscriptDetailResponse,
  ManuscriptImportRequest,
  ManuscriptImportResponse,
  UpdateManuscriptRequest,
} from "$lib/types/manuscript";

export class CatalogStore {
  manuscripts = $state<ManuscriptCatalogEntry[]>([]);
  capabilities = $state<CapabilityListEntry[]>([]);
  capabilityDetail = $state<CapabilityResolveResponse | null>(null);
  capabilityDetailId = $state<string | null>(null);
  capabilityDetailLoading = $state(false);
  capabilityDetailError = $state<string | null>(null);
  manuscriptDetail = $state<ManuscriptDetailResponse | null>(null);
  manuscriptDetailId = $state<string | null>(null);
  manuscriptDetailLoading = $state(false);
  manuscriptDetailError = $state<string | null>(null);
  manuscriptSaveBusy = $state(false);
  manuscriptSaveMessage = $state<string | null>(null);
  importBusy = $state(false);
  importError = $state<string | null>(null);
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

  async loadManuscriptDetail(manuscriptId: string) {
    if (this.manuscriptDetailId === manuscriptId && this.manuscriptDetail) {
      return;
    }
    this.manuscriptDetailId = manuscriptId;
    this.manuscriptDetailLoading = true;
    this.manuscriptDetailError = null;
    this.manuscriptSaveMessage = null;
    try {
      this.manuscriptDetail = await getManuscript(manuscriptId);
    } catch (err) {
      this.manuscriptDetail = null;
      this.manuscriptDetailError =
        err instanceof Error ? err.message : String(err);
    } finally {
      this.manuscriptDetailLoading = false;
    }
  }

  clearManuscriptDetail() {
    this.manuscriptDetailId = null;
    this.manuscriptDetail = null;
    this.manuscriptDetailError = null;
    this.manuscriptSaveMessage = null;
  }

  async saveManuscriptDetail(
    manuscriptId: string,
    request: UpdateManuscriptRequest,
  ) {
    this.manuscriptSaveBusy = true;
    this.manuscriptSaveMessage = null;
    this.manuscriptDetailError = null;
    try {
      this.manuscriptDetail = await updateManuscript(manuscriptId, request);
      this.manuscriptDetailId = manuscriptId;
      this.manuscriptSaveMessage = "Saved";
      await this.refresh();
    } catch (err) {
      this.manuscriptDetailError =
        err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.manuscriptSaveBusy = false;
    }
  }

  async importSpecialists(
    request: ManuscriptImportRequest,
  ): Promise<ManuscriptImportResponse> {
    this.importBusy = true;
    this.importError = null;
    try {
      const response = await importManuscripts(request);
      await this.refresh();
      return response;
    } catch (err) {
      this.importError = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.importBusy = false;
    }
  }
}

export const catalog = new CatalogStore();
