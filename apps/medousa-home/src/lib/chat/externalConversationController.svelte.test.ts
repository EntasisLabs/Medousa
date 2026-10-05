/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import ExternalConversationHarness from "./test-fixtures/ExternalConversationHarness.svelte";
import type { ExternalConversation } from "$lib/daemon/externalConversations";

const api = vi.hoisted(() => ({ list: vi.fn(), send: vi.fn(), getSelection: vi.fn(), setSelection: vi.fn() }));
vi.mock("$lib/daemon/externalConversations", () => ({ listExternalConversations: api.list, sendExternalConversationMessage: api.send }));
vi.mock("$lib/utils/externalConversationSelection", () => ({ getExternalConversationSelection: api.getSelection, setExternalConversationSelection: api.setSelection }));
import { createExternalConversationController } from "./externalConversationController.svelte";

const conversation = (id: string, provider = "muse"): ExternalConversation => ({
  id, provider, label: id, messages: [], events: [],
} as unknown as ExternalConversation);
let component: ReturnType<typeof mount> | undefined;
function controller(scope: () => string = () => "mac", sessionId = "main") {
  let result!: ReturnType<typeof createExternalConversationController>;
  const target = document.createElement("div"); document.body.append(target);
  component = mount(ExternalConversationHarness, { target, props: {
    input: { provider: () => "muse", sessionId: () => sessionId, scope, offline: () => false, visible: () => true },
    ready: (value) => { result = value; },
  } });
  flushSync();
  return result;
}
beforeEach(() => { vi.clearAllMocks(); api.getSelection.mockReturnValue(null); });
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });

describe("external conversation selection shared by mobile composer and transcript", () => {
  it("sends to the chosen Muse session and exposes the returned transcript", async () => {
    api.list.mockResolvedValue([conversation("first"), conversation("second"), conversation("grok", "grok_bot")]);
    const state = controller();
    await state.refresh();
    expect(state.choices.map((item) => item.id)).toEqual(["first", "second"]);
    expect(state.selectedId).toBeNull();
    state.select("second");
    const updated = { ...conversation("second"), label: "updated after send" };
    api.send.mockResolvedValue(updated);
    await state.send("hello", false);
    expect(api.send).toHaveBeenCalledWith("second", "hello", expect.any(String));
    expect(state.selected?.label).toBe("updated after send");
    expect(api.setSelection).toHaveBeenCalledWith("main", "muse", "second");
  });

  it("preserves selection after a failed send and rejects attachments without sending", async () => {
    api.list.mockResolvedValue([conversation("only")]);
    const state = controller();
    await state.refresh();
    await expect(state.send("hello", true)).rejects.toThrow("text only");
    expect(api.send).not.toHaveBeenCalled();
    api.send.mockRejectedValue(new Error("delivery uncertain"));
    await expect(state.send("hello", false)).rejects.toThrow("delivery uncertain");
    expect(state.selectedId).toBe("only");
    expect(state.error).toBe("delivery uncertain");
    expect(state.busy).toBe(false);
  });

  it("does not import the Mac session list after switching to embedded Personal", async () => {
    let scope = "mac";
    let resolve!: (value: ExternalConversation[]) => void;
    api.list.mockReturnValue(new Promise<ExternalConversation[]>((done) => { resolve = done; }));
    const state = controller(() => scope);
    const pending = state.refresh();
    scope = "embedded-personal";
    resolve([conversation("mac-only")]);
    await pending;
    expect(state.conversations).toEqual([]);
    expect(state.selectedId).toBeNull();
    expect(api.setSelection).not.toHaveBeenCalled();
  });
  it("reopens an attached conversation and cannot retarget it", async () => {
    api.getSelection.mockReturnValue("second");
    api.list.mockResolvedValue([conversation("first"), conversation("second")]);
    const state = controller(() => "mac", "external-conversation:muse:second");
    await state.refresh();
    state.select("first");
    expect(state.selectedId).toBe("second");
    api.send.mockResolvedValue(conversation("second"));
    await state.send("hello", false);
    expect(api.send).toHaveBeenCalledWith("second", "hello", expect.any(String));
  });

  it("fails closed when an attached conversation is removed instead of messaging the only remaining agent", async () => {
    api.getSelection.mockReturnValue("removed");
    api.list.mockResolvedValue([conversation("another-agent")]);
    const state = controller(() => "mac", "external-conversation:muse:removed");
    await state.refresh();
    expect(state.selectedId).toBe("removed");
    expect(state.selected).toBeNull();
    await expect(state.send("private request", false)).rejects.toThrow("connected agent");
    expect(api.send).not.toHaveBeenCalled();
  });

});
