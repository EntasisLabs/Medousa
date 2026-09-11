import { describe, expect, it, vi } from "vitest";
import { normalizeReasoningCapability } from "$lib/types/reasoningEffort";
const lookup = vi.hoisted(() => vi.fn());
vi.mock("$lib/utils/modelCapabilityCatalog", () => ({ lookupModelCapabilities: lookup }));
import { ReasoningCapabilities } from "./reasoningCapabilities.svelte";

describe("reasoning capability discovery", () => {
  it("deduplicates pending lookups and isolates late responses by route and workshop", async () => {
    let resolve!: (value: unknown) => void;
    lookup.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const store = new ReasoningCapabilities();
    const first = store.load("one", "openai", "sol");
    const duplicate = store.load("one", "openai", "sol");
    expect(lookup).toHaveBeenCalledTimes(1);
    resolve({ model: { reasoning: normalizeReasoningCapability({ kind: "effort", levels: ["high"] }) } });
    await Promise.all([first, duplicate]);
    expect(store.get("one", "openai", "sol").levels).toEqual(["high"]);
    expect(store.get("two", "openai", "sol").kind).toBe("unknown");
    expect(store.get("one", "openai", "luna").kind).toBe("unknown");
    await store.load("one", "openai", "sol");
    expect(lookup).toHaveBeenCalledTimes(1);
  });
  it("treats an old daemon or failed refresh as unknown", async () => {
    const store = new ReasoningCapabilities();
    lookup.mockResolvedValueOnce({ model: {} });
    await store.load("one", "openai", "sol");
    expect(store.get("one", "openai", "sol").kind).toBe("unknown");
    lookup.mockRejectedValueOnce(new Error("offline"));
    await expect(store.load("one", "openai", "sol", true)).resolves.toBeUndefined();
    expect(store.get("one", "openai", "sol").levels).toEqual([]);
  });
});
