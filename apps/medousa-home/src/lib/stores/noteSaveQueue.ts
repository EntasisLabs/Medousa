/** Per-path serialized vault saves — one in-flight PUT; coalesce superseding bodies. */

export type NoteSaveJob = {
  content: string;
  contentHash: string | null;
  force: boolean;
  source: "manual" | "autosave";
};

export type NoteSaveResult = {
  ok: boolean;
  /** Hash after a successful write (for the next coalesced attempt). */
  contentHash?: string | null;
  /** Body actually written (tag rewrite echo). */
  writtenContent?: string;
  conflict?: boolean;
  error?: string;
};

type Runner = (path: string, job: NoteSaveJob) => Promise<NoteSaveResult>;

/**
 * Serializes saves per path. While a PUT is in flight, newer jobs replace the
 * pending body (coalesce). After success, If-Match for the next attempt uses
 * the hash returned from the prior write — never a stale pre-write hash.
 */
export class NoteSaveQueue {
  private inflight = new Map<string, Promise<NoteSaveResult>>();
  private pending = new Map<string, NoteSaveJob>();

  constructor(private readonly run: Runner) {}

  /** True when a PUT is in flight or a coalesced job is waiting for that path. */
  isBusy(path: string): boolean {
    return this.inflight.has(path) || this.pending.has(path);
  }

  enqueue(path: string, job: NoteSaveJob): Promise<NoteSaveResult> {
    const key = path.trim();
    if (!key) {
      return Promise.resolve({ ok: false, error: "path is required" });
    }

    const existing = this.inflight.get(key);
    if (existing) {
      const prior = this.pending.get(key);
      this.pending.set(key, mergeJobs(prior, job));
      return existing.then(async (first) => {
        const next = this.pending.get(key);
        if (!next) return first;
        this.pending.delete(key);
        // After a successful write, prefer the new hash for If-Match.
        const hash =
          first.ok && first.contentHash != null && !next.force
            ? first.contentHash
            : next.contentHash;
        return this.enqueue(key, { ...next, contentHash: hash });
      });
    }

    const chain = this.run(key, job).finally(() => {
      if (this.inflight.get(key) === chain) this.inflight.delete(key);
    });
    this.inflight.set(key, chain);
    return chain;
  }
}

function mergeJobs(prior: NoteSaveJob | undefined, next: NoteSaveJob): NoteSaveJob {
  if (!prior) return next;
  return {
    content: next.content,
    contentHash: next.force ? null : (prior.contentHash ?? next.contentHash),
    force: prior.force || next.force,
    source: next.source === "manual" || prior.source === "manual" ? "manual" : "autosave",
  };
}
