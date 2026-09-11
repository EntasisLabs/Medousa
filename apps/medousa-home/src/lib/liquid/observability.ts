export interface LiquidPresentationMetrics {
  eligiblePrompts: number;
  renderedEmbeds: number;
  parseFailures: number;
  fallbackRenders: number;
  interactionsDelivered: number;
}

const metrics: LiquidPresentationMetrics = {
  eligiblePrompts: 0,
  renderedEmbeds: 0,
  parseFailures: 0,
  fallbackRenders: 0,
  interactionsDelivered: 0,
};

const ELIGIBLE_INTENT = /\b(recipe|cook|ingredients?|steps?|procedure|compare|versus|decision|metrics?|dashboard|chart|timeline|schedule|plan|options?|choose|image|visual)\b/i;

export function recordLiquidPresentationOpportunity(prompt: string): boolean {
  const eligible = ELIGIBLE_INTENT.test(prompt);
  if (eligible) metrics.eligiblePrompts += 1;
  return eligible;
}

export function recordLiquidMetric(
  metric: "renderedEmbeds" | "parseFailures" | "fallbackRenders" | "interactionsDelivered",
  count = 1,
): void {
  metrics[metric] += Math.max(0, Math.floor(count));
}

export function liquidPresentationMetrics(): LiquidPresentationMetrics & { eligibleUseRate: number } {
  return {
    ...metrics,
    eligibleUseRate: metrics.eligiblePrompts > 0
      ? Math.min(1, metrics.renderedEmbeds / metrics.eligiblePrompts)
      : 0,
  };
}
