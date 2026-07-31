type TraceToken = {
  label: string;
  startedAt: number;
  mark: string;
};

const active = new Map<string, TraceToken>();
const recentLogs = new Map<string, { lastAt: number; suppressed: number }>();
const MAX_ACTIVE_TRACES = 128;

export function traceCodeWorkspaceStart(label: string, workId = ""): string {
  const token = `${label}:${workId}:${Date.now()}:${Math.random().toString(36).slice(2)}`;
  const mark = `medousa-code-${token}`;
  const startedAt = typeof performance !== "undefined" ? performance.now() : Date.now();
  if (active.size >= MAX_ACTIVE_TRACES) {
    const oldest = active.keys().next().value as string | undefined;
    const stale = oldest ? active.get(oldest) : undefined;
    if (oldest) active.delete(oldest);
    if (stale && typeof performance !== "undefined") performance.clearMarks(stale.mark);
  }
  active.set(token, { label, startedAt, mark });
  if (typeof performance !== "undefined") performance.mark(mark);
  return token;
}

export function traceCodeWorkspaceEnd(token: string, detail?: string) {
  const entry = active.get(token);
  if (!entry) return;
  active.delete(token);
  const endedAt = typeof performance !== "undefined" ? performance.now() : Date.now();
  if (typeof performance !== "undefined") performance.clearMarks(entry.mark);
  if (import.meta.env.DEV) {
    const prior = recentLogs.get(entry.label);
    if (prior && endedAt - prior.lastAt < 1_000) {
      prior.suppressed += 1;
      return;
    }
    const suppressed = prior?.suppressed ?? 0;
    recentLogs.set(entry.label, { lastAt: endedAt, suppressed: 0 });
    console.debug(
      `[code-workspace] ${entry.label} ${(endedAt - entry.startedAt).toFixed(1)}ms${detail ? ` · ${detail}` : ""}${suppressed ? ` · ${suppressed} similar events suppressed` : ""}`,
    );
  }
}

/** Defer optional indexing/services until the browser has painted the loaded editor. */
export function deferCodeWorkspaceWork(task: () => void): () => void {
  let cancelled = false;
  const run = () => {
    if (!cancelled) task();
  };
  const idle = (globalThis as typeof globalThis & {
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
    cancelIdleCallback?: (id: number) => void;
  }).requestIdleCallback;
  const cancelIdle = (globalThis as typeof globalThis & {
    requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
    cancelIdleCallback?: (id: number) => void;
  }).cancelIdleCallback;
  if (idle) {
    const id = idle(run, { timeout: 1200 });
    return () => {
      cancelled = true;
      cancelIdle?.(id);
    };
  }
  const id = setTimeout(run, 0);
  return () => {
    cancelled = true;
    clearTimeout(id);
  };
}
