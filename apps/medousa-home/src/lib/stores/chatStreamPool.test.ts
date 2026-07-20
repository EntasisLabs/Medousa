import { beforeEach, describe, expect, it } from "vitest";
import { ChatStreamPool } from "./chatStreamPool.svelte";

describe("ChatStreamPool", () => {
  let pool: ChatStreamPool;

  beforeEach(() => {
    pool = new ChatStreamPool();
  });

  it("acquires a live slot", () => {
    pool.acquire("a");
    expect(pool.isLive("a")).toBe(true);
    expect(pool.liveSessionIds).toEqual(["a"]);
  });

  it("evicts to cached when maxLive is 1", () => {
    pool.acquire("a");
    const demoted = pool.acquire("b");
    expect(pool.isLive("b")).toBe(true);
    expect(pool.isLive("a")).toBe(false);
    expect(pool.get("a")?.status).toBe("cached");
    expect(demoted).toContain("a");
  });

  it("release demotes to cached", () => {
    pool.acquire("a");
    pool.release("a");
    expect(pool.get("a")?.status).toBe("cached");
    expect(pool.isLive("a")).toBe(false);
  });

  it("setMaxLive(2) allows two live streams", () => {
    pool.setMaxLive(2);
    pool.acquire("a");
    pool.acquire("b");
    expect(pool.liveSessionIds.sort()).toEqual(["a", "b"]);
    const demoted = pool.acquire("c");
    expect(pool.isLive("c")).toBe(true);
    expect(demoted.length).toBe(1);
    expect(pool.liveSessionIds).toHaveLength(2);
  });

  it("re-acquire refreshes live without demoting self", () => {
    pool.acquire("a");
    const demoted = pool.acquire("a");
    expect(demoted).toEqual([]);
    expect(pool.isLive("a")).toBe(true);
  });
});
