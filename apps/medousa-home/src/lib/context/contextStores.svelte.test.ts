/** @vitest-environment happy-dom */
import { beforeEach, expect, it, vi } from "vitest";
import { ContextThreadsStore } from "./contextThreads.svelte";
import { ContextNotesStore } from "./contextNotes.svelte";
import type { LocusNodeDetailResponse } from "$lib/types/locus";
const api = vi.hoisted(() => ({ get: vi.fn(), list: vi.fn(), notes: vi.fn() }));
vi.mock("$lib/daemon", () => ({ getLocusNode: api.get, listLocusNodes: api.list, listVaultNotes: api.notes }));
beforeEach(() => vi.resetAllMocks());
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
function record(key: string): LocusNodeDetailResponse { return { node: { sync_key: key, session_id: "chat-a", tier: "raw", timestamp: "2026-10-17T12:00:00Z", context_summary: key, psi: 0, rho: 0, kappa: 0 }, raw: key }; }

it("keeps the latest selection when moment responses arrive out of order", async () => {
  const store = new ContextThreadsStore();
  const first = deferred<LocusNodeDetailResponse>(), second = deferred<LocusNodeDetailResponse>();
  api.get.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const a = store.loadDetail("a"), b = store.loadDetail("b");
  second.resolve(record("b")); await b;
  first.resolve(record("a")); await a;
  expect(store.detail?.node.sync_key).toBe("b");
  expect(store.detailLoading).toBe(false);
});

it("coalesces a rail and lens detail read and discards a cleared selection", async () => {
  const store = new ContextThreadsStore(), read = deferred<LocusNodeDetailResponse>();
  api.get.mockReturnValue(read.promise);
  const pending = store.loadDetail("a"); void store.loadDetail("a");
  expect(api.get).toHaveBeenCalledOnce();
  store.clearDetail(); read.resolve(record("a")); await pending;
  expect(store.detail).toBeNull();
});

it("does not restore a previous workshop's catalog after a delayed response", async () => {
  const threads = new ContextThreadsStore(), notes = new ContextNotesStore();
  const old = deferred<{ nodes: LocusNodeDetailResponse["node"][] }>();
  const oldNotes = deferred<{ notes: { path: string; title: string; modified_at_utc: string }[] }>();
  api.list.mockReturnValue(old.promise); api.notes.mockReturnValue(oldNotes.promise);
  const a = threads.refresh(), b = notes.refresh();
  threads.resetForWorkshopSwitch(); notes.resetForWorkshopSwitch();
  old.resolve({ nodes: [record("old").node] }); oldNotes.resolve({ notes: [{ path: "old.md", title: "Old", modified_at_utc: "" }] });
  await Promise.all([a, b]);
  expect(threads.nodes).toEqual([]); expect(notes.notes).toEqual([]);
});

it("shares bounded note metadata reads and retains links on failure", async () => {
  const store = new ContextNotesStore();
  const note = { path: "first.md", title: "First", modified_at_utc: "" };
  api.notes.mockResolvedValueOnce({ notes: [note], next_cursor: "page-2" });
  const a = store.refresh(), b = store.refresh(); await Promise.all([a, b]);
  expect(api.notes).toHaveBeenCalledOnce();
  expect(api.notes).toHaveBeenCalledWith({ limit: 200 });
  api.notes.mockRejectedValueOnce(new Error("Unavailable")); await store.refresh();
  expect(store.notes).toEqual([note]); expect(store.error).toBe("Unavailable");
});
