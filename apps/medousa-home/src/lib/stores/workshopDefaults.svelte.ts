import {
  loadTuiDefaults,
  persistTuiDefaults,
  persistTuiRuntimePrefs,
} from "$lib/config";
import { messagingSecretStatus, messagingSaveSecret, messagingClearSecret } from "$lib/messaging";
import { runtime } from "$lib/stores/runtime.svelte";
import type { StageRoutingMatrix } from "$lib/types/runtime";
import {
  allowedModulesToText,
  defaultWorkshopDefaults,
  normalizeWorkshopDefaults,
  parseAllowedModulesText,
  type TuiDefaults,
  type WorkshopDefaultsTab,
} from "$lib/types/workshopDefaults";
import { isTauri } from "$lib/window";

export class WorkshopDefaultsStore {
  activeTab = $state<WorkshopDefaultsTab>("setup");
  draft = $state<TuiDefaults>(defaultWorkshopDefaults());
  allowedModulesText = $state("");
  apiKeySet = $state(false);
  apiKeyDraft = $state("");
  clearApiKey = $state(false);

  loading = $state(false);
  saving = $state(false);
  message = $state<string | null>(null);
  loaded = $state(false);

  selectedRouteRole = $state("orchestrator");

  async load() {
    if (!isTauri()) {
      this.loaded = true;
      return;
    }
    this.loading = true;
    this.message = null;
    try {
      const raw = await loadTuiDefaults();
      this.draft = normalizeWorkshopDefaults(raw);
      this.allowedModulesText = allowedModulesToText(this.draft.allowedModules);
      if (!this.draft.stageRouting?.orchestrator?.role) {
        this.draft.stageRouting = defaultStageRouting(
          this.draft.provider ?? "ollama",
          this.draft.model ?? "qwen2.5:7b",
        );
      }
      this.apiKeySet = await messagingSecretStatus("api_key");
      this.apiKeyDraft = "";
      this.clearApiKey = false;
      this.loaded = true;
    } catch (err) {
      this.message = err instanceof Error ? err.message : String(err);
      this.loaded = true;
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

  async save() {
    if (!isTauri()) return;
    this.saving = true;
    this.message = null;
    try {
      const payload: TuiDefaults = {
        ...this.draft,
        baseUrl: this.draft.baseUrl?.trim() || null,
        envOverrides: this.draft.envOverrides?.trim() || null,
        allowedModules: parseAllowedModulesText(this.allowedModulesText),
      };
      if (
        payload.sliceColdWindowTurns != null &&
        payload.sliceHotWindowTurns != null &&
        payload.sliceColdWindowTurns < payload.sliceHotWindowTurns
      ) {
        payload.sliceColdWindowTurns = payload.sliceHotWindowTurns;
      }

      await persistTuiDefaults(payload);

      if (this.clearApiKey) {
        await messagingClearSecret("api_key");
        this.apiKeySet = false;
      } else if (this.apiKeyDraft.trim()) {
        await messagingSaveSecret("api_key", this.apiKeyDraft.trim());
        this.apiKeySet = true;
        this.apiKeyDraft = "";
      }

      runtime.provider = payload.provider ?? runtime.provider;
      runtime.model = payload.model ?? runtime.model;
      if (
        payload.responseDepthMode === "concise" ||
        payload.responseDepthMode === "standard" ||
        payload.responseDepthMode === "deep"
      ) {
        runtime.depthMode = payload.responseDepthMode;
      }
      if (payload.stageRouting) {
        runtime.stageRouting = payload.stageRouting;
      }
      await persistTuiRuntimePrefs(
        runtime.provider,
        runtime.model,
        runtime.depthMode,
        payload.stageRouting ?? undefined,
      );
      runtime.defaultsLoaded = true;

      this.message = "Workshop defaults saved to tui_defaults.json";
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
