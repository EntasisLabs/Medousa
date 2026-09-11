import { untrack } from "svelte";
import { lookupModelCapabilities } from "$lib/utils/modelCapabilityCatalog";
import { UNKNOWN_REASONING, type ReasoningCapability } from "$lib/types/reasoningEffort";

/** Cache by workshop as the connected daemon owns adapter and catalog versions. */
export class ReasoningCapabilities {
  private entries = $state<Record<string, { capability: ReasoningCapability; expires: number }>>({});
  private pending = new Map<string, Promise<void>>();
  private key(scope: string, provider: string, model: string) { return JSON.stringify([scope, provider, model]); }

  get(scope: string, provider: string, model: string): ReasoningCapability {
    return this.entries[this.key(scope, provider, model)]?.capability ?? UNKNOWN_REASONING;
  }

  async load(scope: string, provider: string, model: string, force = false): Promise<void> {
    if (!scope || !provider.trim() || !model.trim()) return;
    const key = this.key(scope, provider, model);
    if (this.pending.has(key)) return this.pending.get(key);
    if (!force && (this.entries[key]?.expires ?? 0) > Date.now()) return;
    const promise = lookupModelCapabilities(provider, model).then((result) => {
      this.entries[key] = { capability: result.model?.reasoning ?? UNKNOWN_REASONING, expires: Date.now() + 60_000 };
    }).catch(() => {
      // An unreachable or older daemon cannot confirm an override.
      this.entries[key] = { capability: UNKNOWN_REASONING, expires: Date.now() + 5_000 };
    }).finally(() => { this.pending.delete(key); });
    this.pending.set(key, promise);
    return promise;
  }
}
export const reasoningCapabilities = new ReasoningCapabilities();

/** Track identity only; cache writes must not trigger another catalog fetch. */
export function trackReasoningCapabilities(context: () => { provider: string; model: string; scope: string; refresh?: boolean }) {
  let previousIdentity = "";
  $effect(() => {
    const { provider, model, scope, refresh } = context();
    const identity = JSON.stringify([scope, provider, model, refresh]);
    if (identity === previousIdentity) return;
    previousIdentity = identity;
    untrack(() => { void reasoningCapabilities.load(scope, provider, model, refresh); });
  });
}
