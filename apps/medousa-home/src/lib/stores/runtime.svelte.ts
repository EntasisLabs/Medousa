import {
  getContinuationStatus,
  getDeliveryStatus,
  getRuntimeStats,
  sendRuntimeConfigCommand,
  sendStageRouteCommand,
} from "$lib/daemon";
import type {
  ContinuationStatusResponse,
  DaemonStatsResponse,
  DeliveryHealthResponse,
  DepthMode,
  RuntimeTab,
  StageRoutingMatrix,
} from "$lib/types/runtime";

const PROVIDER_KEY = "medousa-home-provider";
const MODEL_KEY = "medousa-home-model";
const DEPTH_KEY = "medousa-home-depth-mode";
const ROUTING_KEY = "medousa-home-stage-routing";

const DEFAULT_PROVIDER = "ollama";
const DEFAULT_MODEL = "qwen2.5:7b";
const DEFAULT_DEPTH: DepthMode = "standard";

export class RuntimeStore {
  activeTab = $state<RuntimeTab>("now");
  provider = $state(loadProvider());
  model = $state(loadModel());
  depthMode = $state<DepthMode>(loadDepthMode());
  stageRouting = $state<StageRoutingMatrix>(loadStageRouting());

  stats = $state<DaemonStatsResponse | null>(null);
  delivery = $state<DeliveryHealthResponse | null>(null);
  continuations = $state<ContinuationStatusResponse | null>(null);

  loading = $state(false);
  error = $state<string | null>(null);
  controlsMessage = $state<string | null>(null);
  savingControls = $state(false);

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

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const [stats, delivery, continuations] = await Promise.all([
        getRuntimeStats(),
        getDeliveryStatus(),
        getContinuationStatus(),
      ]);
      this.stats = stats;
      this.delivery = delivery;
      this.continuations = continuations;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
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
      persistStageRouting(this.stageRouting);
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    }
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
        command: { command: "model", args: [nextProvider.trim(), nextModel.trim()] },
      });
      this.provider = response.next_draft_provider;
      this.model = response.next_draft_model;
      this.depthMode = normalizeDepth(response.next_response_depth_mode);
      persistRuntimePrefs(this.provider, this.model, this.depthMode);
      this.stageRouting = defaultStageRouting(this.provider, this.model);
      persistStageRouting(this.stageRouting);
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
        command: { command: "depth", mode },
      });
      this.provider = response.next_draft_provider;
      this.model = response.next_draft_model;
      this.depthMode = normalizeDepth(response.next_response_depth_mode);
      persistRuntimePrefs(this.provider, this.model, this.depthMode);
      this.controlsMessage =
        response.rendered_output ?? `Depth set to ${this.depthMode}`;
    } catch (err) {
      this.controlsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      this.savingControls = false;
    }
  }
}

function normalizeDepth(value: string): DepthMode {
  if (value === "concise" || value === "deep") return value;
  return "standard";
}

function loadProvider(): string {
  if (typeof localStorage === "undefined") return DEFAULT_PROVIDER;
  return localStorage.getItem(PROVIDER_KEY)?.trim() || DEFAULT_PROVIDER;
}

function loadModel(): string {
  if (typeof localStorage === "undefined") return DEFAULT_MODEL;
  return localStorage.getItem(MODEL_KEY)?.trim() || DEFAULT_MODEL;
}

function loadDepthMode(): DepthMode {
  if (typeof localStorage === "undefined") return DEFAULT_DEPTH;
  const stored = localStorage.getItem(DEPTH_KEY);
  if (stored === "concise" || stored === "deep") return stored;
  return DEFAULT_DEPTH;
}

function loadStageRouting(): StageRoutingMatrix {
  if (typeof localStorage === "undefined") {
    return defaultStageRouting(DEFAULT_PROVIDER, DEFAULT_MODEL);
  }
  try {
    const raw = localStorage.getItem(ROUTING_KEY);
    if (!raw) return defaultStageRouting(loadProvider(), loadModel());
    const parsed = JSON.parse(raw) as StageRoutingMatrix;
    if (parsed?.orchestrator?.role) return parsed;
  } catch {
    // fall through
  }
  return defaultStageRouting(loadProvider(), loadModel());
}

function persistRuntimePrefs(provider: string, model: string, depth: DepthMode) {
  localStorage.setItem(PROVIDER_KEY, provider);
  localStorage.setItem(MODEL_KEY, model);
  localStorage.setItem(DEPTH_KEY, depth);
}

function persistStageRouting(matrix: StageRoutingMatrix) {
  localStorage.setItem(ROUTING_KEY, JSON.stringify(matrix));
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
