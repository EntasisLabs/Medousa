import { describe, expect, it, vi } from "vitest";
import { creativeDocumentSession } from "./creativeDocumentSession";
import { creativeSessionPool } from "./creativeSessionPool";
import type { CreativeApi } from "./creativeApi";
import type { CreativeMutationResponse } from "$lib/types/generated/daemon_api";
const response = (revision = 0): CreativeMutationResponse => ({ document: { id: "doc", revision, title: "Release", pinned: false, created_at: "now", updated_at: "now", scene: { schema: "medousa-create", version: 3, background: "transparent", nodes: [] }, assets: {} }, applied_revision: revision, duplicate: false, can_undo: revision > 0, can_redo: false });
function api(scope = "workshop_one") { return { scope, get: vi.fn().mockResolvedValue(response()), transact: vi.fn().mockResolvedValue(response(1)), history: vi.fn().mockResolvedValue(response(2)) } as unknown as CreativeApi; }
describe("creative document session", () => {
  it("retains the exact request and base revision after a lost response", async () => {
    const port = api(); vi.mocked(port.transact).mockRejectedValueOnce(new Error("lost response"));
    const session = creativeDocumentSession(port, "doc"); await session.load();
    const operations = [{ op: "rename" as const, title: "New title" }]; session.stage("Rename", operations);
    operations[0].title = "Caller mutated input";
    const id = session.pending!.id;
    await expect(session.save()).rejects.toThrow("lost response");
    expect(session.pending).toMatchObject({ id, expected_revision: 0, operations: [{ op: "rename", title: "New title" }] });
    await expect(session.load()).rejects.toThrow("pending edit");
    await session.save(); expect(port.transact).toHaveBeenCalledTimes(2);
    expect(vi.mocked(port.transact).mock.calls[0][1]).toEqual(vi.mocked(port.transact).mock.calls[1][1]);
    expect(session.pending).toBeNull(); expect(session.document!.revision).toBe(1);
  });
  it("keeps history request identity too", async () => {
    const port = api(); vi.mocked(port.history).mockRejectedValueOnce(new Error("lost undo"));
    const session = creativeDocumentSession(port, "doc"); await session.load();
    await expect(session.history()).rejects.toThrow("lost undo");
    await expect(session.history(true)).rejects.toThrow("pending history");
    await session.history(); expect(vi.mocked(port.history).mock.calls[0][1]).toEqual(vi.mocked(port.history).mock.calls[1][1]);
    expect(session.pendingHistory).toBeNull();
  });
  it("does not permit replacing an in-flight draft", async () => {
    const port = api(); let finish!: (value: CreativeMutationResponse) => void;
    vi.mocked(port.transact).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const session = creativeDocumentSession(port, "doc"); await session.load(); session.stage("Rename", [{ op: "rename", title: "New" }]);
    const saving = session.save(); expect(() => session.discardPending()).toThrow("in-flight");
    expect(() => session.stage("Other", [])).toThrow("pending edit"); finish(response(1)); await saving;
  });
});
describe("creative session pool", () => {
  it("shares across panes and preserves drafts on release", async () => {
    const pool = creativeSessionPool(1); const port = api();
    const first = pool.acquire(port, "doc"); const second = pool.acquire(port, "doc"); expect(second.session).toBe(first.session);
    await first.session.load(); first.session.stage("Rename", [{ op: "rename", title: "Draft" }]); first.release(); first.release(); second.release();
    expect(pool.acquire(port, "doc").session.pending).not.toBeNull();
    expect(() => pool.acquire(port, "other")).toThrow("existing creative draft");
  });
  it("separates workshops and evicts only unused clean sessions", () => {
    const pool = creativeSessionPool(2); const one = pool.acquire(api("one"), "same"); const two = pool.acquire(api("two"), "same"); expect(one.session).not.toBe(two.session);
    expect(() => pool.acquire(api("three"), "same")).toThrow(); one.release(); pool.acquire(api("three"), "same"); expect(pool.size).toBe(2);
  });
});
