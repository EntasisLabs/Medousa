import type {
  DisposeReason,
  FeatureContext,
  FeatureId,
  FeatureInstance,
  FeatureModuleLoader,
} from "./types";

type LoadRecord = {
  promise: Promise<FeatureInstance>;
  abort: AbortController;
};

const inflight = new Map<FeatureId, LoadRecord>();
const live = new Map<FeatureId, FeatureInstance>();

export class FeatureLoadError extends Error {
  readonly featureId: FeatureId;
  readonly reason: DisposeReason;

  constructor(featureId: FeatureId, reason: DisposeReason, cause?: unknown) {
    super(`feature ${featureId} failed (${reason})`);
    this.featureId = featureId;
    this.reason = reason;
    if (cause !== undefined) this.cause = cause;
  }
}

export async function loadFeature(
  id: FeatureId,
  importModule: FeatureModuleLoader,
  context: Omit<FeatureContext, "signal" | "track"> & { signal?: AbortSignal },
): Promise<FeatureInstance> {
  const existing = live.get(id);
  if (existing) return existing;

  const pending = inflight.get(id);
  if (pending) return pending.promise;

  const abort = new AbortController();
  const onOuterAbort = () => abort.abort(context.signal?.reason);
  context.signal?.addEventListener("abort", onOuterAbort, { once: true });
  if (context.signal?.aborted) abort.abort(context.signal.reason);

  const promise = (async () => {
    let instance: FeatureInstance | undefined;
    try {
      const module = await importModule();
      if (abort.signal.aborted) {
        throw new FeatureLoadError(id, "cancelled");
      }
      instance = await module.start({
        platform: context.platform,
        signal: abort.signal,
        track(partial) {
          instance = partial;
        },
      });
      if (abort.signal.aborted) {
        await instance.dispose("cancelled");
        instance = undefined;
        throw new FeatureLoadError(id, "cancelled");
      }
      live.set(id, instance);
      return instance;
    } catch (error) {
      if (instance) {
        await instance.dispose("start-failed");
      }
      if (error instanceof FeatureLoadError) throw error;
      throw new FeatureLoadError(id, "start-failed", error);
    } finally {
      inflight.delete(id);
      context.signal?.removeEventListener("abort", onOuterAbort);
    }
  })();

  inflight.set(id, { promise, abort });
  return promise;
}

export async function disposeFeature(
  id: FeatureId,
  reason: DisposeReason = "teardown",
): Promise<void> {
  const pending = inflight.get(id);
  if (pending) pending.abort.abort(reason);
  const instance = live.get(id);
  if (!instance) return;
  live.delete(id);
  await instance.dispose(reason);
}

export function loadedFeature(id: FeatureId): FeatureInstance | undefined {
  return live.get(id);
}

export function listLiveFeatureIds(): FeatureId[] {
  return [...live.keys()].sort();
}

export function resetFeaturesForTests(): void {
  inflight.clear();
  live.clear();
}
