import {
  loadTuiDefaultsSummary,
  persistTuiRuntimePrefs,
} from "$lib/config";
import {
  getContinuationStatus,
  getDeliveryStatus,
  getRuntimeDefaults,
  getRuntimeStats,
  sendRuntimeConfigCommand,
  sendStageRouteCommand,
} from "$lib/daemon";
import type {
  ContinuationStatusResponse,
  DaemonStatsResponse,
  DeliveryHealthResponse,
  DepthMode,
  ReasoningEffortMode,
  RuntimeDefaultsResponse,
  RuntimeTab,
  StageRoutingMatrix,
} from "$lib/types/runtime";
import type { InferenceProfiles } from "$lib/types/inferenceProfiles";
import { normalizeReasoningEffort } from "$lib/types/reasoningEffort";
import { pollAllSettled } from "$lib/utils/poll";
import { isTauriMobilePlatform } from "$lib/platform";
import { isTauri } from "$lib/window";

const DEFAULT_PROVIDER = "ollama";
const DEFAULT_MODEL = "qwen2.5:7b";
const DEFAULT_DEPTH: DepthMode = "standard";
const DEFAULT_REASONING: ReasoningEffortMode = "default";

export class RuntimeStore {
  activeTab = $state<RuntimeTab>("now");
  provider = $state(DEFAULT_PROVIDER);
  model = $state(DEFAULT_MODEL);
  depthMode = $state<DepthMode>(DEFAULT_DEPTH);
  reasoningEffort = $state<ReasoningEffortMode>(DEFAULT_REASONING);
  stageRouting = $state<StageRoutingMatrix>(
    defaultStageRouting(DEFAULT_PROVIDER, DEFAULT_MODEL),
  );
  inferenceProfiles = $state<InferenceProfiles | null>(null);
  defaultsLoaded = $state(false);

  stats = $state<DaemonStatsResponse | null>(null);
  delivery = $state<DeliveryHealthResponse | null>(null);
  continuations = $state<ContinuationStatusResponse | null>(null);

  loading = $state(false);
  error = $state<string | null>(null);
  errorDetail = $state<string | null>(null);
  controlsMessage = $state<string | null>(null);
  savingControls = $state(false);

  private refreshInFlight = false;
  private refreshRequestId = 0;

  modelLabel(): string {
    return `${this.provider}:${this.model}`;
  }

  depthHint(): string {
    if (this.depthMode === "concise") return "short direct answers";
    if (this.depthMode === "deep") return "detailed evidence-forward answers";
    return "balanced answer depth";
  }

  stageRoutes(): Array<StageRoutingMatrix[keyof StageRoutingMatrix]> {
    const matrix = this.stageRouting;
    return [
      matrix.orchestrator,
      matrix.chunker,
      matrix.extractor,
      matrix.summarizer,
      matrix.verifier,
      matrix.packer,
      matrix.final_response,
    ];
  }

  async loadWorkshopRuntime(options?: { connected?: boolean }) {
    if (!isTauri() || this.defaultsLoaded) return;

    if (isTauriMobilePlatform()) {
      if (options?.connected === false) {
        return;
      }
      try {
        const defaults = await getRuntimeDefaults();
        this.applyRuntimeDefaults(defaults);
      } catch {
        // Keep built-in defaults when offline.
      }
      this.defaultsLoaded = true;
      return;
    }

    try {
      const summary = await loadTuiDefaultsSummary();
      this.applyRuntimeDefaults({
        provider: summary.provider?.trim() || DEFAULT_PROVIDER,
        model: summary.model?.trim() || DEFAULT_MODEL,
        response_depth_mode: summary.responseDepthMode ?? DEFAULT_DEPTH,
        reasoning_effort: summary.reasoningEffort ?? DEFAULT_REASONING,
        stage_routing:
          summary.stageRouting?.orchestrator?.role
            ? summary.stageRouting
            : defaultStageRouting(
                summary.provider?.trim() || DEFAULT_PROVIDER,
                summary.model?.trim() || DEFAULT_MODEL,
              ),
      });
    } catch {
      // Local defaults are optional.
    }
    this.defaultsLoaded = true;
  }

  /** @deprecated use loadWorkshopRuntime */
  async loadFromTuiDefaults() {
    return this.loadWorkshopRuntime({ connected: true });
  }

  resetWorkshopRuntime() {
    this.defaultsLoaded = false;
  }

  private applyRuntimeDefaults(
    defaults: Pick<
      RuntimeDefaultsResponse,
      "provider" | "model" | "response_depth_mode" | "reasoning_effort" | "stage_routing"
    > & {
      inference_profiles?: InferenceProfiles | null;
    },
  ) {
    const provider = defaults.provider.trim() || DEFAULT_PROVIDER;
    const model = defaults.model.trim() || DEFAULT_MODEL;
    this.provider = provider;
    this.model = model;
    this.depthMode = normalizeDepth(defaults.response_depth_mode ?? DEFAULT_DEPTH);
    this.reasoningEffort = normalizeReasoningEffort(
      defaults.reasoning_effort ?? DEFAULT_REASONING,
    );
    if (defaults.stage_routing?.orchestrator?.role) {
      this.stageRouting = defaults.stage_routing;
    } else {
      this.stageRouting = defaultStageRouting(provider, model);
    }
    this.inferenceProfiles = defaults.inference_profiles ?? null;
  }

  async refresh() {
    if (this.refreshInFlight) return;

    this.refreshInFlight = true;
    this.loading = true;
    const requestId = ++this.refreshRequestId;

    try {
      const polled = await pollAllSettled(
        {
          stats: () => getRuntimeStats(),
          delivery: () => getDeliveryStatus(),
          continuations: () => getContinuationStatus(),
        },
        {
          stats: { value: this.stats, error: null },
          delivery: { value: this.delivery, error: null },
          continuations: { value: this.continuations, error: null },
        },
      );

      if (requestId !== this.refreshRequestId) return;

      this.stats = polled.next.stats.value;
      this.delivery = polled.next.delivery.value;
      this.continuations = polled.next.continuations.value;

      if (polled.failed.length === 0) {
        this.error = null;
        this.errorDetail = null;
      } else if (polled.allFailed && !this.hasTelemetryData()) {
        this.error = polled.failed[0] ?? "Telemetry unavailable";
        this.errorDetail = polled.failed.join(" · ");
      } else {
        this.error =
          polled.failed.length === 1
            ? polled.failed[0]
            : "Some telemetry endpoints are temporarily unavailable";
        this.errorDetail = polled.failed.join(" · ");
      }
    } finally {
      if (requestId === this.refreshRequestId) {
        this.loading = false;
        this.refreshInFlight = false;
      }
    }
  }

  async refreshStageRoutes() {
    try {
      const response = await sendStageRouteCommand({
        stage_routing: this.stageRouting,
        provider: this.provider,
        model: this.model,
        command: { command: "routes", role: null },
      });
      this.stageRouting = response.stage_routing;
      if (isTauri() && !isTauriMobilePlatform()) {
        await persistTuiRuntimePrefs(
          this.provider,
          this.model,
          this.depthMode,
          this.reasoningEffort,
          this.stageRouting,
        );
      }
    } catch {
      // Keep last known routes — routing refresh is best-effort.
    }
  }

  private hasTelemetryData(): boolean {
    return (
      this.stats !== null || this.delivery !== null || this.continuations !== null
    );
  }

  async applyModel(nextProvider: string, nextModel: string) {
    this.savingControls = true;
    this.controlsMessage = null;
    try {
      const response = await sendRuntimeConfigCommand({
        current_provider: this.provider,
        current_model: this.model,
        draft_provider: this.provider,
        draft_model: this.model,
        current_response_depth_mode: this.depthMode,
        current_reasoning_effort: this.reasoningEffort,
        command: { command: "model", args: [nextProvider.trim(), nextModel.trim()] },
      });
      this.provider = response.next_draft_provider;
      this.model = response.next_draft_model;
      this.depthMode = normalizeDepth(response.next_response_depth_mode);
      this.reasoningEffort = normalizeReasoningEffort(response.next_reasoning_effort);
      this.stageRouting = defaultStageRouting(this.provider, this.model);
      await this.persistSharedDefaults(
        response.should_apply_settings,
        response.should_persist_depth_defaults,
        response.should_persist_reasoning_defaults,
      );
      this.controlsMessage =
        response.rendered_output ?? `Model set to ${this.provider}:${this.model}`;
    } catch (err) {
      this.controlsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      this.savingControls = false;
    }
  }

  async setDepthMode(mode: DepthMode) {
    this.savingControls = true;
    this.controlsMessage = null;
    try {
      const response = await sendRuntimeConfigCommand({
        current_provider: this.provider,
        current_model: this.model,
        draft_provider: this.provider,
        draft_model: this.model,
        current_response_depth_mode: this.depthMode,
        current_reasoning_effort: this.reasoningEffort,
        command: { command: "depth", mode },
      });
      this.provider = response.next_draft_provider;
      this.model = response.next_draft_model;
      this.depthMode = normalizeDepth(response.next_response_depth_mode);
      this.reasoningEffort = normalizeReasoningEffort(response.next_reasoning_effort);
      await this.persistSharedDefaults(
        response.should_apply_settings,
        response.should_persist_depth_defaults,
        response.should_persist_reasoning_defaults,
      );
      this.controlsMessage =
        response.rendered_output ?? `Depth set to ${this.depthMode}`;
    } catch (err) {
      this.controlsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      this.savingControls = false;
    }
  }

  async setReasoningEffort(mode: ReasoningEffortMode) {
    this.savingControls = true;
    this.controlsMessage = null;
    try {
      const response = await sendRuntimeConfigCommand({
        current_provider: this.provider,
        current_model: this.model,
        draft_provider: this.provider,
        draft_model: this.model,
        current_response_depth_mode: this.depthMode,
        current_reasoning_effort: this.reasoningEffort,
        command: { command: "reasoning", mode },
      });
      this.provider = response.next_draft_provider;
      this.model = response.next_draft_model;
      this.depthMode = normalizeDepth(response.next_response_depth_mode);
      this.reasoningEffort = normalizeReasoningEffort(response.next_reasoning_effort);
      await this.persistSharedDefaults(
        response.should_apply_settings,
        response.should_persist_depth_defaults,
        response.should_persist_reasoning_defaults,
      );
      this.controlsMessage =
        response.rendered_output ?? `Reasoning effort set to ${this.reasoningEffort}`;
    } catch (err) {
      this.controlsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      this.savingControls = false;
    }
  }

  private async persistSharedDefaults(
    shouldApplySettings: boolean,
    shouldPersistDepth: boolean,
    shouldPersistReasoning = false,
  ) {
    if (
      !isTauri() ||
      isTauriMobilePlatform() ||
      (!shouldApplySettings && !shouldPersistDepth && !shouldPersistReasoning)
    ) {
      return;
    }
    try {
      await persistTuiRuntimePrefs(
        this.provider,
        this.model,
        this.depthMode,
        shouldPersistReasoning || shouldApplySettings ? this.reasoningEffort : undefined,
        shouldApplySettings ? this.stageRouting : undefined,
      );
    } catch (err) {
      this.controlsMessage =
        err instanceof Error ? err.message : String(err);
    }
  }
}

function normalizeDepth(value: string): DepthMode {
  if (value === "concise" || value === "deep") return value;
  return "standard";
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

export const runtime = new RuntimeStore();
