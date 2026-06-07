import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";

export interface PollResult<T> {
  value: T | null;
  error: string | null;
}

/** Run parallel polls; keep last good values on partial failure. */
export async function pollAllSettled<T extends Record<string, () => Promise<unknown>>>(
  tasks: T,
  current: { [K in keyof T]: PollResult<Awaited<ReturnType<T[K]>>> },
): Promise<{
  next: { [K in keyof T]: PollResult<Awaited<ReturnType<T[K]>>> };
  failed: string[];
  allFailed: boolean;
}> {
  const entries = Object.entries(tasks) as [keyof T, T[keyof T]][];
  const results = await Promise.allSettled(
    entries.map(([, task]) => task()),
  );

  const next = { ...current } as {
    [K in keyof T]: PollResult<Awaited<ReturnType<T[K]>>>;
  };
  const failed: string[] = [];

  results.forEach((result, index) => {
    const key = entries[index][0];
    if (result.status === "fulfilled") {
      next[key] = {
        value: result.value as Awaited<ReturnType<T[typeof key]>>,
        error: null,
      };
    } else {
      const summary = formatDaemonErrorSummary(result.reason);
      const previous = current[key];
      next[key] = {
        value: previous.value,
        error: summary,
      };
      failed.push(summary);
    }
  });

  return {
    next,
    failed,
    allFailed: failed.length === entries.length,
  };
}
