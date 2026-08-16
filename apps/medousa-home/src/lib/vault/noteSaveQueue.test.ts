import { describe, expect, it, vi } from "vitest";
import { NoteSaveQueue, type NoteSaveJob, type NoteSaveResult } from "./noteSaveQueue";

describe("NoteSaveQueue", () => {
  it("serializes saves per path", async () => {
    const order: string[] = [];
    let releaseFirst!: () => void;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });

    const queue = new NoteSaveQueue(async (_path, job) => {
      order.push(`start:${job.content}`);
      if (job.content === "a") await firstGate;
      order.push(`end:${job.content}`);
      return { ok: true, contentHash: `h-${job.content}` };
    });

    const p1 = queue.enqueue("n.md", {
      content: "a",
      contentHash: "h0",
      force: false,
      source: "autosave",
    });
    const p2 = queue.enqueue("n.md", {
      content: "b",
      contentHash: "h0",
      force: false,
      source: "autosave",
    });

    expect(order).toEqual(["start:a"]);
    releaseFirst();
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.ok).toBe(true);
    expect(r2.ok).toBe(true);
    expect(order).toEqual(["start:a", "end:a", "start:b", "end:b"]);
  });

  it("uses hash from prior success for coalesced follow-up", async () => {
    const hashes: Array<string | null> = [];
    let releaseFirst!: () => void;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });

    const queue = new NoteSaveQueue(async (_path, job) => {
      hashes.push(job.contentHash);
      if (job.content === "a") await firstGate;
      return { ok: true, contentHash: "h-after-a" };
    });

    const p1 = queue.enqueue("n.md", {
      content: "a",
      contentHash: "stale",
      force: false,
      source: "autosave",
    });
    const p2 = queue.enqueue("n.md", {
      content: "b",
      contentHash: "still-stale",
      force: false,
      source: "manual",
    });

    releaseFirst();
    await Promise.all([p1, p2]);
    expect(hashes).toEqual(["stale", "h-after-a"]);
  });

  it("keeps paths independent", async () => {
    const run = vi.fn(async (_path: string, job: NoteSaveJob): Promise<NoteSaveResult> => ({
      ok: true,
      contentHash: job.contentHash,
    }));
    const queue = new NoteSaveQueue(run);
    await Promise.all([
      queue.enqueue("a.md", {
        content: "1",
        contentHash: null,
        force: false,
        source: "manual",
      }),
      queue.enqueue("b.md", {
        content: "2",
        contentHash: null,
        force: false,
        source: "manual",
      }),
    ]);
    expect(run).toHaveBeenCalledTimes(2);
  });
});
