import { codeExecutionScopeKey } from "./codeWorkspaceContext.svelte";

const listeners = new Map<string, Set<() => void>>();
const keyFor = (scope: string, workId: string) => JSON.stringify([scope, workId]);

/** Notification-driven invalidation of aggregate observations, scoped to their daemon/project. */
export function subscribeCodeDiagnostics(workId: string, listener: () => void): () => void {
  const key = keyFor(codeExecutionScopeKey(), workId);
  const subscribers = listeners.get(key) ?? new Set();
  subscribers.add(listener);
  listeners.set(key, subscribers);
  return () => {
    subscribers.delete(listener);
    if (!subscribers.size) listeners.delete(key);
  };
}

export function notifyCodeDiagnostics(scope: string, workId: string) {
  for (const listener of listeners.get(keyFor(scope, workId)) ?? []) listener();
}
