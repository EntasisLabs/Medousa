import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultStageRouting } from "$lib/utils/stageRouting";
import {
  SessionModelSelections,
  type ChatModelContext,
} from "./sessionModelSelection.svelte";

function selection(model: string, provider = "openai-chatgpt") {
  return { provider, model, stageRouting: defaultStageRouting(provider, model) };
}
function context(sessionId = "coder", workshopScopeId = "workshop-a"): ChatModelContext {
  return { sessionId, workshopScopeId, messages: [] };
}

describe("chat model selections", () => {
  let values: Map<string, string>;
  beforeEach(() => {
    values = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      removeItem: (key: string) => values.delete(key),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("restores Sol after selecting Luna in another chat, including after reload", () => {
    const store = new SessionModelSelections();
    const sol = selection("gpt-5.6-sol");
    store.set(context(), sol);
    store.set(context("new-chat"), selection("gpt-5.6-luna"));
    expect(store.resolve(context(), sol)?.model).toBe("gpt-5.6-sol");
    const reloaded = new SessionModelSelections();
    expect(reloaded.resolve(context(), selection("other"))).toEqual(sol);
    expect(reloaded.get(context("new-chat"))?.model).toBe("gpt-5.6-luna");
    expect(reloaded.resolve(context("fresh"), sol)).toEqual(sol);
  });

  it("isolates workshop scopes, even when session ids match", () => {
    const store = new SessionModelSelections();
    store.set(context(), selection("sol"));
    store.set(context("coder", "workshop-b"), selection("luna", "openai"));
    expect(store.get(context())?.model).toBe("sol");
    expect(store.get(context("coder", "workshop-b"))?.provider).toBe("openai");
  });

  it("recovers the latest principal receipt but keeps an explicit pick through hydration", () => {
    const store = new SessionModelSelections();
    const chat = context();
    chat.messages = [
      { id: "1", role: "assistant", content: "", responseProvider: "openai", responseModel: "old" },
      { id: "2", role: "assistant", content: "", responseProvider: "openai-chatgpt", responseModel: "sol" },
      { id: "3", role: "assistant", lane: "worker", content: "", responseProvider: "openai", responseModel: "worker-model" },
      { id: "4", role: "assistant", lane: "ask", content: "", responseProvider: "openai", responseModel: "ask-model" },
    ];
    expect(store.resolve(chat, selection("luna"))).toEqual(selection("sol"));
    store.set(chat, selection("astra"));
    expect(store.resolve(chat, selection("luna"))).toEqual(selection("astra"));
  });

  it("snapshots custom routing without retaining references to workshop defaults", () => {
    const store = new SessionModelSelections();
    const defaults = selection("sol");
    defaults.stageRouting.extractor.model = "luna";
    store.set(context(), defaults);
    defaults.model = "astra";
    defaults.stageRouting.extractor.model = "changed";
    expect(store.get(context())?.model).toBe("sol");
    expect(store.get(context())?.stageRouting.extractor.model).toBe("luna");
    expect(new SessionModelSelections().get(context())).toEqual(store.get(context()));
  });

  it("does not invent a route before defaults arrive, but honors an explicit selection", () => {
    const store = new SessionModelSelections();
    expect(store.resolve(context(), null)).toBeNull();
    store.set(context(), selection("sol"));
    expect(store.resolve(context(), null)?.model).toBe("sol");
  });

  it("ignores damaged or unsupported stored values and removes deleted preferences", () => {
    const store = new SessionModelSelections();
    store.set(context(), selection("sol"));
    const key = [...values.keys()][0];
    for (const raw of ["broken", "null", '{"version":99}', JSON.stringify({version: 1, provider: "openai", model: "sol", stageRouting: {}})]) {
      values.set(key, raw);
      expect(new SessionModelSelections().resolve(context(), selection("luna"))?.model).toBe("luna");
    }
    store.clear(context());
    expect(store.get(context())).toBeNull();
    expect(new SessionModelSelections().get(context())).toBeNull();
  });

  it("retains the choice in memory if browser storage is unavailable", () => {
    vi.stubGlobal("localStorage", {
      getItem() { throw new Error("unavailable"); },
      setItem() { throw new Error("quota"); },
      removeItem() { throw new Error("unavailable"); },
    });
    const store = new SessionModelSelections();
    expect(store.get(context())).toBeNull();
    store.set(context(), selection("sol"));
    expect(store.get(context())?.model).toBe("sol");
    store.clear(context());
    expect(store.get(context())).toBeNull();
  });
});
