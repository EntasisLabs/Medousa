import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultStageRouting } from "$lib/utils/stageRouting";

const environment = vi.hoisted(() => ({ mobile: false }));
vi.mock("$lib/platform", () => ({
  homeChannelSurface: () => "desktop",
  isTauriMobilePlatform: () => environment.mobile,
}));
vi.mock("$lib/stores/runtime.svelte", () => ({
  runtime: { provider: "openai-chatgpt", model: "sol", stageRouting: null, defaultsLoaded: true,
    depthMode: "deep", reasoningEffort: "high" },
}));
vi.mock("$lib/stores/governedBrowser.svelte", () => ({ governedBrowser: {} }));
vi.mock("$lib/stores/userProfiles.svelte", () => ({
  userProfiles: { turnIdentityUserId: () => "user-1" },
}));

import { runtime } from "$lib/stores/runtime.svelte";
import { sessionModelSelections, type ChatModelContext } from "$lib/chat/sessionModelSelection.svelte";
import { reasoningCapabilities } from "$lib/chat/reasoningCapabilities.svelte";
import { normalizeReasoningCapability } from "$lib/types/reasoningEffort";
import { buildInteractiveTurnOptions, prepareInteractiveTurnOptions } from "./interactiveTurnOptions";

let sequence = 0;
function chat(): ChatModelContext {
  return { sessionId: `session-${++sequence}`, workshopScopeId: "workshop", messages: [] };
}

describe("interactive chat model routing", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    });
    environment.mobile = false;
    runtime.provider = "openai-chatgpt";
    runtime.model = "sol";
    runtime.stageRouting = defaultStageRouting(runtime.provider, runtime.model);
    runtime.defaultsLoaded = true;
  });
  afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it("pins the first submitted model and routing, independent of other chats and defaults", () => {
    const coder = chat();
    const fresh = chat();
    const first = buildInteractiveTurnOptions(coder);
    sessionModelSelections.set(fresh, {
      provider: "openai", model: "luna", stageRouting: defaultStageRouting("openai", "luna"),
    });
    expect(buildInteractiveTurnOptions(fresh)).toMatchObject({ provider: "openai", model: "luna" });
    expect(runtime.model).toBe("sol");
    runtime.model = "astra";
    runtime.stageRouting = defaultStageRouting(runtime.provider, "astra");
    const next = buildInteractiveTurnOptions(coder);
    expect(next).toEqual(first);
    expect(next.stageRouting?.final_response.model).toBe("sol");
    expect(next).toMatchObject({ responseDepthMode: "deep", reasoningEffort: "default", identityUserId: "user-1" });
  });

  it("prepares the saved model effort without leaking another chat or global default", async () => {
    const context = chat();
    const selection = { provider: "openai", model: "sol", stageRouting: defaultStageRouting("openai", "sol") };
    sessionModelSelections.setReasoning(context, selection, "none");
    vi.spyOn(reasoningCapabilities, "load").mockResolvedValue();
    vi.spyOn(reasoningCapabilities, "get").mockReturnValue(normalizeReasoningCapability({ kind: "effort", levels: ["none", "high"] }));
    runtime.reasoningEffort = "high";
    expect((await prepareInteractiveTurnOptions(context)).reasoningEffort).toBe("none");
    sessionModelSelections.setReasoning(context, selection, "max");
    expect((await prepareInteractiveTurnOptions(context)).reasoningEffort).toBe("default");
  });

  it("cancels preparation if the user switches conversations during discovery", async () => {
    const context = chat();
    let done!: () => void;
    vi.spyOn(reasoningCapabilities, "load").mockImplementation(() => new Promise((resolve) => { done = resolve; }));
    const pending = prepareInteractiveTurnOptions(context);
    context.sessionId = "switched";
    done();
    await expect(pending).rejects.toThrow("Conversation changed");
  });

  it("defers uninitialized mobile defaults but sends explicit session choices", () => {
    environment.mobile = true;
    runtime.defaultsLoaded = false;
    const session = chat();
    expect(buildInteractiveTurnOptions(session).model).toBeUndefined();
    expect(sessionModelSelections.get(session)).toBeNull();
    sessionModelSelections.set(session, {
      provider: "openai", model: "luna", stageRouting: defaultStageRouting("openai", "luna"),
    });
    expect(buildInteractiveTurnOptions(session)).toMatchObject({ provider: "openai", model: "luna" });
  });

  it("sends the recovered main-chat receipt instead of the newly loaded default", () => {
    const session = chat();
    session.messages = [{ id: "reply", role: "assistant", content: "", responseProvider: "openai", responseModel: "previous" }];
    expect(buildInteractiveTurnOptions(session)).toMatchObject({ provider: "openai", model: "previous" });
    expect(sessionModelSelections.get(session)?.stageRouting.final_response.model).toBe("previous");
  });
});
