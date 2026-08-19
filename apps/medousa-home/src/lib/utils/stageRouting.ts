import type { StageRoutingMatrix } from "$lib/types/runtime";

function route(
  role: string,
  provider: string,
  model: string,
  policy: string,
  fallback: string,
): StageRoutingMatrix["orchestrator"] {
  return {
    role,
    provider,
    model,
    policy_profile: policy,
    fallback_chain: [fallback, "safe-default"],
  };
}

export function defaultStageRouting(provider: string, model: string): StageRoutingMatrix {
  return {
    orchestrator: route("orchestrator", provider, model, "balanced", "orchestrator"),
    chunker: route("chunker", provider, model, "fast", "chunker"),
    extractor: route("extractor", provider, model, "analytical", "extractor"),
    summarizer: route("summarizer", provider, model, "balanced", "summarizer"),
    verifier: route("verifier", provider, model, "strict", "verifier"),
    packer: route("packer", provider, model, "balanced", "packer"),
    final_response: route("final_response", provider, model, "balanced", "final_response"),
  };
}

function matrixRoutes(matrix: StageRoutingMatrix): StageRoutingMatrix["orchestrator"][] {
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

function sameTarget(
  provider: string,
  model: string,
  otherProvider: string,
  otherModel: string,
): boolean {
  return (
    provider.trim().toLowerCase() === otherProvider.trim().toLowerCase() &&
    model.trim() === otherModel.trim()
  );
}

/** True when every role is the same provider+model pair (a leftover host clone). */
export function uniformStageTarget(
  matrix: StageRoutingMatrix,
): { provider: string; model: string } | null {
  const routes = matrixRoutes(matrix);
  const provider = routes[0]?.provider.trim() ?? "";
  const model = routes[0]?.model.trim() ?? "";
  if (!provider || !model) return null;
  if (!routes.every((entry) => sameTarget(provider, model, entry.provider, entry.model))) {
    return null;
  }
  return { provider, model };
}

/**
 * Keep Chat on the host picker model. A uniform leftover matrix (all DeepSeek
 * after the picker moved to GPT Luna) is rebased; mixed worker roles stay.
 */
export function alignStageRoutingWithHost(
  matrix: StageRoutingMatrix | null | undefined,
  provider: string,
  model: string,
): StageRoutingMatrix {
  const hostProvider = provider.trim();
  const hostModel = model.trim();
  if (!hostProvider || !hostModel) {
    return matrix ?? defaultStageRouting("ollama", "qwen2.5:7b");
  }
  if (!matrix?.orchestrator?.role) {
    return defaultStageRouting(hostProvider, hostModel);
  }
  const uniform = uniformStageTarget(matrix);
  if (uniform && !sameTarget(uniform.provider, uniform.model, hostProvider, hostModel)) {
    return defaultStageRouting(hostProvider, hostModel);
  }
  return {
    ...matrix,
    final_response: {
      ...matrix.final_response,
      provider: hostProvider,
      model: hostModel,
    },
  };
}
