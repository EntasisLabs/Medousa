import {
  fetchHostCharter,
  getEngineTuiDefaults,
  migrateGlobalTuiDefaultsToEngine,
  putEngineTuiDefaults,
} from "$lib/daemon";
import {
  saveProviderApiKey,
  providerApiKeyConfigured,
} from "$lib/utils/providerSettings";
import { workshopDefaultsSyncPort } from "$lib/runtime/workshopDefaultsPorts";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import {
  allowedModulesToText,
  defaultWorkshopDefaults,
  normalizeWorkshopDefaults,
  parseAllowedModulesText,
  type TuiDefaults,
} from "$lib/types/workshopDefaults";
import { syncFlatFieldsFromProfiles } from "$lib/types/inferenceProfiles";
import { isTauriMobilePlatform } from "$lib/platform";
import { workshopCharterOnHostHint } from "$lib/platformCopy";
import { isTauri } from "$lib/window";
import {
  isFavoriteModel,
  toggleFavoriteModel,
  type FavoriteModel,
} from "$lib/utils/modelCatalog";

export class WorkshopDefaultsStore {
  draft = $state<TuiDefaults>(defaultWorkshopDefaults());
  allowedModulesText = $state("");
  apiKeySet = $state(false);
  apiKeyDraft = $state("");
  clearApiKey = $state(false);
  sttApiKeySet = $state(false);
  sttApiKeyDraft = $state("");
  clearSttApiKey = $state(false);

  loading = $state(false);
  saving = $state(false);
  message = $state<string | null>(null);
  modelsNotice = $state<string | null>(null);
  loaded = $state(false);
  /** Serialized snapshot after last load/save — for dirty detection. */
  private baseline = $state<string | null>(null);

  selectedRouteRole = $state("orchestrator");

  get dirty(): boolean {
    if (!this.loaded || this.baseline == null) return false;
    return this.captureSnapshot() !== this.baseline;
  }

  private captureSnapshot(): string {
    return JSON.stringify({
      draft: this.draft,
      allowedModulesText: this.allowedModulesText,
      apiKeyDraft: this.apiKeyDraft,
      clearApiKey: this.clearApiKey,
      sttApiKeyDraft: this.sttApiKeyDraft,
      clearSttApiKey: this.clearSttApiKey,
    });
  }

  private markClean() {
    this.baseline = this.captureSnapshot();
  }

  /** Call when draft was persisted outside `save()` (e.g. Versions On/Off). */
  acknowledgeClean() {
    if (!this.loaded) return;
    this.markClean();
  }

  resetForReconnect() {
    this.loaded = false;
    this.baseline = null;
    this.message = null;
  }

  async load(force = false) {
    if (!isTauri()) {
      this.loaded = true;
      return;
    }
    if (this.loaded && !force) return;
    this.loading = true;
    this.message = null;
    try {
      if (isTauriMobilePlatform()) {
        const raw = await fetchHostCharter();
        this.draft = plainCharterCopy(normalizeWorkshopDefaults(raw));
        this.allowedModulesText = allowedModulesToText(this.draft.allowedModules);
        this.apiKeySet = false;
        this.apiKeyDraft = "";
        this.clearApiKey = false;
        workshopDefaultsSyncPort().applyVoiceDraft(this.draft);
        this.loaded = true;
        this.markClean();
        return;
      }

      await migrateGlobalTuiDefaultsToEngine().catch(() => false);
      const raw = await getEngineTuiDefaults();
      this.draft = normalizeWorkshopDefaults(raw);
      this.allowedModulesText = allowedModulesToText(this.draft.allowedModules);
      if (!this.draft.stageRouting?.orchestrator?.role) {
        this.draft.stageRouting = defaultStageRouting(
          this.draft.provider ?? "ollama",
          this.draft.model ?? "qwen2.5:7b",
        );
      }
      this.apiKeySet = this.draft.provider
        ? await providerApiKeyConfigured(this.draft.provider)
        : false;
      this.apiKeyDraft = "";
      this.clearApiKey = false;
      this.sttApiKeySet = this.draft.sttProvider
        ? await providerApiKeyConfigured(this.draft.sttProvider)
        : false;
      this.sttApiKeyDraft = "";
      this.clearSttApiKey = false;
      workshopDefaultsSyncPort().applyVoiceDraft(this.draft);
      this.loaded = true;
      this.markClean();
    } catch (err) {
      this.message = err instanceof Error ? err.message : String(err);
      this.loaded = false;
      this.baseline = null;
    } finally {
      this.loading = false;
    }
  }

  routeRoles(): string[] {
    const matrix = this.draft.stageRouting;
    if (!matrix) return [];
    return [
      "orchestrator",
      "chunker",
      "extractor",
      "summarizer",
      "verifier",
      "packer",
      "final_response",
    ];
  }

  selectedRoute() {
    const matrix = this.draft.stageRouting;
    if (!matrix) return null;
    const role = this.selectedRouteRole as keyof StageRoutingMatrix;
    return matrix[role] ?? null;
  }

  updateSelectedRoute(patch: Partial<StageRoutingMatrix["orchestrator"]>) {
    const matrix = this.draft.stageRouting;
    if (!matrix) return;
    const role = this.selectedRouteRole as keyof StageRoutingMatrix;
    const current = matrix[role];
    if (!current) return;
    this.draft.stageRouting = {
      ...matrix,
      [role]: { ...current, ...patch },
    };
  }

  favoriteModels(): FavoriteModel[] {
    return this.draft.favoriteModels ?? [];
  }

  isFavorite(provider: string, model: string): boolean {
    return isFavoriteModel(this.favoriteModels(), provider, model);
  }

  async toggleFavorite(provider: string, model: string) {
    if (!isTauri() || isTauriMobilePlatform()) return;
    const next = toggleFavoriteModel(this.favoriteModels(), provider, model);
    this.draft = { ...this.draft, favoriteModels: next };
    await putEngineTuiDefaults(syncFlatFieldsFromProfiles(this.draft));
  }

  private flashModelsNotice(text: string) {
    this.modelsNotice = text;
    setTimeout(() => {
      if (this.modelsNotice === text) {
        this.modelsNotice = null;
      }
    }, 2200);
  }

  async saveInferenceProfiles() {
    if (!isTauri()) return;
    if (isTauriMobilePlatform()) {
      this.flashModelsNotice(workshopCharterOnHostHint());
      return;
    }
    this.saving = true;
    try {
      const payload: TuiDefaults = syncFlatFieldsFromProfiles({
        ...this.draft,
        baseUrl: this.draft.baseUrl?.trim() || null,
        sttBaseUrl: this.draft.sttBaseUrl?.trim() || null,
      });
      await putEngineTuiDefaults(payload);
      this.draft = payload;
      await workshopDefaultsSyncPort().applyRuntimeFromDefaults(payload);
      this.flashModelsNotice("Saved");
      this.markClean();
    } catch (err) {
      this.flashModelsNotice(err instanceof Error ? err.message : String(err));
    } finally {
      this.saving = false;
    }
  }

  async save() {
    if (!isTauri()) return;
    if (isTauriMobilePlatform()) {
      this.message = workshopCharterOnHostHint();
      return;
    }
    this.saving = true;
    this.message = null;
    try {
      const payload: TuiDefaults = syncFlatFieldsFromProfiles({
        ...this.draft,
        baseUrl: this.draft.baseUrl?.trim() || null,
        sttBaseUrl: this.draft.sttBaseUrl?.trim() || null,
        envOverrides: this.draft.envOverrides?.trim() || null,
        allowedModules: parseAllowedModulesText(this.allowedModulesText),
      });
      if (
        payload.sliceColdWindowTurns != null &&
        payload.sliceHotWindowTurns != null &&
        payload.sliceColdWindowTurns < payload.sliceHotWindowTurns
      ) {
        payload.sliceColdWindowTurns = payload.sliceHotWindowTurns;
      }

      await putEngineTuiDefaults(payload);

      if (this.clearApiKey) {
        if (payload.provider?.trim()) {
          await saveProviderApiKey(payload.provider.trim().toLowerCase(), null);
        }
        this.apiKeySet = false;
      } else if (this.apiKeyDraft.trim()) {
        const key = this.apiKeyDraft.trim();
        const provider = payload.provider?.trim().toLowerCase();
        if (provider) {
          await saveProviderApiKey(provider, key);
        }
        this.apiKeySet = true;
        this.apiKeyDraft = "";
      }

      if (this.clearSttApiKey) {
        const sttProvider = payload.sttProvider?.trim().toLowerCase();
        if (sttProvider) {
          await saveProviderApiKey(sttProvider, null);
        }
        this.sttApiKeySet = false;
      } else if (this.sttApiKeyDraft.trim()) {
        const key = this.sttApiKeyDraft.trim();
        const sttProvider = payload.sttProvider?.trim().toLowerCase();
        if (sttProvider) {
          await saveProviderApiKey(sttProvider, key);
        }
        this.sttApiKeySet = true;
        this.sttApiKeyDraft = "";
      }

      await workshopDefaultsSyncPort().applyRuntimeFromDefaults(payload);

      this.message = "Saved";
      this.markClean();
    } catch (err) {
      this.message = err instanceof Error ? err.message : String(err);
    } finally {
      this.saving = false;
    }
  }
}

function defaultStageRouting(provider: string, model: string): StageRoutingMatrix {
  const route = (
    role: string,
    policy: string,
    fallback: string,
  ): StageRoutingMatrix["orchestrator"] => ({
    role,
    provider,
    model,
    policy_profile: policy,
    fallback_chain: [fallback, "safe-default"],
  });

  return {
    orchestrator: route("orchestrator", "balanced", "orchestrator"),
    chunker: route("chunker", "fast", "chunker"),
    extractor: route("extractor", "analytical", "extractor"),
    summarizer: route("summarizer", "balanced", "summarizer"),
    verifier: route("verifier", "strict", "verifier"),
    packer: route("packer", "balanced", "packer"),
    final_response: route("final_response", "balanced", "final_response"),
  };
}

export const workshopDefaults = new WorkshopDefaultsStore();

function plainCharterCopy(draft: TuiDefaults): TuiDefaults {
  return JSON.parse(JSON.stringify(draft)) as TuiDefaults;
}
